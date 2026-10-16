use std::io::{self, ErrorKind};

pub const SLEEP_OPTIONS: [u16; 10] = [30, 60, 90, 120, 180, 240, 300, 600, 900, 1200];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls {
    pub debounce_stage: u8,
    pub tap: Option<u8>,
    pub sleep_seconds: Option<u16>,
}

impl Controls {
    pub fn defaults() -> Self {
        Self {
            debounce_stage: 2,
            tap: None,
            sleep_seconds: Some(300),
        }
    }

    pub fn read(leds: &[u8]) -> io::Result<Self> {
        if leds.len() != 128 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "device configuration must contain 128 bytes",
            ));
        }
        let controls = Self {
            debounce_stage: leds[3].checked_add(1).unwrap_or(0),
            tap: (leds[22] != 0).then_some(leds[22]),
            sleep_seconds: (leds[24] != 0).then_some(u16::from(leds[24]) * 30),
        };
        controls.validate().map_err(|error| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("unknown device setting: {error}"),
            )
        })?;
        Ok(controls)
    }

    pub fn apply(&self, leds: &mut [u8]) -> io::Result<()> {
        if leds.len() != 128 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "device configuration must contain 128 bytes",
            ));
        }
        self.validate()?;
        leds[3] = self.debounce_stage - 1;
        leds[22] = self.tap.unwrap_or(0);
        leds[24] = self.sleep_seconds.map_or(0, |seconds| (seconds / 30) as u8);
        Ok(())
    }

    fn validate(&self) -> io::Result<()> {
        let message = if !(1..=5).contains(&self.debounce_stage) {
            "debounce stage must be 1..5"
        } else if self.tap.is_some_and(|value| !(10..=127).contains(&value)) {
            "Tap sensitivity must be 10..127 or disabled"
        } else if self
            .sleep_seconds
            .is_some_and(|seconds| !SLEEP_OPTIONS.contains(&seconds))
        {
            "unsupported sleep timeout"
        } else {
            return Ok(());
        };
        Err(io::Error::new(ErrorKind::InvalidInput, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_every_other_byte() {
        let original = *include_bytes!("../tests/fixtures/f87pro-led-read.bin");
        for debounce_stage in 1..=5 {
            for tap in [None, Some(10), Some(127)] {
                for sleep_seconds in std::iter::once(None).chain(SLEEP_OPTIONS.map(Some)) {
                    let controls = Controls {
                        debounce_stage,
                        tap,
                        sleep_seconds,
                    };
                    let mut changed = original;
                    controls.apply(&mut changed).unwrap();
                    assert_eq!(Controls::read(&changed).unwrap(), controls);
                    for index in 0..128 {
                        if ![3, 22, 24].contains(&index) {
                            assert_eq!(changed[index], original[index], "byte {index}");
                        }
                    }
                }
            }
        }
        let mut changed = original;
        Controls::defaults().apply(&mut changed).unwrap();
        assert_eq!([changed[3], changed[22], changed[24]], [1, 0, 10]);
    }

    #[test]
    fn invalid_controls_never_partially_mutate_configuration() {
        let original = [0xa5; 128];
        let invalid = [
            Controls {
                debounce_stage: 0,
                ..Controls::defaults()
            },
            Controls {
                debounce_stage: 6,
                ..Controls::defaults()
            },
            Controls {
                tap: Some(0),
                ..Controls::defaults()
            },
            Controls {
                tap: Some(9),
                ..Controls::defaults()
            },
            Controls {
                tap: Some(128),
                ..Controls::defaults()
            },
            Controls {
                sleep_seconds: Some(0),
                ..Controls::defaults()
            },
            Controls {
                sleep_seconds: Some(31),
                ..Controls::defaults()
            },
            Controls {
                sleep_seconds: Some(150),
                ..Controls::defaults()
            },
            Controls {
                sleep_seconds: Some(1230),
                ..Controls::defaults()
            },
        ];
        for controls in invalid {
            let mut changed = original;
            assert_eq!(
                controls.apply(&mut changed).unwrap_err().kind(),
                ErrorKind::InvalidInput
            );
            assert_eq!(changed, original);
        }
        for length in [0, 24, 127, 129] {
            let mut changed = vec![0xa5; length];
            assert!(Controls::defaults().apply(&mut changed).is_err());
            assert_eq!(changed, vec![0xa5; length]);
        }
    }

    #[test]
    fn unknown_fields_and_wrong_lengths_fail_read_without_normalizing() {
        let mut original = [0; 128];
        Controls::defaults().apply(&mut original).unwrap();
        for (offset, raw) in [(3, 5), (3, 255), (22, 1), (22, 128), (24, 5), (24, 255)] {
            let mut changed = original;
            changed[offset] = raw;
            assert_eq!(
                Controls::read(&changed).unwrap_err().kind(),
                ErrorKind::InvalidData
            );
            assert_eq!(changed[offset], raw);
        }
        for length in [0, 24, 127, 129] {
            assert_eq!(
                Controls::read(&vec![0; length]).unwrap_err().kind(),
                ErrorKind::InvalidData
            );
        }
    }
}
