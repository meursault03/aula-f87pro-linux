//! CDev3632 radio transport. The caller must lock and verify the receiver node.
use std::{
    fs::File,
    io::{self, ErrorKind, Read, Write},
    os::fd::AsRawFd,
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_millis(1500);

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}
unsafe extern "C" {
    fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn packet(command: u8, argument: u8, total: u8, index: u8, data: &[u8]) -> io::Result<[u8; 20]> {
    if argument > 15 || total == 0 || total > 127 || index >= total || data.len() > 14 {
        return Err(invalid("invalid wireless packet fields"));
    }
    let mut packet = [0; 20];
    packet[..5].copy_from_slice(&[
        0x13,
        command,
        total,
        index,
        argument << 4 | data.len() as u8,
    ]);
    packet[5..5 + data.len()].copy_from_slice(data);
    packet[19] = packet[..19]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    Ok(packet)
}

fn validate(packet: &[u8]) -> io::Result<()> {
    if packet.len() != 20
        || packet[0] != 0x13
        || packet[2] == 0
        || packet[2] > 127
        || packet[3] >= packet[2]
        || packet[4] & 15 > 14
        || packet[19]
            != packet[..19]
                .iter()
                .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
    {
        return Err(invalid(
            "invalid wireless response length, header or checksum",
        ));
    }
    Ok(())
}

struct Fragments {
    command: u8,
    argument: u8,
    parts: Vec<Option<Vec<u8>>>,
}
impl Fragments {
    fn new(command: u8, argument: u8) -> Self {
        Self {
            command,
            argument,
            parts: Vec::new(),
        }
    }
    fn push(&mut self, packet: &[u8]) -> io::Result<Option<Vec<u8>>> {
        validate(packet)?;
        if packet[1] != self.command || packet[4] >> 4 != self.argument {
            return Err(invalid("wireless response does not match query"));
        }
        if self.parts.is_empty() {
            self.parts.resize(packet[2] as usize, None);
        }
        if packet[2] as usize != self.parts.len() {
            return Err(invalid("wireless fragment count changed"));
        }
        let data = packet[5..5 + (packet[4] & 15) as usize].to_vec();
        let slot = &mut self.parts[packet[3] as usize];
        if slot.as_ref().is_some_and(|old| old != &data) {
            return Err(invalid(&format!(
                "conflicting duplicate wireless fragment {}",
                packet[3]
            )));
        }
        *slot = Some(data);
        Ok(self.parts.iter().all(Option::is_some).then(|| {
            self.parts
                .iter()
                .flat_map(|part| part.as_ref().unwrap().iter().copied())
                .collect()
        }))
    }
}

fn ready(file: &File, deadline: Instant) -> io::Result<bool> {
    loop {
        let mut descriptor = PollFd {
            fd: file.as_raw_fd(),
            events: 1,
            revents: 0,
        };
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(i32::MAX as u128) as i32;
        // poll owns no buffer and this descriptor remains alive for the call.
        let result = unsafe { poll(&mut descriptor, 1, timeout) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if descriptor.revents & (8 | 16 | 32) != 0 {
            return Err(io::Error::new(
                ErrorKind::NotConnected,
                "wireless receiver disconnected",
            ));
        }
        return Ok(result > 0 && descriptor.revents & 1 != 0);
    }
}

fn receive(file: &File, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut buffer = [0; 1024];
    loop {
        if !ready(file, deadline)? {
            return Err(io::Error::new(
                ErrorKind::TimedOut,
                "wireless response timed out; keyboard may be asleep or disconnected",
            ));
        }
        let length = (&*file).read(&mut buffer)?;
        if length == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "wireless receiver closed",
            ));
        }
        // Ignore keyboard, media and status reports. They are never logged.
        if buffer[0] == 0x13 {
            return Ok(buffer[..length].to_vec());
        }
    }
}

fn drain(file: &File) -> io::Result<()> {
    let mut buffer = [0; 1024];
    for _ in 0..512 {
        if !ready(file, Instant::now())? {
            return Ok(());
        }
        let _ = (&*file).read(&mut buffer)?;
    }
    Err(invalid("wireless input queue did not drain"))
}

