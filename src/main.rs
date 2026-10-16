use aula_f87pro::discovery::Transport;
use aula_f87pro::state::{self, Snapshot};
use aula_f87pro::{
    KEYS, Model, led_effect, macro_request, read_request, remap_matrix, response_payload,
    write_led_request, write_matrix_request,
};
use serde::Serialize;
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
compile_error!("This initial HID transport supports Linux x86_64.");
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, ErrorKind, Read, Write},
    os::fd::AsRawFd,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

unsafe extern "C" {
    fn ioctl(fd: std::ffi::c_int, request: std::ffi::c_ulong, ...) -> std::ffi::c_int;
    fn flock(fd: std::ffi::c_int, operation: std::ffi::c_int) -> std::ffi::c_int;
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Matrix(u8),
    Leds,
    Macros,
}

trait StateDevice {
    fn snapshot(&self) -> io::Result<Snapshot>;
    fn write_part(&self, part: Part, data: &[u8]) -> io::Result<()>;
}

struct Device {
    file: File,
    transport: Transport,
}

fn identify(identity: &[u8], transport: Transport) -> io::Result<Model> {
    let model = Model::from_identity(identity).ok_or_else(|| {
        io::Error::other(format!("unrecognized keyboard identity: {identity:02x?}"))
    })?;
    if model == Model::F87 && transport != Transport::UsbCable {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "AULA F87 standard configuration currently requires USB cable",
        ));
    }
    Ok(model)
}

impl Device {
    fn read(&self, operation: u8, argument: u8, length: u16) -> io::Result<Vec<u8>> {
        if self.transport == Transport::UsbCable {
            return read_feature(&self.file, operation, argument, length);
        }
        match (operation, argument, length) {
            (0x82, 1, 6) => aula_f87pro::wireless::read_identity(&self.file),
            (0x83, layer @ 0..=1, 512) => aula_f87pro::wireless::read_matrix(&self.file, layer),
            (0x84, 0, 128) => aula_f87pro::wireless::read_leds(&self.file),
            _ => Err(io::Error::new(
                ErrorKind::InvalidInput,
                "invalid wireless read",
            )),
        }
    }
}

impl StateDevice for Device {
    fn snapshot(&self) -> io::Result<Snapshot> {
        if self.transport == Transport::UsbCable {
            return self.file.snapshot();
        }
        let identity = self.read(0x82, 1, 6)?;
        identify(&identity, self.transport)?;
        let state = Snapshot {
            version: 2,
            identity,
            leds: self.read(0x84, 0, 128)?,
            layers: (0..2)
                .map(|layer| self.read(0x83, layer, 512))
                .collect::<io::Result<_>>()?,
            macros: aula_f87pro::wireless::read_macros(&self.file)?,
        };
        state.validate()?;
        Ok(state)
    }

    fn write_part(&self, part: Part, data: &[u8]) -> io::Result<()> {
        if self.transport == Transport::UsbCable {
            return self.file.write_part(part, data);
        }
        match part {
            Part::Matrix(2..) => {
                return Err(io::Error::new(
                    ErrorKind::Unsupported,
                    "radio configuration supports layers 0 and 1 only",
                ));
            }
            Part::Matrix(layer) => aula_f87pro::wireless::write_matrix(&self.file, layer, data)?,
            Part::Leds => aula_f87pro::wireless::write_leds(&self.file, data)?,
            Part::Macros => aula_f87pro::wireless::write_macros(&self.file, data)?,
        }
        let readback = match part {
            Part::Matrix(layer) => self.read(0x83, layer, 512)?,
            Part::Leds => self.read(0x84, 0, 128)?,
            Part::Macros => aula_f87pro::wireless::read_macros(&self.file)?,
        };
        if readback != data {
            return Err(io::Error::other("wireless configuration readback differs"));
        }
        Ok(())
    }
}

