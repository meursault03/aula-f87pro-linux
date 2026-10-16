**AULA F87 Pro for Linux 0.2.0**, a native Rust/GTK4 companion.

- Portuguese and English interfaces. Choose a language in Settings and reopen the app.
- Device-reported version, five debounce stages, idle sleep and experimental Tap sensitivity.
- Start at login, quit/minimize behavior and keyboard shortcuts for tab navigation.
- USB lighting, key assignments, macros and local profiles, with backup, readback and undo.
- Experimental 2.4 GHz lighting and key assignments on layers 0/1.

### Linux downloads

- **Arch Linux:** install the `.pkg.tar.zst` with `pacman -U`.
- **Ubuntu 24.04+ / Debian 13+:** install the `.deb` with `apt install ./filename.deb`.
- **Other compatible Linux x86_64 systems:** extract the `.tar.gz` and run
  `python3 install.py --udev`. Requires GTK4 ≥4.10 and glibc ≥2.39.
- `PKGBUILD` and `SHA256SUMS` are included. The archive is not a self-contained AppImage.

Open **AULA F87 Pro** from the application menu. Reconnect the keyboard or receiver
after installing device-access rules. Close an older instance before updating.
Profiles and app preferences remain local.

Radio support is experimental and reads can be intermittent. Radio macro editing
and layers 2/3 require USB. Bluetooth, firmware flashing and full factory reset are
not implemented. Restoring the three behavior settings preserves lighting, keys and
macros. Debounce stages are not verified millisecond values; Tap semantics, sleep
timing and macro timing/playback modes still need physical validation.

[English guide](https://github.com/meursault03/aula-f87pro-linux#readme) ·
[Guia em português](https://github.com/meursault03/aula-f87pro-linux/blob/main/README.pt.md)