fn send(file: &File, packet: &[u8; 20]) -> io::Result<()> {
    let written = (&*file).write(packet)?;
    if written != packet.len() {
        return Err(io::Error::new(
            ErrorKind::WriteZero,
            "short wireless output report",
        ));
    }
    Ok(())
}

fn receive_query(file: &File, fragments: &mut Fragments, expected: usize) -> io::Result<Vec<u8>> {
    let command = fragments.command;
    let argument = fragments.argument;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let response = receive(file, deadline).map_err(|cause| {
            io::Error::new(
                cause.kind(),
                format!(
                    "wireless query {command:02x}/{argument} received {}/{} fragments: {cause}",
                    fragments.parts.iter().filter(|part| part.is_some()).count(),
                    fragments.parts.len()
                ),
            )
        })?;
        if response.get(1) != Some(&command)
            || response.get(4).is_some_and(|field| field >> 4 != argument)
        {
            continue;
        }
        if let Some(data) = fragments.push(&response)? {
            return if data.len() == expected {
                Ok(data)
            } else {
                Err(invalid(
                    "wireless payload length differs from expected layout",
                ))
            };
        }
    }
}

fn query(file: &File, command: u8, argument: u8, expected: usize) -> io::Result<Vec<u8>> {
    let request = packet(command, argument, 1, 0, &[])?;
    let mut error = invalid("incomplete wireless response");
    let mut fragments = Fragments::new(command, argument);
    let mut verified = None;
    for _ in 0..8 {
        drain(file)?;
        // Read continuously while the output call completes, as the original driver does.
        let started = std::sync::Barrier::new(2);
        let result = thread::scope(|scope| {
            let barrier = &started;
            let fragments = &mut fragments;
            let reader = scope.spawn(move || {
                barrier.wait();
                receive_query(file, fragments, expected)
            });
            barrier.wait();
            let sent = send(file, &request);
            let received = reader
                .join()
                .map_err(|_| io::Error::other("wireless reader failed"))?;
            sent?;
            received
        });
        thread::sleep(Duration::from_millis(70));
        match result {
            Ok(data) => {
                if verified.as_ref() == Some(&data) {
                    return Ok(data);
                }
                verified = Some(data);
                fragments = Fragments::new(command, argument);
            }
            Err(cause) => {
                if cause.kind() != ErrorKind::TimedOut {
                    fragments = Fragments::new(command, argument);
                    verified = None;
                }
                error = cause;
            }
        }
    }
    Err(error)
}

fn write_data(file: &File, command: u8, argument: u8, data: &[u8]) -> io::Result<()> {
    let total =
        u8::try_from(data.len().div_ceil(14)).map_err(|_| invalid("wireless write too large"))?;
    drain(file)?;
    for (index, fragment) in data.chunks(14).enumerate() {
        let request = packet(command, argument, total, index as u8, fragment)?;
        thread::sleep(Duration::from_millis(15));
        send(file, &request)?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let ack = receive(file, deadline)?;
            if ack.get(1) != Some(&command) {
                continue;
            }
            validate(&ack)?;
            if ack[3] != index as u8 || ack[4] >> 4 != argument {
                continue;
            }
            break;
        }
    }
    thread::sleep(Duration::from_millis(70));
    Ok(())
}