impl StateDevice for File {
    fn snapshot(&self) -> io::Result<Snapshot> {
        let identity = read_feature(self, 0x82, 1, 6)?;
        identify(&identity, Transport::UsbCable)?;
        let state = Snapshot {
            version: 1,
            identity,
            leds: read_feature(self, 0x84, 0, 128)?,
            layers: (0..4)
                .map(|layer| read_feature(self, 0x83, layer, 512))
                .collect::<io::Result<_>>()?,
            macros: read_macros(self)?,
        };
        state.validate()?;
        Ok(state)
    }

    fn write_part(&self, part: Part, data: &[u8]) -> io::Result<()> {
        let bad_length = || io::Error::new(ErrorKind::InvalidInput, "invalid configuration length");
        if part == Part::Macros {
            if data.len() != 4096 {
                return Err(bad_length());
            }
            for (chunk, payload) in data.as_chunks::<512>().0.iter().enumerate() {
                let mut report = macro_request(5, chunk as u8)?;
                report[8..].copy_from_slice(payload);
                write_report(self, &mut report)?;
            }
        } else {
            let mut report = match part {
                Part::Matrix(layer) => {
                    write_matrix_request(layer, data.try_into().map_err(|_| bad_length())?)?
                }
                Part::Leds => write_led_request(data.try_into().map_err(|_| bad_length())?),
                Part::Macros => unreachable!(),
            };
            write_report(self, &mut report)?;
        };
        let readback = match part {
            Part::Matrix(layer) => read_feature(self, 0x83, layer, 512)?,
            Part::Leds => read_feature(self, 0x84, 0, 128)?,
            Part::Macros => read_macros(self)?,
        };
        if readback != data {
            return Err(io::Error::other("configuration readback differs"));
        }
        Ok(())
    }
}

fn write_report(file: &File, report: &mut [u8; 520]) -> io::Result<()> {
    thread::sleep(Duration::from_millis(20));
    // The report remains writable and alive for the synchronous ioctl.
    let count = unsafe { ioctl(file.as_raw_fd(), 0xc2084806, report.as_mut_ptr()) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    if count != 520 {
        return Err(io::Error::new(
            ErrorKind::UnexpectedEof,
            format!("Write returned {count} bytes; expected 520"),
        ));
    }
    thread::sleep(Duration::from_millis(70));
    Ok(())
}

#[derive(Debug, Serialize)]
struct ApplyResult {
    changed: bool,
    backup: Option<String>,
    state: Snapshot,
}

fn write_difference(
    device: &impl StateDevice,
    from: &Snapshot,
    to: &Snapshot,
    recovering: bool,
) -> io::Result<()> {
    let mut failures = Vec::new();
    let matrix_and_leds = (0..from.layers.len())
        .map(|layer| {
            (
                Part::Matrix(layer as u8),
                &from.layers[layer],
                &to.layers[layer],
            )
        })
        .chain(std::iter::once((Part::Leds, &from.leds, &to.leds)));
    let macros = (Part::Macros, &from.macros, &to.macros);
    // Install macro data before binding keys; unbind keys before restoring old macro data.
    let parts: Vec<_> = if recovering {
        matrix_and_leds.chain(std::iter::once(macros)).collect()
    } else {
        std::iter::once(macros).chain(matrix_and_leds).collect()
    };
    for (part, old, new) in parts {
        if old != new
            && let Err(error) = device.write_part(part, new)
        {
            if !recovering {
                return Err(error);
            }
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other(failures.join("; ")))
    }
}

fn apply_state(
    device: &impl StateDevice,
    desired: Snapshot,
    expected: &Snapshot,
    save: impl FnOnce(&Snapshot) -> io::Result<String>,
) -> io::Result<ApplyResult> {
    desired.validate()?;
    expected.validate()?;
    let original = device.snapshot()?;
    if &original != expected {
        return Err(io::Error::other(
            "keyboard changed since it was loaded; refresh before applying",
        ));
    }
    if desired.identity != original.identity {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "profile belongs to another keyboard model",
        ));
    }
    if desired.version != original.version {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "profile transport differs: USB profiles require USB; 2.4 GHz profiles require the receiver",
        ));
    }
    if desired.leds != original.leds
        && desired.leds[9] == 0
        && !aula_f87pro::LED_EFFECTS.contains(&desired.leds[10])
    {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "lighting effect is unavailable in the original app for this keyboard",
        ));
    }
    if original.version == 2 && desired.leds[14] != original.leds[14] {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "LED backup uses another transport; load RGB fields only or restore using USB",
        ));
    }
    if desired == original {
        return Ok(ApplyResult {
            changed: false,
            backup: None,
            state: original,
        });
    }
    validate_macro_changes(&original, &desired)?;
    let backup = save(&original)?;
    let applied = write_difference(device, &original, &desired, false).and_then(|()| {
        if device.snapshot()? == desired {
            Ok(())
        } else {
            Err(io::Error::other("full state readback differs"))
        }
    });
    if let Err(error) = applied {
        let restored = write_difference(device, &desired, &original, true).and_then(|()| {
            if device.snapshot()? == original {
                Ok(())
            } else {
                Err(io::Error::other("full state rollback readback differs"))
            }
        });
        return Err(io::Error::other(match restored {
            Ok(()) => format!("{error}; original state restored; backup: {backup}"),
            Err(rollback) => {
                format!("{error}; ROLLBACK FAILED: {rollback}; restore backup: {backup}")
            }
        }));
    }
    Ok(ApplyResult {
        changed: true,
        backup: Some(backup),
        state: desired,
    })
}

