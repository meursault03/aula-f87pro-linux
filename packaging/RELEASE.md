**AULA F87 Pro for Linux 0.3.0**, a native Rust/GTK4 companion.

- Experimental USB support for the standard AULA F87, with the detected model shown in the app.
- Corrected the lighting list to match the 14 effects enabled by the original software.
  Rainbow wheel, Trail, Sweep and Storm are no longer offered for these models.
- Profiles and backups from another keyboard model cannot be applied.
- Shorter English and Portuguese guides focused on installation and everyday use.

### Linux downloads

- **Arch Linux:** install the `.pkg.tar.zst` with `pacman -U`.
- **Ubuntu 24.04+ / Debian 13+:** install the `.deb` with `apt install ./filename.deb`.
- **Other compatible Linux x86_64 systems:** extract the `.tar.gz` and run
  `python3 install.py --udev`. Requires GTK4 ≥4.10 and glibc ≥2.39.
- `PKGBUILD` and `SHA256SUMS` are included. The archive is not a self-contained AppImage.

Open **AULA F87 Pro** from the application menu. Reconnect the keyboard or receiver
after installing device-access rules. Close an older instance before updating.
Profiles and app preferences remain local.

Standard F87 support has not yet been validated on physical hardware and requires
USB. F87 Pro radio support remains experimental; macro editing requires USB.
Bluetooth, firmware flashing and per-key RGB customization are not available.

[English guide](https://github.com/meursault03/aula-f87pro-linux#readme) ·
[Guia em português](https://github.com/meursault03/aula-f87pro-linux/blob/main/README.pt.md)
