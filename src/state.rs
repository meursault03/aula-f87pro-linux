use serde::{Deserialize, Serialize};
use std::io::{self, ErrorKind};

pub const MAX_SNAPSHOT_BYTES: usize = 65_536;

pub fn legacy_backup(bytes: &[u8], matrix: bool) -> io::Result<(Option<u8>, &[u8])> {
    let header = if matrix { 7 } else { 6 };
    let valid_length = if matrix {
        [7 + 504, 7 + 512].contains(&bytes.len())
    } else {
        bytes.len() == 6 + 128
    };
    if !valid_length
        || crate::Model::from_identity(&bytes[..6]).is_none()
        || (matrix && bytes[6] > 3)
        || (matrix && bytes.len() == 511 && bytes[..6] != [3, 0, 0, 0, 1, 11])
    {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid F87/F87 Pro backup",
        ));
    }
    Ok((matrix.then(|| bytes[6]), &bytes[header..]))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u8,
    pub identity: Vec<u8>,
    pub leds: Vec<u8>,
    pub layers: Vec<Vec<u8>>,
    pub macros: Vec<u8>,
}

pub fn decode(bytes: &[u8]) -> io::Result<Snapshot> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "snapshot exceeds 65536 bytes",
        ));
    }
    let snapshot: Snapshot = serde_json::from_slice(bytes)?;
    snapshot.validate()?;
    Ok(snapshot)
}

impl Snapshot {
    pub fn with_lighting_from(&self, profile: &Self) -> io::Result<Self> {
        self.validate()?;
        profile.validate()?;
        if self.identity != profile.identity {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "profile belongs to another keyboard model",
            ));
        }
        if profile.leds[9] != 0 || !crate::LED_EFFECTS.contains(&profile.leds[10]) {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "custom or unavailable lighting preview and transfer are not supported",
            ));
        }
        let mut draft = self.clone();
        // Copy known lighting fields; preserve transport flags and all unrelated device bytes.
        draft.leds[9..11].copy_from_slice(&profile.leds[9..11]);
        draft.leds[58..94].copy_from_slice(&profile.leds[58..94]);
        Ok(draft)
    }
    pub fn validate(&self) -> io::Result<()> {
        // Version 2 records only the bytes exposed by radio: never invent USB trailers or unread macro banks.
        let (layer_count, matrix_length, macro_length) = match self.version {
            1 => (4, 512, 4096),
            2 => (2, 504, 512),
            _ => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "unsupported F87 Pro snapshot version",
                ));
            }
        };
        let model = crate::Model::from_identity(&self.identity);
        if model.is_none()
            || (self.version == 2 && model != Some(crate::Model::F87Pro))
            || self.leds.len() != 128
            || self.layers.len() != layer_count
            || self.layers.iter().any(|layer| layer.len() != matrix_length)
            || self.macros.len() != macro_length
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "invalid F87/F87 Pro snapshot",
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    pub fn key_code(&self, layer: u8, key: u8) -> io::Result<[u8; 4]> {
        if layer > 3 {
            return Err(invalid_key());
        }
        let offset = key_offset(key)?;
        self.layers
            .get(usize::from(layer))
            .and_then(|matrix| matrix.get(offset..offset + 4))
            .and_then(|code| code.try_into().ok())
            .ok_or_else(invalid_key)
    }

    pub fn set_key_code(&mut self, layer: u8, key: u8, code: [u8; 4]) -> io::Result<()> {
        if layer > 3 {
            return Err(invalid_key());
        }
        let offset = key_offset(key)?;
        let destination = self
            .layers
            .get_mut(usize::from(layer))
            .and_then(|matrix| matrix.get_mut(offset..offset + 4))
            .ok_or_else(invalid_key)?;
        destination.copy_from_slice(&code);
        Ok(())
    }
}

fn invalid_key() -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, "invalid key or layer")
}