fn validate_macro_changes(original: &Snapshot, desired: &Snapshot) -> io::Result<()> {
    let changed_macros = desired.macros != original.macros;
    let changed_references: Vec<_> = original
        .layers
        .iter()
        .zip(&desired.layers)
        .flat_map(|(old, new)| old.as_chunks::<4>().0.iter().zip(new.as_chunks::<4>().0))
        .filter_map(|(old, new)| (new[0] == 3 && (changed_macros || old != new)).then_some(new[3]))
        .collect();
    if changed_macros || !changed_references.is_empty() {
        if original.version == 2 {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "macro editing over 2.4 GHz requires a complete memory backup; connect by USB cable",
            ));
        }
        let buffer: &[u8; 4096] =
            desired.macros.as_slice().try_into().map_err(|_| {
                io::Error::new(ErrorKind::InvalidData, "invalid macro buffer length")
            })?;
        let macros = aula_f87pro::macros::macro_blobs(buffer)?;
        if changed_references
            .iter()
            .any(|slot| usize::from(*slot) >= macros.len())
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "key references a nonexistent macro slot",
            ));
        }
    }
    Ok(())
}

fn load_snapshot(path: &str) -> io::Result<Snapshot> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(state::MAX_SNAPSHOT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    state::decode(&bytes)
}

fn save_snapshot(state: &Snapshot) -> io::Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let path = env::current_dir()?.join(format!("aula-f87pro-state-{timestamp}.json"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&state.encode()?)?;
    file.sync_all()?;
    File::open(env::current_dir()?)?.sync_all()?;
    Ok(path.to_string_lossy().into_owned())
}

#[repr(C)]
#[derive(Default)]
struct DeviceInfo {
    bus: u32,
    vendor: i16,
    product: i16,
}

fn read_feature(file: &File, operation: u8, argument: u8, length: u16) -> io::Result<Vec<u8>> {
    receive_feature(file, read_request(operation, argument, length)?, length)
}

fn read_macros(file: &File) -> io::Result<Vec<u8>> {
    let mut data = Vec::with_capacity(4096);
    for chunk in 0..8 {
        data.extend_from_slice(&receive_feature(file, macro_request(0x85, chunk)?, 512)?);
    }
    Ok(data)
}

