use std::io::{self, ErrorKind};
pub mod device_settings;
pub mod discovery;
mod keys;
pub mod macros;
pub mod state;
pub mod wireless;
pub use keys::KEYS;

pub fn macro_request(operation: u8, chunk: u8) -> io::Result<[u8; 520]> {
    if !matches!(operation, 5 | 0x85) || chunk >= 8 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid macro request",
        ));
    }
    // CDevG5KB AccessData 0x488a20: eight 512-byte chunks, zero-based index.
    let mut report = [0; 520];
    report[..8].copy_from_slice(&[6, operation, 0, 0, 8, chunk, 0, 2]);
    Ok(report)
}

pub fn read_request(operation: u8, argument: u8, length: u16) -> io::Result<[u8; 520]> {
    if !matches!(
        (operation, argument, length),
        (0x82, 1, 6) | (0x84, 0, 128) | (0x83, 0..=3, 512)
    ) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid read request",
        ));
    }
    let mut report = [0; 520];
    report[..8].copy_from_slice(&[
        6,
        operation,
        argument,
        0,
        1,
        0,
        length as u8,
        (length >> 8) as u8,
    ]);
    Ok(report)
}

pub fn response_payload(report: &[u8], length: u16) -> io::Result<Vec<u8>> {
    if report.first() != Some(&6) || length == 0 || length > 512 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid feature response",
        ));
    }
    report
        .get(8..8 + usize::from(length))
        .map(|data| data.to_vec())
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::UnexpectedEof,
                format!(
                    "GET_FEATURE returned {} bytes; need at least {}",
                    report.len(),
                    8 + usize::from(length)
                ),
            )
        })
}

pub fn led_effect(original: &[u8; 128], effect: u8) -> io::Result<[u8; 128]> {
    // KB.ini LedOpt first field -> config[10], per parser 0x410840 and FillLED 0x48abf0.
    let hardware = match effect {
        1..=18 => effect,
        0 | 21 => 0,
        _ => {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "supported LED effects: 0 (off), 1..18",
            ));
        }
    };
    let mut config = *original;
    // FillLED at 0x48abf0: custom flag at 9, hardware effect at 10.
    config[9] = 0;
    config[10] = hardware;
    Ok(config)
}

pub fn led_settings(
    original: &[u8; 128],
    effect: u8,
    brightness: u8,
    speed: u8,
    palette: u8,
) -> io::Result<[u8; 128]> {
    if brightness > 4 || speed > 4 || palette > 7 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "LED brightness/speed must be 0..4 and palette 0..7",
        ));
    }
    let mut changed = led_effect(original, effect)?;
    if (1..=18).contains(&effect) {
        // FillLED 0x48abf0: brightness byte, then speed high nibble / palette low nibble.
        let offset = 58 + 2 * usize::from(effect - 1);
        changed[offset] = brightness;
        changed[offset + 1] = (speed << 4) | palette;
    }
    Ok(changed)
}

pub fn write_led_request(config: &[u8; 128]) -> [u8; 520] {
    let mut report = [0; 520];
    report[..8].copy_from_slice(&[6, 4, 0, 0, 1, 0, 128, 0]);
    report[8..136].copy_from_slice(config);
    report
}

pub fn write_matrix_request(layer: u8, matrix: &[u8; 512]) -> io::Result<[u8; 520]> {
    let mut report = read_request(0x83, layer, 512)?;
    report[1] = 3;
    report[8..].copy_from_slice(matrix);
    Ok(report)
}

pub fn remap_matrix<const N: usize>(original: &[u8; N], from: u8, to: u8) -> io::Result<[u8; N]> {
    if !matches!(N, 504 | 512) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid matrix length",
        ));
    }
    let slot = |key: u8| {
        KEYS.get(usize::from(key.wrapping_sub(1)))
            .map(|(_, slot)| slot * 4)
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    "key IDs must be in 1..87; use keys LAYER to see labels",
                )
            })
    };
    let from = slot(from)?;
    let to = slot(to)?;
    let mut changed = *original;
    changed[from..from + 4].copy_from_slice(&original[to..to + 4]);
    Ok(changed)
}

pub fn wired_interface(descriptor: &[u8]) -> io::Result<bool> {
    Ok(report_bits(descriptor, 0xff00, 6, 0xb0)? == 519 * 8)
}

