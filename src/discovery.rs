use serde::Serialize;
use std::{fs, io, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    UsbCable,
    #[serde(rename = "receiver_24g")]
    Receiver24G,
}

#[derive(Debug, Serialize)]
pub struct Connection {
    pub path: PathBuf,
    pub transport: Transport,
    pub configuration_supported: bool,
    pub detail: &'static str,
}

fn classify(identity: &str, descriptor: &[u8]) -> io::Result<Option<Transport>> {
    if identity
        .lines()
        .any(|line| line == "HID_ID=0003:0000258A:0000010C")
        && crate::wired_interface(descriptor)?
    {
        return Ok(Some(Transport::UsbCable));
    }
    // Original 0x40c080: CDev3632 uses FF02, report 0x13, 20-byte input/output.
    if identity
        .lines()
        .any(|line| line == "HID_ID=0003:00003554:0000FA09")
        && crate::report_bits(descriptor, 0xff02, 0x13, 0x80)? == 19 * 8
        && crate::report_bits(descriptor, 0xff02, 0x13, 0x90)? == 19 * 8
    {
        return Ok(Some(Transport::Receiver24G));
    }
    Ok(None)
}

/// Enumerates advertised interfaces; does not open or send reports to any device.
pub fn connections() -> io::Result<Vec<Connection>> {
    let mut connections = Vec::new();
    for entry in fs::read_dir("/sys/class/hidraw")? {
        let entry = entry?;
        let device = entry.path().join("device");
        let (Ok(identity), Ok(descriptor)) = (
            fs::read_to_string(device.join("uevent")),
            fs::read(device.join("report_descriptor")),
        ) else {
            continue; // An interface can disappear during hot-unplug.
        };
        let Some(transport) = classify(&identity, &descriptor)? else {
            continue;
        };
        connections.push(Connection {
            path: PathBuf::from("/dev").join(entry.file_name()),
            transport,
            configuration_supported: true,
            detail: match transport {
                Transport::UsbCable => "Interface de configuração por cabo disponível.",
                Transport::Receiver24G => "Interface 2,4 GHz disponível para iluminação e camadas 0/1; macros preservadas. A presença do receptor não confirma que o teclado está ligado.",
            },
        });
    }
    connections.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(connections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_transports_by_both_identity_and_report_shape() {
        let receiver = include_bytes!("../tests/fixtures/receiver-descriptor.bin");
        let wired = include_bytes!("../tests/fixtures/f87pro-control-descriptor.bin");
        let receiver_id = "HID_ID=0003:00003554:0000FA09";
        let wired_id = "HID_ID=0003:0000258A:0000010C";
        assert_eq!(
            classify(receiver_id, receiver).unwrap(),
            Some(Transport::Receiver24G)
        );
        assert_eq!(
            classify(wired_id, wired).unwrap(),
            Some(Transport::UsbCable)
        );
        assert_eq!(classify(wired_id, receiver).unwrap(), None);
        assert_eq!(classify(receiver_id, wired).unwrap(), None);
        assert_eq!(
            classify("HID_ID=0003:00003554:0000F58A", receiver).unwrap(),
            None
        );
    }
}