fn receive_feature(file: &File, mut report: [u8; 520], length: u16) -> io::Result<Vec<u8>> {
    let mut received = 0;
    // linux/hidraw.h: HIDIOCSFEATURE(520), HIDIOCGFEATURE(520), including report ID.
    for request in [0xc2084806, 0xc2084807] {
        thread::sleep(Duration::from_millis(20));
        // report is writable and remains alive for the synchronous ioctl.
        let count = unsafe { ioctl(file.as_raw_fd(), request, report.as_mut_ptr()) };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        if request == 0xc2084806 && count != 520 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                format!("SET_FEATURE returned {count} bytes; expected 520"),
            ));
        }
        received = count as usize;
    }
    let reply = report
        .get(..received)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "oversized feature response"))?;
    response_payload(reply, length)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("list");
    if !matches!(
        (command, args.len()),
        ("list", 0 | 1)
            | ("connections", 1)
            | ("inspect", 1)
            | ("led-effect" | "restore-led" | "keys" | "restore-keys", 2)
            | ("remap", 4)
            | ("snapshot", 1)
            | ("apply-state", 3)
    ) {
        return Err(
            "usage: aula-f87pro list | connections | inspect | led-effect INDEX | restore-led BACKUP | keys LAYER | remap LAYER FROM_ID TO_ID | restore-keys BACKUP | snapshot | apply-state DESIRED EXPECTED".into(),
        );
    }
    let states = if command == "apply-state" {
        Some((load_snapshot(&args[1])?, load_snapshot(&args[2])?))
    } else {
        None
    };
    let restoration = if matches!(command, "restore-led" | "restore-keys") {
        Some(fs::read(&args[1])?)
    } else {
        None
    };
    if let Some(backup) = &restoration {
        state::legacy_backup(backup, command == "restore-keys")?;
    }
    let matrix_command = matches!(command, "keys" | "remap" | "restore-keys");
    let layer: u8 = match command {
        "keys" | "remap" => args[1].parse()?,
        "restore-keys" => restoration.as_ref().ok_or("missing backup")?[6],
        _ => 0,
    };
    if layer > 3 {
        return Err("layer must be in 0..3".into());
    }
    let connections = aula_f87pro::discovery::connections()?;
    if command == "connections" {
        println!("{}", serde_json::to_string(&connections)?);
        return Ok(());
    }
    // Prefer the cable when both interfaces belong to the same connected keyboard.
    let wired = connections
        .iter()
        .any(|c| c.transport == Transport::UsbCable);
    let devices: Vec<_> = connections
        .into_iter()
        .filter(|c| !wired || c.transport == Transport::UsbCable)
        .collect();
    if command == "list" {
        for device in &devices {
            println!("{}", device.path.display());
        }
        if devices.is_empty() {
            println!("No compatible AULA F87/F87 Pro USB or F87 Pro 2.4 GHz receiver interface.");
        }
        return Ok(());
    }
    if devices.len() != 1 {
        return Err(
            "requires exactly one compatible AULA F87/F87 Pro USB or F87 Pro 2.4 GHz receiver interface"
                .into(),
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&devices[0].path)?;
    // Serialize this application's HID queries and writes across CLI/GUI processes.
    if unsafe { flock(file.as_raw_fd(), 2 | 4) } < 0 {
        return Err("keyboard is busy in another process; retry shortly".into());
    }
    let mut info = DeviceInfo::default();
    // HIDIOCGRAWINFO verifies the opened node, which can change after discovery.
    if unsafe { ioctl(file.as_raw_fd(), 0x80084803, &mut info as *mut DeviceInfo) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let (vendor, product) = match devices[0].transport {
        Transport::UsbCable => (0x258a, 0x010c),
        Transport::Receiver24G => (0x3554, 0xfa09),
    };
    if info.bus != 3 || info.vendor as u16 != vendor || info.product as u16 != product {
        return Err("opened device differs from the discovered keyboard".into());
    }
    let device = Device {
        file,
        transport: devices[0].transport,
    };
    if command == "snapshot" {
        println!("{}", serde_json::to_string(&device.snapshot()?)?);
        return Ok(());
    }
    if let Some((desired, expected)) = states {
        let result = apply_state(&device, desired, &expected, save_snapshot)?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    let radio_state =
        if device.transport == Transport::Receiver24G && !matches!(command, "inspect" | "keys") {
            Some(device.snapshot()?)
        } else {
            None
        };
    let identity = if let Some(state) = &radio_state {
        state.identity.clone()
    } else {
        device.read(0x82, 1, 6)?
    };
    let model = identify(&identity, device.transport)?;
    if let Some(backup) = &restoration
        && backup[..6] != identity
    {
        return Err("backup belongs to another keyboard model".into());
    }
    println!("{} identity: {identity:02x?}", model.name());
    let (read_operation, length) = if matrix_command {
        (0x83, 512)
    } else {
        (0x84, 128)
    };
    let original = if let Some(state) = &radio_state {
        if matrix_command {
            state
                .layers
                .get(layer as usize)
                .cloned()
                .ok_or("radio configuration supports layers 0 and 1 only")?
        } else {
            state.leds.clone()
        }
    } else {
        device.read(read_operation, layer, length)?
    };
    if command == "inspect" {
        println!("LED configuration: {original:02x?}");
        return Ok(());
    }
    if command == "keys" {
        for (index, (label, slot)) in KEYS.iter().enumerate() {
            println!(
                "{:2} {:10} slot={:3} code={:02x?}",
                index + 1,
                label,
                slot,
                &original[slot * 4..slot * 4 + 4]
            );
        }
        return Ok(());
    }
    let changed = match command {
        "led-effect" => led_effect(original.as_slice().try_into()?, args[1].parse()?)?.to_vec(),
        "remap" if original.len() == 504 => remap_matrix::<504>(
            original.as_slice().try_into()?,
            args[2].parse()?,
            args[3].parse()?,
        )?
        .to_vec(),
        "remap" => remap_matrix::<512>(
            original.as_slice().try_into()?,
            args[2].parse()?,
            args[3].parse()?,
        )?
        .to_vec(),
        "restore-led" => restoration.as_ref().ok_or("missing backup")?[6..].to_vec(),
        "restore-keys" => restoration.as_ref().ok_or("missing backup")?[7..].to_vec(),
        _ => unreachable!(),
    };
    if changed.len() != original.len() {
        return Err(
            "backup uses another transport; restore it using the connection where it was saved"
                .into(),
        );
    }
    if !matrix_command
        && changed != original
        && changed[9] == 0
        && !aula_f87pro::LED_EFFECTS.contains(&changed[10])
    {
        return Err("lighting effect is unavailable in the original app for this keyboard".into());
    }
    if changed == original {
        println!("Configuration already matches.");
        return Ok(());
    }
    if let Some(current) = radio_state {
        let mut desired = current.clone();
        if matrix_command {
            desired.layers[layer as usize] = changed;
        } else {
            desired.leds = changed;
        }
        let applied = apply_state(&device, desired, &current, save_snapshot)?;
        println!(
            "Configuration applied and read back successfully. Backup: {}",
            applied.backup.as_deref().unwrap_or("no change")
        );
        return Ok(());
    }
    let backup = format!(
        "aula-f87pro-{}-{}.bin",
        if matrix_command { "keys" } else { "led" },
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let mut saved = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)?;
    saved.write_all(&identity)?;
    if matrix_command {
        saved.write_all(&[layer])?;
    }
    saved.write_all(&original)?;
    saved.sync_all()?;
    File::open(env::current_dir()?)?.sync_all()?;
    println!(
        "Original configuration saved to {}",
        env::current_dir()?.join(&backup).display()
    );
    device
        .write_part(
            if matrix_command {
                Part::Matrix(layer)
            } else {
                Part::Leds
            },
            &changed,
        )
        .map_err(|error| format!("{error}; backup: {backup}"))?;
    println!("Configuration applied and read back successfully.");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct FakeDevice {
        state: RefCell<Snapshot>,
        calls: Cell<usize>,
        fail: Vec<usize>,
        parts: RefCell<Vec<Part>>,
    }

    impl StateDevice for FakeDevice {
        fn snapshot(&self) -> io::Result<Snapshot> {
            Ok(self.state.borrow().clone())
        }
        fn write_part(&self, part: Part, data: &[u8]) -> io::Result<()> {
            self.parts.borrow_mut().push(part);
            let call = self.calls.get() + 1;
            self.calls.set(call);
            let mut state = self.state.borrow_mut();
            let part = match part {
                Part::Matrix(layer) => &mut state.layers[layer as usize],
                Part::Leds => &mut state.leds,
                Part::Macros => &mut state.macros,
            };
            if self.fail.contains(&call) {
                // A failed transport can already have changed part of the hardware buffer.
                part[..2].copy_from_slice(&data[..2]);
                return Err(io::Error::other("injected USB failure"));
            }
            part.copy_from_slice(data);
            Ok(())
        }
    }

    fn device(fail: Vec<usize>) -> FakeDevice {
        FakeDevice {
            state: RefCell::new(Snapshot {
                version: 1,
                identity: vec![3, 0, 0, 0, 1, 11],
                leds: vec![0; 128],
                layers: vec![vec![0; 512]; 4],
                macros: vec![0; 4096],
            }),
            calls: Cell::new(0),
            fail,
            parts: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn identifies_only_declared_models_and_keeps_standard_f87_radio_disabled() {
        assert_eq!(
            identify(&[3, 0, 0, 0, 0, 0x8f], Transport::UsbCable).unwrap(),
            Model::F87
        );
        for transport in [Transport::UsbCable, Transport::Receiver24G] {
            assert_eq!(
                identify(&[3, 0, 0, 0, 1, 11], transport).unwrap(),
                Model::F87Pro
            );
            for identity in [
                vec![3, 0, 0, 0, 0, 0xa9],
                vec![3, 0, 0, 0, 0, 0],
                vec![3, 0, 0],
                vec![3, 0, 0, 0, 1, 11, 0],
            ] {
                assert!(identify(&identity, transport).is_err());
            }
        }
        assert!(identify(&[3, 0, 0, 0, 0, 0x8f], Transport::Receiver24G).is_err());
    }

    #[test]
    fn apply_saves_before_writing_and_changes_only_required_parts() {
        let device = device(vec![]);
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.layers[2][0] = 5;
        desired.leds[10] = 12;
        let result = apply_state(&device, desired.clone(), &original, |saved| {
            assert_eq!(saved, &original);
            assert_eq!(device.calls.get(), 0);
            Ok("backup.json".into())
        })
        .unwrap();
        assert_eq!(device.calls.get(), 2);
        assert_eq!(device.snapshot().unwrap(), desired);
        assert!(result.changed);
        assert_eq!(result.backup.as_deref(), Some("backup.json"));
        assert_eq!(result.state, desired);
    }

    #[test]
    fn standard_f87_usb_applies_and_restores_lighting_with_its_own_identity() {
        let device = device(vec![]);
        device.state.borrow_mut().identity = vec![3, 0, 0, 0, 0, 0x8f];
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.leds[10] = 17;
        apply_state(&device, desired.clone(), &original, |saved| {
            assert_eq!(saved.identity, original.identity);
            assert_eq!(device.calls.get(), 0);
            Ok("f87-backup.json".into())
        })
        .unwrap();
        assert_eq!(*device.parts.borrow(), [Part::Leds]);
        apply_state(&device, original.clone(), &desired, |_| {
            Ok("undo.json".into())
        })
        .unwrap();
        assert_eq!(device.snapshot().unwrap(), original);
    }

    #[test]
    fn cross_model_profiles_never_backup_or_write() {
        for identity in [vec![3, 0, 0, 0, 0, 0x8f], vec![3, 0, 0, 0, 1, 11]] {
            let device = device(vec![]);
            let mut desired = device.snapshot().unwrap();
            device.state.borrow_mut().identity = identity;
            let original = device.snapshot().unwrap();
            desired.identity = if original.identity[4] == 0 {
                vec![3, 0, 0, 0, 1, 11]
            } else {
                vec![3, 0, 0, 0, 0, 0x8f]
            };
            desired.leds[10] = 2;
            assert!(
                apply_state(&device, desired, &original, |_| panic!(
                    "foreign profile backed up"
                ))
                .is_err()
            );
            assert_eq!(device.calls.get(), 0);
            assert_eq!(device.snapshot().unwrap(), original);
        }
    }

    #[test]
    fn profiles_cannot_select_effects_hidden_by_original_app() {
        let device = device(vec![]);
        let original = device.snapshot().unwrap();
        for effect in [6, 9, 14, 18] {
            let mut desired = original.clone();
            desired.leds[10] = effect;
            assert!(
                apply_state(&device, desired, &original, |_| panic!(
                    "masked effect backed up"
                ))
                .is_err()
            );
        }
        assert_eq!(device.calls.get(), 0);
    }

    #[test]
    fn radio_changes_preserve_unread_memory_and_reject_macro_edits_before_backup() {
        let device = device(vec![]);
        {
            let mut state = device.state.borrow_mut();
            state.version = 2;
            state.layers.truncate(2);
            for layer in &mut state.layers {
                layer.truncate(504);
            }
            state.macros.truncate(512);
        }
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.leds[10] = 1;
        desired.set_key_code(0, 13, [0, 0, 0, 4]).unwrap();
        apply_state(&device, desired.clone(), &original, |_| {
            Ok("radio-backup.json".into())
        })
        .unwrap();
        assert_eq!(*device.parts.borrow(), [Part::Matrix(0), Part::Leds]);
        assert_eq!(device.snapshot().unwrap().macros, original.macros);
        let current = device.snapshot().unwrap();
        let mut wrong_led_backup = current.clone();
        wrong_led_backup.leds[14] ^= 1;
        assert!(
            apply_state(&device, wrong_led_backup, &current, |_| panic!(
                "foreign LED backup written"
            ))
            .is_err()
        );
        for edit_buffer in [true, false] {
            let mut invalid = current.clone();
            if edit_buffer {
                invalid.macros[1] = 3;
            } else {
                invalid.set_key_code(0, 13, [3, 1, 1, 0]).unwrap();
            }
            assert!(
                apply_state(&device, invalid, &current, |_| panic!(
                    "unsupported macro edit backed up"
                ))
                .is_err()
            );
        }
        let cable = super::tests::device(vec![]).snapshot().unwrap();
        assert!(
            apply_state(&device, cable, &current, |_| panic!(
                "incompatible profile backed up"
            ))
            .is_err()
        );
    }

    #[test]
    fn radio_device_rejects_unavailable_layers_without_accessing_hardware() {
        let device = Device {
            file: File::open("/dev/null").unwrap(),
            transport: Transport::Receiver24G,
        };
        for layer in 2..=3 {
            assert!(device.read(0x83, layer, 512).is_err());
            assert_eq!(
                device
                    .write_part(Part::Matrix(layer), &[0; 504])
                    .unwrap_err()
                    .kind(),
                ErrorKind::Unsupported
            );
        }
    }

    #[test]
    fn stale_expected_noop_and_backup_failure_never_write() {
        let device = device(vec![]);
        let original = device.snapshot().unwrap();
        let mut changed = original.clone();
        changed.leds[10] = 1;
        assert!(
            apply_state(&device, changed.clone(), &changed, |_| panic!(
                "stale state backed up"
            ))
            .is_err()
        );
        assert!(
            !apply_state(&device, original.clone(), &original, |_| panic!(
                "no-op backed up"
            ))
            .unwrap()
            .changed
        );
        assert!(
            apply_state(&device, changed, &original, |_| Err(io::Error::other(
                "disk full"
            )))
            .is_err()
        );
        assert_eq!(device.calls.get(), 0);
        assert_eq!(device.snapshot().unwrap(), original);
    }

    #[test]
    fn partial_write_failure_restores_full_original_state() {
        let device = device(vec![2]);
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.layers[0][0] = 10;
        desired.layers[1][0] = 20;
        desired.leds[10] = 1;
        let error =
            apply_state(&device, desired, &original, |_| Ok("backup.json".into())).unwrap_err();
        assert!(error.to_string().contains("original state restored"));
        assert_eq!(device.snapshot().unwrap(), original);
        assert_eq!(device.calls.get(), 5);
    }

    #[test]
    fn rollback_failure_is_explicit_and_remaining_parts_are_attempted() {
        let device = device(vec![2, 3]);
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.layers[0][0] = 10;
        desired.layers[1][0] = 20;
        desired.leds[10] = 1;
        let error =
            apply_state(&device, desired, &original, |_| Ok("backup.json".into())).unwrap_err();
        assert!(error.to_string().contains("ROLLBACK FAILED"));
        assert!(error.to_string().contains("backup.json"));
        assert_eq!(device.calls.get(), 5);
    }

    #[test]
    fn macro_data_is_written_before_binding_and_restored_after_unbinding() {
        let device = device(vec![2]);
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.macros = valid_macro_buffer();
        desired.layers[0][0] = 10;
        let error =
            apply_state(&device, desired, &original, |_| Ok("backup.json".into())).unwrap_err();
        assert!(error.to_string().contains("original state restored"));
        assert_eq!(
            *device.parts.borrow(),
            vec![Part::Macros, Part::Matrix(0), Part::Matrix(0), Part::Macros]
        );
        assert_eq!(device.snapshot().unwrap(), original);
    }

    fn valid_macro_buffer() -> Vec<u8> {
        use aula_f87pro::macros::{MacroDefinition, MacroEvent, replace_macro};
        replace_macro(
            &[0; 4096],
            0,
            &MacroDefinition {
                name: "A".into(),
                events: vec![
                    MacroEvent {
                        key: 4,
                        pressed: true,
                        delay_ms: 0,
                    },
                    MacroEvent {
                        key: 4,
                        pressed: false,
                        delay_ms: 5,
                    },
                ],
            },
        )
        .unwrap()
        .to_vec()
    }

    #[test]
    fn invalid_macro_tables_and_dangling_references_never_backup_or_write() {
        let device = device(vec![]);
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.macros[0] = 1;
        assert!(
            apply_state(&device, desired, &original, |_| panic!(
                "invalid table backed up"
            ))
            .is_err()
        );
        let mut desired = original.clone();
        desired.layers[0][..4].copy_from_slice(&[3, 1, 1, 0]);
        assert!(
            apply_state(&device, desired, &original, |_| panic!(
                "dangling reference backed up"
            ))
            .is_err()
        );
        let mut desired = original.clone();
        desired.macros = valid_macro_buffer();
        desired.layers[0][..4].copy_from_slice(&[3, 1, 1, 1]);
        assert!(
            apply_state(&device, desired, &original, |_| panic!(
                "out-of-bounds reference backed up"
            ))
            .is_err()
        );
        assert_eq!(device.calls.get(), 0);
    }

    #[test]
    fn opaque_unchanged_macros_allow_other_changes_but_no_new_macro_bindings() {
        let device = device(vec![]);
        device.state.borrow_mut().macros[0] = 1;
        device.state.borrow_mut().layers[0][..4].copy_from_slice(&[3, 1, 1, 20]);
        let original = device.snapshot().unwrap();
        let mut desired = original.clone();
        desired.leds[10] = 1;
        apply_state(&device, desired.clone(), &original, |_| {
            Ok("backup.json".into())
        })
        .unwrap();
        let current = device.snapshot().unwrap();
        desired.layers[0][4..8].copy_from_slice(&[3, 1, 1, 20]);
        assert!(
            apply_state(&device, desired, &current, |_| panic!(
                "unknown table backed up"
            ))
            .is_err()
        );
        assert_eq!(device.calls.get(), 1);
    }
}
