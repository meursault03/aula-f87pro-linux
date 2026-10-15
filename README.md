# AULA F87 Pro for Linux

English · [Português](README.pt.md)

A native Linux companion for the **AULA F87 Pro**. Set the lighting, remap keys,
prepare macros over USB, and keep named profiles on your computer. Built with
Rust and GTK4, using a protocol reconstructed from the original desktop driver.

[Download Linux preview](https://github.com/meursault03/aula-f87pro-linux/releases/tag/v0.1.0) ·
[Install](#install-on-linux) · [Connection support](#connection-support) ·
[Build from source](CONTRIBUTING.md)

![AULA F87 Pro Linux app with an aligned TKL keyboard and an RGB lighting preview](assets/screenshots/lighting.png)

*Native GTK interface with demonstration data. The animated RGB preview illustrates
the selected configuration; it does not stream the physical LEDs. The interface
is currently in Portuguese.*

## Install on Linux

**Version 0.1.0 is an experimental preview for Linux x86_64.**
Download a package from [Releases](https://github.com/meursault03/aula-f87pro-linux/releases/tag/v0.1.0):

| System | Download | Install |
| --- | --- | --- |
| Arch Linux | `aula-f87pro-0.1.0-1-x86_64.pkg.tar.zst` | `sudo pacman -U ./aula-f87pro-0.1.0-1-x86_64.pkg.tar.zst` |
| Ubuntu 24.04+ / Debian 13+ | `aula-f87pro_0.1.0_amd64.deb` | `sudo apt install ./aula-f87pro_0.1.0_amd64.deb` |
| Other compatible distributions | `aula-f87pro-0.1.0-linux-x86_64.tar.gz` | Extract and run `python3 install.py --udev` inside its folder. |

The archive needs GTK4 ≥ 4.10, glibc ≥ 2.39 and Python 3 for installation; it is
not a self-contained AppImage. Its installer writes to `~/.local/bin` and
`~/.local/share`. `--udev` uses the system's graphical authentication dialog to
install device-access rules. Omit that option if the rules are already installed.

The Arch and Debian packages include the desktop launcher, icon and udev rules.
Reconnect the keyboard or receiver after installation, then open **AULA F87 Pro**
from your application menu. Run the app as your regular desktop user.
Checksums are in `SHA256SUMS` beside the downloads.

## Configure your keyboard

1. Connect by USB, or select **2.4G** on the keyboard with its receiver inserted.
   Press a key if the keyboard is asleep and wait for the state to load.
2. Choose an effect, brightness, speed and palette in **Iluminação**. The preview
   updates immediately; the hardware changes only after **Aplicar alterações**.
3. In **Teclas**, select a key and prepare a key, shortcut, media action or
   disabled assignment. You can prepare several edits before applying them.
4. **Desfazer última alteração** restores the last confirmed configuration.
5. Save named profiles in **Perfis e backups**. Loading a profile stages edits.
   **Carregar só RGB** transfers lighting between USB and receiver profiles
   while preserving keys, macros and connection-specific fields.

The app saves a backup before writing, checks the device again before applying,
and verifies the result by reading it back. A failed operation attempts to
restore the original accessible state. Drafts stay local until you apply them.

| Key assignments | Profiles and recovery |
| --- | --- |
| ![Physical keyboard and current key assignment](assets/screenshots/keys.png) | ![Named local profiles and recovery actions](assets/screenshots/profiles.png) |

## Connection support

| Feature | USB cable | 2.4 GHz receiver |
| --- | --- | --- |
| Lighting: 18 effects, brightness, speed and palette | Supported | Supported, experimental |
| Key assignments and shortcuts | Layers 0–3 | Layers 0–1 |
| Macro editing and new macro bindings | Supported | Preserved; editing requires USB |
| Named profiles, RGB preview and undo | Supported | Supported for accessible state |

Supported interfaces: USB `258a:010c`, receiver `3554:fa09`, and keyboard identity
`03 00 00 00 01 0b`. When both connections are present, the app prefers USB.
The receiver's presence alone does not mean that the keyboard is awake.

Radio reads can be intermittent. The app retries missing fragments and requires
two complete equal reads before accepting a block. Incomplete state does not
permit a write. Radio backups contain the two exposed key layers and the macro
memory prefix actually read; unread layers and macro banks are never written.
Full profiles use the format of the connection where they were saved.

**Bluetooth, firmware flashing and arbitrary per-key RGB are not implemented.**
The timing and every playback mode of macros still need physical validation.

## Macros over USB

Build a sequence of key presses, releases and delays, select its target key,
and use **Preparar macro** before applying. Every pressed key must be released.
Existing unsupported event types are kept intact rather than rewritten.

![USB macro editor with a demonstration press-and-release sequence](assets/screenshots/macros.png)

*Demonstration macro, captured with an isolated simulated backend. Screenshots do
not represent a hardware test or a global key recorder.*

## Local data and troubleshooting

Profiles, backups and undo data live in `$XDG_DATA_HOME/aula-f87pro`
(default `~/.local/share/aula-f87pro`). Nothing is uploaded. Profiles can contain
personal macro sequences: inspect them before sharing them in an issue.

| Problem | What to check |
| --- | --- |
| Receiver detected, keyboard unavailable | Check the 2.4G switch, press a key, and retry **Recarregar teclado**. |
| Permission error | Install the included udev rules and reconnect the device. Use a local desktop session. |
| Profile cannot be applied on this connection | Use **Carregar só RGB**, or connect using the mode where the full profile was saved. |
| Hardware changed outside the app | Save the draft as a profile, then reload or discard it before applying. |
| Old window after an update | Close the previous instance, then open the updated app. |

For a bug report, include the app version, distribution, desktop, connection mode,
and the text in **Detalhes da última operação**. Do not attach personal profiles
or the proprietary Windows installer.

## Development and license

See [CONTRIBUTING.md](CONTRIBUTING.md) for builds, tests and Linux packaging.
No proprietary executable, firmware or private research notes are distributed.

MIT. Independent community software, not affiliated with AULA.
