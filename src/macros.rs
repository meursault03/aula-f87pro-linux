use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::{self, ErrorKind};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MacroEvent {
    pub key: u8,
    pub pressed: bool,
    pub delay_ms: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MacroDefinition {
    pub name: String,
    pub events: Vec<MacroEvent>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

// Shipped encoder 0x48a310: UTF-16LE name, followed by four-byte events.
pub fn encode_definition(definition: &MacroDefinition) -> io::Result<Vec<u8>> {
    let name: Vec<u16> = definition.name.encode_utf16().collect();
    if name.is_empty() || name.len() > 30 || name.contains(&0) {
        return Err(invalid(
            "macro name must contain 1..30 UTF-16 units without NUL",
        ));
    }
    if definition.events.is_empty() || definition.events.len() > 1000 {
        return Err(invalid("macro must contain 1..1000 events"));
    }
    let mut bytes = vec![(name.len() * 2) as u8];
    for unit in name {
        bytes.extend(unit.to_le_bytes());
    }
    let mut pressed = BTreeSet::new();
    for event in &definition.events {
        if !matches!(event.key, 4..=0x73 | 0xe0..=0xe7) || event.delay_ms > 0xfffff {
            return Err(invalid(
                "invalid macro HID key or delay (maximum 1048575 ms)",
            ));
        }
        if if event.pressed {
            !pressed.insert(event.key)
        } else {
            !pressed.remove(&event.key)
        } {
            return Err(invalid("macro key presses and releases must be paired"));
        }
        bytes.extend([
            0x10 | if event.pressed { 0 } else { 0x80 } | (event.delay_ms >> 16) as u8,
            (event.delay_ms >> 8) as u8,
            event.delay_ms as u8,
            event.key,
        ]);
    }
    if !pressed.is_empty() {
        return Err(invalid("macro must release every pressed key"));
    }
    Ok(bytes)
}

pub fn decode_definition(bytes: &[u8]) -> io::Result<MacroDefinition> {
    let name_len = usize::from(*bytes.first().ok_or_else(|| invalid("empty macro"))?);
    if name_len > 60
        || name_len % 2 != 0
        || bytes.len() < name_len + 1
        || !(bytes.len() - name_len - 1).is_multiple_of(4)
    {
        return Err(invalid("invalid macro name or event length"));
    }
    let name = String::from_utf16(
        &bytes[1..1 + name_len]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>(),
    )
    .map_err(|_| invalid("invalid macro UTF-16 name"))?;
    let mut events = Vec::new();
    for event in bytes[1 + name_len..].as_chunks::<4>().0.iter() {
        // shortcut: existing mouse events remain opaque; expose them when a mouse editor is added.
        if event[0] & 0x70 != 0x10 {
            return Err(invalid("macro includes unsupported non-keyboard events"));
        }
        events.push(MacroEvent {
            key: event[3],
            pressed: event[0] & 0x80 == 0,
            delay_ms: (u32::from(event[0] & 0x0f) << 16)
                | (u32::from(event[1]) << 8)
                | u32::from(event[2]),
        });
    }
    let definition = MacroDefinition { name, events };
    encode_definition(&definition)?;
    Ok(definition)
}

// FillMatrix 0x48a6b0 packs N offset/length u16LE entries before N opaque blobs.
pub fn macro_blobs(buffer: &[u8; 4096]) -> io::Result<Vec<Vec<u8>>> {
    if buffer.iter().all(|byte| *byte == 0) || buffer.iter().all(|byte| *byte == 0xff) {
        return Ok(Vec::new());
    }
    let first = usize::from(u16::from_le_bytes([buffer[0], buffer[1]]));
    if first == 0 || first % 4 != 0 || first / 4 > 200 {
        return Err(invalid("unrecognized macro table; existing data preserved"));
    }
    let mut blobs = Vec::new();
    let mut previous_end = first;
    for entry in buffer[..first].as_chunks::<4>().0.iter() {
        let offset = usize::from(u16::from_le_bytes([entry[0], entry[1]]));
        let length = usize::from(u16::from_le_bytes([entry[2], entry[3]]));
        if offset != previous_end || length == 0 || offset + length > buffer.len() {
            return Err(invalid(
                "invalid macro table bounds; existing data preserved",
            ));
        }
        let blob = &buffer[offset..offset + length];
        let name_len = usize::from(blob[0]);
        if name_len > 60
            || name_len % 2 != 0
            || length < 1 + name_len
            || (length - name_len - 1) % 4 != 0
        {
            return Err(invalid(
                "invalid macro blob length; existing data preserved",
            ));
        }
        blobs.push(blob.to_vec());
        previous_end = offset + length;
    }
    Ok(blobs)
}

pub fn replace_macro(
    buffer: &[u8; 4096],
    slot: usize,
    definition: &MacroDefinition,
) -> io::Result<[u8; 4096]> {
    let mut blobs = macro_blobs(buffer)?;
    if slot > blobs.len() || slot >= 200 {
        return Err(invalid(
            "macro slot must exist or append immediately after existing slots",
        ));
    }
    let encoded = encode_definition(definition)?;
    if slot == blobs.len() {
        blobs.push(encoded);
    } else {
        blobs[slot] = encoded;
    }
    let mut offset = blobs.len() * 4;
    let total = offset + blobs.iter().map(Vec::len).sum::<usize>();
    if total > buffer.len() {
        return Err(invalid("macro memory is full (4096 bytes)"));
    }
    let mut result = [0; 4096];
    for (index, blob) in blobs.iter().enumerate() {
        result[index * 4..index * 4 + 2].copy_from_slice(&(offset as u16).to_le_bytes());
        result[index * 4 + 2..index * 4 + 4].copy_from_slice(&(blob.len() as u16).to_le_bytes());
        result[offset..offset + blob.len()].copy_from_slice(blob);
        offset += blob.len();
    }
    Ok(result)
}

// Converter 0x489c50: macro mode/count in bytes 1/2, FillMatrix inserts slot in byte 3.
pub fn macro_binding(slot: u8, mode: u8, repeat: u8) -> io::Result<[u8; 4]> {
    if slot >= 200 || !matches!((mode, repeat), (1, 1..=255) | (2 | 4, 1)) {
        return Err(invalid("invalid macro playback mode or repetition count"));
    }
    Ok([3, mode, repeat, slot])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> MacroDefinition {
        MacroDefinition {
            name: name.into(),
            events: vec![
                MacroEvent {
                    key: 4,
                    pressed: true,
                    delay_ms: 0xabcde,
                },
                MacroEvent {
                    key: 4,
                    pressed: false,
                    delay_ms: 12,
                },
            ],
        }
    }

    #[test]
    fn encodes_shipped_keyboard_event_layout() {
        let definition = sample("A");
        let bytes = encode_definition(&definition).unwrap();
        assert_eq!(bytes, [2, b'A', 0, 0x1a, 0xbc, 0xde, 4, 0x90, 0, 12, 4]);
        assert_eq!(decode_definition(&bytes).unwrap(), definition);
    }

    #[test]
    fn replacement_preserves_opaque_other_slots() {
        let mut buffer = replace_macro(&[0; 4096], 0, &sample("primeira")).unwrap();
        let first_event = 4 + 1 + "primeira".len() * 2;
        buffer[first_event] = 0x20;
        let original = macro_blobs(&buffer).unwrap();
        assert!(decode_definition(&original[0]).is_err());
        let buffer = replace_macro(&buffer, 1, &sample("segunda")).unwrap();
        let replacement = replace_macro(&buffer, 1, &sample("curta")).unwrap();
        let blobs = macro_blobs(&replacement).unwrap();
        assert_eq!(blobs.len(), 2);
        assert_eq!(blobs[0], original[0]);
        assert_eq!(decode_definition(&blobs[1]).unwrap().name, "curta");
        assert_eq!(&replacement[..2], &[8, 0]);
        assert!(replace_macro(&buffer, 3, &sample("salto")).is_err());
    }

    #[test]
    fn rejects_unbalanced_events_and_unsafe_tables() {
        let mut definition = sample("A");
        definition.events.pop();
        assert!(encode_definition(&definition).is_err());
        definition.events[0].delay_ms = 0x100000;
        assert!(encode_definition(&definition).is_err());
        assert!(decode_definition(&[3, 65, 0, 0]).is_err());
        assert!(macro_blobs(&[0xff; 4096]).unwrap().is_empty());
        let mut corrupt = [0; 4096];
        corrupt[..4].copy_from_slice(&[5, 0, 20, 0]);
        assert!(macro_blobs(&corrupt).is_err());
        corrupt[..4].copy_from_slice(&[4, 0, 0xff, 0xff]);
        assert!(macro_blobs(&corrupt).is_err());
        assert_eq!(macro_binding(2, 1, 3).unwrap(), [3, 1, 3, 2]);
        assert!(macro_binding(2, 2, 2).is_err());
    }

    #[test]
    fn validates_name_and_memory_without_dropping_existing_macros() {
        let definition = sample("ação 🎮");
        assert_eq!(
            decode_definition(&encode_definition(&definition).unwrap()).unwrap(),
            definition
        );
        assert!(encode_definition(&sample(&"🎮".repeat(16))).is_err());
        let mut big = sample(&"A".repeat(30));
        big.events = big.events.iter().cycle().take(1000).cloned().collect();
        let buffer = replace_macro(&[0; 4096], 0, &big).unwrap();
        assert!(replace_macro(&buffer, 1, &sample("nome suficientemente grande")).is_err());
    }
}
