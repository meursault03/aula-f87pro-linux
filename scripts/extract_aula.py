"""Recover selected files from the supplied AULA installer without running it."""
import hashlib
import lzma
from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1]


def decompress(raw):
    prop = raw[0]
    decoder = lzma.LZMADecompressor(format=lzma.FORMAT_RAW, filters=[{
        "id": lzma.FILTER_LZMA1, "dict_size": int.from_bytes(raw[1:5], "little"),
        "lc": prop % 9, "lp": prop // 9 % 5, "pb": prop // 45,
    }])
    result = decoder.decompress(raw[5:], max_length=16 * 1024 * 1024)
    assert decoder.eof, "Incomplete or oversized compressed stream"
    return result


def metadata_block(source, offset):
    header = source[offset:offset + 9]
    assert zlib.crc32(header[4:]) == int.from_bytes(header[:4], "little")
    size = int.from_bytes(header[4:8], "little")
    assert header[8] == 1 and offset + 9 + size <= len(source)
    raw = bytearray()
    for start in range(offset + 9, offset + 9 + size, 4100):
        block = source[start:min(start + 4100, offset + 9 + size)]
        assert zlib.crc32(block[4:]) == int.from_bytes(block[:4], "little")
        raw.extend(block[4:])
    return decompress(raw), offset + 9 + size


def undo_exe_filter(data):
    # Inno 5.3.3 uses the pre-5.3.9 filter described by innoextract exefilter.hpp.
    data = bytearray(data)
    i = 0
    while i + 5 <= len(data):
        if data[i] in (0xE8, 0xE9) and i % 65536 <= 65531:
            if data[i + 4] in (0, 255):
                address = (int.from_bytes(data[i + 1:i + 4], "little") - i - 5) & 0xFFFFFF
                data[i + 1:i + 4] = address.to_bytes(3, "little")
            i += 5
        else:
            i += 1
    return data


def main():
    source = (ROOT / "F87Pro Software.exe").read_bytes()
    # shortcut: offsets apply only to this installer; use innoextract for other builds.
    assert hashlib.sha256(source).hexdigest() == "a91db0e84dd6e677aaa1701a6c4851e71131b561815909aacfc6f8679d20fdee"
    header, next_block = metadata_block(source, 0x2D4EE6 + 64)
    locations, _ = metadata_block(source, next_block)
    assert b"O\x00e\x00m\x00D\x00r\x00v\x00.\x00e\x00x\x00e\x00" in header
    payload = decompress(source[0x44A04:])
    files = {
        "OemDrv.exe": 0x5DC4A2,
        "Update-F87.exe": 0xEE2FA,
        "Update-wired.exe": 0x36EB79,
        "uninstall.dll": 0x5A8F8A,
        "InitSetup.dll": 0x5CE38A,
        "F87PRO-KB.ini": 0x2FAB81,
        "wired-KB.ini": 0x32DC9C,
    }
    verified = {}
    for name, offset in files.items():
        index = locations.find(struct.pack("<Q", offset))
        assert index >= 0
        size = int.from_bytes(locations[index + 8:index + 16], "little")
        assert 0 < size <= len(payload) - offset
        data = payload[offset:offset + size]
        if name.endswith((".exe", ".dll")):
            data = undo_exe_filter(data)
            assert data[:2] == b"MZ"
        assert hashlib.md5(data).digest() == locations[index + 24:index + 40], name
        verified[name] = data
    destination = ROOT / "analysis" / "verified"
    destination.mkdir(parents=True, exist_ok=True)
    for name, data in verified.items():
        (destination / name).write_bytes(data)
        print(name, len(data), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
