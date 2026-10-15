First public preview of **AULA F87 Pro for Linux**, a native Rust/GTK4 companion.

- Aligned TKL layout and an animated RGB preview for the selected profile.
- USB lighting, key assignments, shortcuts and macro editing.
- Experimental 2.4 GHz lighting and key assignments on layers 0/1.
- Local profiles, RGB-only loading between connections, backup, readback and undo.

### Linux downloads

- **Arch Linux:** install the `.pkg.tar.zst` with `pacman -U`.
- **Ubuntu 24.04+ / Debian 13+:** install the `.deb` with `apt install ./filename.deb`.
- **Other compatible Linux x86_64 systems:** extract the `.tar.gz` and run
  `python3 install.py --udev`. Requires GTK4 ≥4.10 and glibc ≥2.39.
- `PKGBUILD` and `SHA256SUMS` are included. The archive is not a self-contained AppImage.

Open **AULA F87 Pro** from the application menu after installing. Reconnect the
keyboard or receiver so the device-access rules take effect. Run the app as your
normal desktop user. Close an older instance before updating.

Radio support is experimental: reads can be intermittent. Radio macro editing
and layers 2/3 require USB. Bluetooth and firmware flashing are not implemented.
The RGB preview is illustrative, and physical macro timing remains unverified.

[English guide](https://github.com/meursault03/aula-f87pro-linux#readme) ·
[Guia em português](https://github.com/meursault03/aula-f87pro-linux/blob/main/README.pt.md)