fn key_offset(key: u8) -> io::Result<usize> {
    crate::KEYS
        .get(usize::from(key.wrapping_sub(1)))
        .map(|(_, slot)| slot * 4)
        .ok_or_else(invalid_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_backups_accept_real_radio_and_usb_sizes_and_reject_truncation() {
        for size in [504, 512] {
            let mut bytes = vec![3, 0, 0, 0, 1, 11, 0];
            bytes.extend(vec![0; size]);
            assert_eq!(legacy_backup(&bytes, true).unwrap(), (Some(0), &bytes[7..]));
            assert!(legacy_backup(&bytes, false).is_err());
            bytes.pop();
            assert!(legacy_backup(&bytes, true).is_err());
        }
        assert!(legacy_backup(&[], true).is_err());
        assert!(legacy_backup(&[0; 134], false).is_err());
    }

    fn fixture() -> Snapshot {
        Snapshot {
            version: 1,
            identity: vec![3, 0, 0, 0, 1, 11],
            leds: include_bytes!("../tests/fixtures/f87pro-led-read.bin").to_vec(),
            layers: vec![include_bytes!("../tests/fixtures/f87pro-matrix-layer0.bin").to_vec(); 4],
            macros: vec![0; 4096],
        }
    }

    #[test]
    fn standard_f87_usb_state_and_effects_preserve_device_data() {
        // Synthetic state using the identity declared by Dev/kb/1/KB.ini, not a hardware capture.
        let mut original = fixture();
        original.identity = vec![3, 0, 0, 0, 0, 0x8f];
        assert_eq!(decode(&original.encode().unwrap()).unwrap(), original);
        for &effect in crate::LED_EFFECTS {
            let mut changed = original.clone();
            changed.leds = crate::led_settings(
                original.leds.as_slice().try_into().unwrap(),
                effect,
                4,
                3,
                7,
            )
            .unwrap()
            .to_vec();
            assert_eq!(changed.leds[10], effect);
            assert_eq!(changed.identity, original.identity);
            assert_eq!(changed.layers, original.layers);
            assert_eq!(changed.macros, original.macros);
            assert_eq!(decode(&changed.encode().unwrap()).unwrap(), changed);
        }
        let mut backup = original.identity.clone();
        backup.extend_from_slice(&original.leds);
        assert!(legacy_backup(&backup, false).is_ok());
        let mut matrix_backup = original.identity.clone();
        matrix_backup.push(0);
        matrix_backup.extend_from_slice(&original.layers[0]);
        assert!(legacy_backup(&matrix_backup, true).is_ok());
        matrix_backup.truncate(511);
        assert!(legacy_backup(&matrix_backup, true).is_err());
        let mut radio = original;
        radio.version = 2;
        radio.layers.truncate(2);
        for layer in &mut radio.layers {
            layer.truncate(504);
        }
        radio.macros.truncate(512);
        assert!(radio.validate().is_err());
    }

    #[test]
    fn rgb_loading_rejects_a_profile_from_another_model() {
        let pro = fixture();
        let mut standard = pro.clone();
        standard.identity = vec![3, 0, 0, 0, 0, 0x8f];
        assert!(standard.with_lighting_from(&pro).is_err());
        assert!(pro.with_lighting_from(&standard).is_err());
        assert!(standard.with_lighting_from(&standard).is_ok());
    }

    #[test]
    fn rgb_profiles_reject_masked_effects_but_backups_preserve_them() {
        let original = fixture();
        for effect in [6, 9, 14, 18] {
            let mut profile = original.clone();
            profile.leds[10] = effect;
            assert_eq!(decode(&profile.encode().unwrap()).unwrap(), profile);
            assert!(original.with_lighting_from(&profile).is_err());
        }
    }

    #[test]
    fn snapshot_roundtrip_and_key_edit_preserve_every_other_byte() {
        let original = fixture();
        let bytes = original.encode().unwrap();
        assert_eq!(decode(&bytes).unwrap(), original);
        let mut changed = original.clone();
        changed.set_key_code(0, 13, [0, 0, 0, 0x48]).unwrap();
        assert_eq!(changed.key_code(0, 13).unwrap(), [0, 0, 0, 0x48]);
        changed
            .set_key_code(0, 13, original.key_code(0, 13).unwrap())
            .unwrap();
        assert_eq!(changed, original);
        for (layer, key) in [(4, 1), (0, 0), (0, 88)] {
            assert!(changed.key_code(layer, key).is_err());
            assert!(changed.set_key_code(layer, key, [0; 4]).is_err());
        }
    }

    #[test]
    fn rejects_incompatible_truncated_and_excessive_profiles() {
        for field in 0..7 {
            let mut state = fixture();
            match field {
                0 => state.version = 2,
                1 => state.identity[0] = 4,
                2 => state.leds.pop().map(|_| ()).unwrap(),
                3 => state.layers.pop().map(|_| ()).unwrap(),
                4 => state.layers[0].push(0),
                5 => state.identity.push(0),
                _ => state.macros.push(0),
            }
            assert!(decode(&serde_json::to_vec(&state).unwrap()).is_err());
        }
        assert!(decode(&vec![b' '; 65_537]).is_err());
        assert!(decode(br#"{"version":1}"#).is_err());
        let mut state = fixture();
        state.layers.clear();
        assert!(state.key_code(0, 1).is_err());
        assert!(state.set_key_code(0, 1, [0; 4]).is_err());
    }

    #[test]
    fn radio_snapshots_record_only_observed_bytes_and_keep_key_edits_bounded() {
        let mut radio = fixture();
        radio.version = 2;
        radio.layers.truncate(2);
        for layer in &mut radio.layers {
            layer.truncate(504);
        }
        radio.macros.truncate(512);
        assert_eq!(decode(&radio.encode().unwrap()).unwrap(), radio);
        for key in 1..=87 {
            let before = radio.clone();
            let code = radio.key_code(0, key).unwrap();
            radio.set_key_code(0, key, [0, 0, 0, 4]).unwrap();
            radio.set_key_code(0, key, code).unwrap();
            assert_eq!(radio, before);
        }
        radio.macros.push(0);
        assert!(radio.validate().is_err());
    }

    #[test]
    fn rgb_profiles_cross_transports_without_overwriting_transport_flags_or_keys() {
        let mut profile = fixture();
        profile.leds[9] = 0;
        profile.leds[10] = 2;
        profile.leds[60] = 1;
        profile.leds[14] = 0;
        let mut radio = fixture();
        radio.version = 2;
        radio.layers.truncate(2);
        for layer in &mut radio.layers {
            layer.truncate(504);
        }
        radio.macros.truncate(512);
        radio.leds[14] = 1;
        let changed = radio.with_lighting_from(&profile).unwrap();
        assert_eq!(changed.version, 2);
        assert_eq!(changed.leds[10], 2);
        assert_eq!(changed.leds[60], 1);
        assert_eq!(changed.leds[14], 1);
        assert_eq!(changed.layers, radio.layers);
        assert_eq!(changed.macros, radio.macros);
        profile.leds[9] = 1;
        assert!(radio.with_lighting_from(&profile).is_err());
    }
}