pub(crate) fn report_bits(descriptor: &[u8], page: u32, id: u32, item: u8) -> io::Result<u64> {
    let invalid = || io::Error::new(ErrorKind::InvalidData, "invalid HID descriptor");
    let mut globals = [0u32; 4]; // Usage page, report size, report count, report ID.
    let mut stack = Vec::new();
    let mut feature_bits = 0u64;
    let mut position = 0;
    while position < descriptor.len() {
        let prefix = descriptor[position];
        position += 1;
        if prefix == 0xfe {
            let size = *descriptor.get(position).ok_or_else(invalid)? as usize;
            position += 2 + size;
            if position > descriptor.len() {
                return Err(invalid());
            }
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            n => n as usize,
        };
        let bytes = descriptor
            .get(position..position + size)
            .ok_or_else(invalid)?;
        let value = bytes
            .iter()
            .enumerate()
            .fold(0, |v, (i, b)| v | ((*b as u32) << (8 * i)));
        position += size;
        match prefix & 0xfc {
            0x04 => globals[0] = value,
            0x74 => globals[1] = value,
            0x94 => globals[2] = value,
            0x84 => globals[3] = value,
            0xa4 => {
                if stack.len() >= 16 {
                    return Err(invalid());
                }
                stack.push(globals);
            }
            0xb4 => globals = stack.pop().ok_or_else(invalid)?,
            tag if tag == item && globals[0] == page && globals[3] == id => {
                feature_bits = feature_bits
                    .checked_add(u64::from(globals[1]) * u64::from(globals[2]))
                    .ok_or_else(invalid)?;
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(invalid());
    }
    Ok(feature_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_headers_match_original_and_reject_out_of_range_chunks() {
        for operation in [5, 0x85] {
            for chunk in 0..8 {
                let report = macro_request(operation, chunk).unwrap();
                assert_eq!(&report[..8], &[6, operation, 0, 0, 8, chunk, 0, 2]);
                assert!(report[8..].iter().all(|byte| *byte == 0));
            }
        }
        assert!(macro_request(0x85, 8).is_err());
        assert!(macro_request(0x83, 0).is_err());
    }

    #[test]
    fn identity_request_matches_original() {
        let frame = read_request(0x82, 1, 6).unwrap();
        assert_eq!(&frame[..8], &[6, 0x82, 1, 0, 1, 0, 6, 0]);
        assert!(frame[8..].iter().all(|b| *b == 0));
        assert_eq!(frame.len(), 520);
    }

    #[test]
    fn accepts_unpadded_response_and_rejects_missing_payload() {
        let reply = [6, 0x82, 1, 0, 1, 0, 6, 0, 3, 0, 0, 0, 1, 0x0b];
        assert_eq!(response_payload(&reply, 6).unwrap(), [3, 0, 0, 0, 1, 0x0b]);
        assert!(response_payload(&reply[..13], 6).is_err());
        let mut wrong_id = reply;
        wrong_id[0] = 9;
        assert!(response_payload(&wrong_id, 6).is_err());
    }

    #[test]
    fn leds_request_and_invalid_lengths() {
        assert_eq!(
            &read_request(0x84, 0, 128).unwrap()[..8],
            &[6, 0x84, 0, 0, 1, 0, 128, 0]
        );
        assert!(read_request(0x84, 0, 0).is_err());
        assert!(read_request(0x84, 0, 513).is_err());
        assert!(read_request(4, 0, 128).is_err());
    }

    #[test]
    fn expanded_led_settings_preserve_unrelated_fields() {
        let original = include_bytes!("../tests/fixtures/f87pro-led-read.bin");
        let changed = led_settings(original, 2, 1, 4, 5).unwrap();
        assert_eq!(changed[60], 1);
        assert_eq!(changed[61], 0x45);
        for index in 0..128 {
            if ![9, 10, 60, 61].contains(&index) {
                assert_eq!(changed[index], original[index]);
            }
        }
        assert!(led_settings(original, 2, 5, 4, 5).is_err());
        assert!(led_settings(original, 2, 4, 5, 5).is_err());
        assert!(led_settings(original, 2, 4, 4, 8).is_err());
        let off = led_settings(original, 0, 0, 0, 0).unwrap();
        assert_eq!(&off[58..], &original[58..]);
    }

    #[test]
    fn effect_change_preserves_real_configuration() {
        let original = include_bytes!("../tests/fixtures/f87pro-led-read.bin");
        let changed = led_effect(original, 1).unwrap();
        assert_eq!(changed[9], 0);
        assert_eq!(changed[10], 1);
        for i in 0..128 {
            if i != 9 && i != 10 {
                assert_eq!(changed[i], original[i]);
            }
        }
        assert_eq!(led_effect(original, 12).unwrap()[10], original[10]);
        assert_eq!(led_effect(original, 13).unwrap()[10], 13);
        assert_eq!(led_effect(original, 18).unwrap()[10], 18);
        assert_eq!(led_effect(original, 21).unwrap()[10], 0);
        assert!(led_effect(original, 19).is_err());
        let frame = write_led_request(&changed);
        assert_eq!(&frame[..8], &[6, 4, 0, 0, 1, 0, 128, 0]);
        assert_eq!(&frame[8..136], &changed);
        assert!(frame[136..].iter().all(|b| *b == 0));
    }

    #[test]
    fn real_led_payload_survives_response_parsing() {
        let payload = include_bytes!("../tests/fixtures/f87pro-led-read.bin");
        let mut response = vec![6, 0x84, 0, 0, 1, 0, 128, 0];
        response.extend_from_slice(payload);
        assert_eq!(response_payload(&response, 128).unwrap(), payload);
    }

    #[test]
    fn matrix_read_write_headers_and_layer_bounds() {
        let matrix = [0x5a; 512];
        for layer in 0..4 {
            assert_eq!(
                &read_request(0x83, layer, 512).unwrap()[..8],
                &[6, 0x83, layer, 0, 1, 0, 0, 2]
            );
            let frame = write_matrix_request(layer, &matrix).unwrap();
            assert_eq!(&frame[..8], &[6, 3, layer, 0, 1, 0, 0, 2]);
            assert_eq!(&frame[8..], &matrix);
        }
        assert!(read_request(0x83, 4, 512).is_err());
        assert!(write_matrix_request(4, &matrix).is_err());
    }

    #[test]
    fn remapping_real_matrix_preserves_other_slots() {
        let original = include_bytes!("../tests/fixtures/f87pro-matrix-layer0.bin");
        assert_eq!(KEYS[12], ("F12", 78));
        assert_eq!(KEYS[29], ("Pause", 96));
        assert_eq!(&original[312..316], &[0, 0, 0, 0x45]);
        let changed = remap_matrix(original, 13, 30).unwrap();
        assert_eq!(&changed[312..316], &original[384..388]);
        assert_eq!(&changed[..312], &original[..312]);
        assert_eq!(&changed[316..], &original[316..]);
    }

    #[test]
    fn copying_b_to_a_changes_only_a_slot() {
        let mut original = [0; 512];
        for slot in 0..128 {
            original[slot * 4..slot * 4 + 4].copy_from_slice(&[0, 0, slot as u8, 0]);
        }
        assert_eq!(KEYS[49], ("A", 9));
        assert_eq!(KEYS[68], ("B", 34));
        let changed = remap_matrix(&original, 50, 69).unwrap();
        assert_eq!(&changed[36..40], &original[136..140]);
        assert_eq!(&changed[..36], &original[..36]);
        assert_eq!(&changed[40..], &original[40..]);
        assert!(remap_matrix(&original, 0, 69).is_err());
        assert!(remap_matrix(&original, 50, 88).is_err());
    }

    #[test]
    fn rejects_receiver_with_same_report_id() {
        let receiver = include_bytes!("../tests/fixtures/receiver-descriptor.bin");
        assert!(!wired_interface(receiver).unwrap());
        let wired = [
            0x06, 0x00, 0xff, 0x85, 6, 0x75, 8, 0x96, 0x07, 0x02, 0xb1, 2,
        ];
        assert!(wired_interface(&wired).unwrap());
        assert!(wired_interface(&wired[..wired.len() - 1]).is_err());
        assert!(
            wired_interface(include_bytes!(
                "../tests/fixtures/f87pro-control-descriptor.bin"
            ))
            .unwrap()
        );
    }

    #[test]
    fn descriptor_globals_restore_and_malformed_items_fail() {
        let pushed = [
            0x06, 0x00, 0xff, 0x85, 6, 0x75, 8, 0x96, 0x07, 0x02, 0xa4, 0x06, 0x04, 0xff, 0xb4,
            0xb1, 2,
        ];
        assert!(wired_interface(&pushed).unwrap());
        assert!(wired_interface(&[0xb4]).is_err());
        assert!(wired_interface(&[0xa4]).is_err());
        assert!(wired_interface(&[0xfe, 10, 0]).is_err());
    }
}
