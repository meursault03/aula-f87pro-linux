use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

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
    /// USB descriptor release number; receiver-reported in 2.4 GHz mode.
    pub descriptor_version: Option<u16>,
    pub configuration_supported: bool,
    pub detail: &'static str,
}

/// Matches the original application's V%X.%X.%X descriptor-version display.
pub fn format_descriptor_version(version: u16) -> String {
    format!(
        "V{:X}.{:X}.{:X}",
        version >> 8,
        (version >> 4) & 0xf,
        version & 0xf
    )
}

fn descriptor_version(device: &Path, transport: Transport) -> Option<u16> {
    let device = fs::canonicalize(device).ok()?;
    let expected = match transport {
        Transport::UsbCable => (0x258a, 0x010c),
        Transport::Receiver24G => (0x3554, 0xfa09),
    };
    let read_hex = |path: PathBuf| {
        let text = fs::read_to_string(path).ok()?;
        let text = text.trim();
        (text.len() == 4 && text.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| u16::from_str_radix(text, 16).ok())
            .flatten()
    };
    for parent in device.ancestors() {
        if parent.join("idVendor").exists() || parent.join("idProduct").exists() {
            // Stop at the nearest USB device; never report a hub's release number.
            return ((
                read_hex(parent.join("idVendor"))?,
                read_hex(parent.join("idProduct"))?,
            ) == expected)
                .then(|| read_hex(parent.join("bcdDevice")))
                .flatten();
        }
    }
    None
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
            descriptor_version: descriptor_version(&device, transport),
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
    fn formats_original_descriptor_version_components() {
        assert_eq!(format_descriptor_version(0x0102), "V1.0.2");
        assert_eq!(format_descriptor_version(0x12ab), "V12.A.B");
    }

    #[test]
    fn reads_only_matching_nearest_usb_parent_version() {
        let root = std::env::temp_dir().join(format!(
            "aula-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let usb = root.join("hub/keyboard");
        let hid = usb.join("interface/hid");
        fs::create_dir_all(&hid).unwrap();
        for (name, value) in [
            ("idVendor", "1234"),
            ("idProduct", "5678"),
            ("bcdDevice", "9999"),
        ] {
            fs::write(root.join("hub").join(name), value).unwrap();
        }
        for (name, value) in [
            ("idVendor", "258a\n"),
            ("idProduct", "010c\n"),
            ("bcdDevice", "0102\n"),
        ] {
            fs::write(usb.join(name), value).unwrap();
        }
        let link = root.join("hidraw-device");
        std::os::unix::fs::symlink(&hid, &link).unwrap();
        assert_eq!(descriptor_version(&link, Transport::UsbCable), Some(0x0102));
        assert_eq!(descriptor_version(&link, Transport::Receiver24G), None);
        fs::write(usb.join("bcdDevice"), "bogus").unwrap();
        assert_eq!(descriptor_version(&link, Transport::UsbCable), None);
        fs::write(usb.join("bcdDevice"), "12345").unwrap();
        assert_eq!(descriptor_version(&link, Transport::UsbCable), None);
        fs::remove_file(usb.join("bcdDevice")).unwrap();
        assert_eq!(descriptor_version(&link, Transport::UsbCable), None);
        fs::write(usb.join("idVendor"), "3554").unwrap();
        fs::write(usb.join("idProduct"), "fa09").unwrap();
        fs::write(usb.join("bcdDevice"), "0103").unwrap();
        assert_eq!(
            descriptor_version(&link, Transport::Receiver24G),
            Some(0x0103)
        );
        fs::remove_dir_all(root).unwrap();
    }

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