pub fn read_identity(file: &File) -> io::Result<Vec<u8>> {
    Ok(query(file, 5, 0, 10)?[..6].to_vec())
}
pub fn read_leds(file: &File) -> io::Result<Vec<u8>> {
    query(file, 0x44, 0, 128)
}
pub fn read_matrix(file: &File, layer: u8) -> io::Result<Vec<u8>> {
    if layer > 3 {
        return Err(invalid("layer must be in 0..3"));
    }
    query(file, 0x41, layer, 504)
}
pub fn write_leds(file: &File, data: &[u8]) -> io::Result<()> {
    if data.len() != 128 {
        return Err(invalid("LED configuration must be 128 bytes"));
    }
    write_data(file, 4, 0, data)
}
pub fn write_matrix(file: &File, layer: u8, data: &[u8]) -> io::Result<()> {
    if layer > 3 || data.len() != 504 {
        return Err(invalid(
            "invalid wireless matrix layer or length (expected 504 bytes)",
        ));
    }
    write_data(file, 1, layer, data)
}
pub fn read_macros(file: &File) -> io::Result<Vec<u8>> {
    // Only the 512 bytes actually returned by firmware; never fabricate the remaining USB memory.
    query(file, 0x43, 0, 512)
}
pub fn write_macros(_file: &File, _data: &[u8]) -> io::Result<()> {
    Err(io::Error::new(
        ErrorKind::Unsupported,
        "wireless macros require validated full-memory backup and readback",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovers_lost_fragments_and_requires_two_equal_complete_reads() {
        use std::{
            os::{fd::OwnedFd, unix::net::UnixDatagram},
            sync::atomic::{AtomicUsize, Ordering},
        };
        let (client, peer) = UnixDatagram::pair().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let file = File::from(OwnedFd::from(client));
        let requests = AtomicUsize::new(0);
        thread::scope(|scope| {
            let requests = &requests;
            scope.spawn(move || {
                for round in 0..4 {
                    let mut request = [0; 20];
                    assert_eq!(peer.recv(&mut request).unwrap(), 20);
                    assert_eq!(request[1], 0x41);
                    requests.fetch_add(1, Ordering::Relaxed);
                    // A delayed reply from another layer must not discard this layer's progress.
                    peer.send(&packet(0x41, 1, 1, 0, &[9; 14]).unwrap())
                        .unwrap();
                    for index in 0..36 {
                        if (round == 0 && index == 5) || (round == 1 && index == 8) {
                            continue;
                        }
                        let value = if round < 2 { 1 } else { 2 };
                        peer.send(&packet(0x41, 0, 36, index, &[value; 14]).unwrap())
                            .unwrap();
                    }
                }
            });
            assert_eq!(query(&file, 0x41, 0, 504).unwrap(), vec![2; 504]);
        });
        assert_eq!(requests.load(Ordering::Relaxed), 4);
    }
    #[test]
    fn identity_query_and_observed_response() {
        assert_eq!(
            packet(5, 0, 1, 0, &[]).unwrap(),
            [
                0x13, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x19
            ]
        );
        let response = [
            0x13, 5, 1, 0, 10, 3, 0, 0, 0, 1, 11, 1, 0, 0, 0, 0, 0, 0, 0, 0x33,
        ];
        assert_eq!(
            Fragments::new(5, 0).push(&response).unwrap().unwrap(),
            [3, 0, 0, 0, 1, 11, 1, 0, 0, 0]
        );
    }

    #[test]
    fn real_radio_fragments_reassemble_without_fabricating_matrix_trailer() {
        for (command, reports, expected) in [
            (
                0x44,
                &include_bytes!("../tests/fixtures/f87pro-radio-led-reports.bin")[..],
                &include_bytes!("../tests/fixtures/f87pro-radio-led-read.bin")[..],
            ),
            (
                0x41,
                &include_bytes!("../tests/fixtures/f87pro-radio-matrix-reports.bin")[..],
                &include_bytes!("../tests/fixtures/f87pro-radio-matrix-read.bin")[..],
            ),
        ] {
            let mut fragments = Fragments::new(command, 0);
            let mut complete = None;
            for report in reports.as_chunks::<20>().0 {
                complete = fragments.push(report).unwrap();
            }
            assert_eq!(complete.unwrap(), expected);
        }
    }
    #[test]
    fn reassembles_by_index_and_never_accepts_missing_or_conflicting_fragments() {
        let a = packet(0x44, 0, 2, 0, &[1; 14]).unwrap();
        let b = packet(0x44, 0, 2, 1, &[2; 2]).unwrap();
        let mut fragments = Fragments::new(0x44, 0);
        assert!(fragments.push(&b).unwrap().is_none());
        assert!(fragments.push(&b).unwrap().is_none());
        assert_eq!(
            fragments.push(&a).unwrap().unwrap(),
            [&[1; 14][..], &[2; 2][..]].concat()
        );
        assert!(
            fragments
                .push(&packet(0x44, 0, 2, 1, &[3; 2]).unwrap())
                .is_err()
        );
        let mut bad = a;
        bad[10] ^= 1;
        assert!(validate(&bad).is_err());
        assert!(Fragments::new(0x44, 1).push(&a).is_err());
        assert!(packet(1, 16, 1, 0, &[]).is_err());
        assert!(packet(1, 0, 128, 0, &[]).is_err());
    }
}
