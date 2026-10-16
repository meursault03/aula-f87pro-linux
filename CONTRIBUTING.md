# Development

Linux x86_64, Rust ≥ 1.92, GTK4 ≥ 4.10, Python ≥ 3.11 and a C toolchain are required.
The interface supports Portuguese and English; each README uses screenshots in its own language.

Install build dependencies:

```sh
# Arch Linux
sudo pacman -S --needed base-devel rust gtk4 pkgconf python
# Ubuntu 24.04 / Debian 13
sudo apt install build-essential pkg-config libgtk-4-dev python3
```

Build both binaries together: the GUI calls its sibling CLI executable.

```sh
cargo build --locked --release --features gui --bins
./target/release/aula-f87pro-gui
```

`python3 scripts/install_desktop.py` builds and installs an optimized copy for the
current user. It accepts the installer's `--prefix` and `--udev` options. Profiles
and existing backups are preserved. Close an older running instance before opening
an updated build.

## Checks

```sh
cargo fmt --check
cargo test --locked --features gui
cargo clippy --locked --all-targets --features gui -- -D warnings
python3 -m unittest discover -s tests -p 'test_*.py' -v
```

The native GTK smoke test uses Xvfb, D-Bus, xdotool and ImageMagick. Install those
tools, build debug binaries with `cargo build --locked --features gui --bins`, then run:

```sh
xvfb-run -a -s '-screen 0 1440x1600x24' env -u WAYLAND_DISPLAY GTK_THEME=Adwaita:dark dbus-run-session -- python3 tests/gui_smoke.py
xvfb-run -a -s '-screen 0 1440x1600x24' env -u WAYLAND_DISPLAY GTK_THEME=Adwaita:dark F87_TEST_RADIO=1 dbus-run-session -- python3 tests/gui_smoke.py
```

`F87_TEST_ENGLISH=1` tests the English interface.
`F87_TEST_REDUCED_MOTION=1` checks that the preview stays still. These tests use
isolated demonstration state and a simulated CLI: **they never write to HID**.

## Linux packages

The release workflow builds on Ubuntu 24.04, tests the app, and packages the same
optimized binaries into a tar archive, a Debian package and an Arch package.
It publishes a prerelease only after all jobs pass. Tags must match Cargo's version.

```sh
python3 scripts/package.py --format tar
# Needs dpkg-deb and readelf; use Ubuntu 24.04 for distributable Debian binaries.
python3 scripts/package.py --format deb
python3 scripts/package.py --arch-recipe dist/aula-f87pro-0.2.0-linux-x86_64.tar.gz
```

The generated `PKGBUILD` verifies the exact release archive by SHA-256. On Arch,
place it beside that archive and run `makepkg`; the CI does the same in a clean
Arch container. Run package tests with temporary install prefixes rather than
installing an unreviewed system package on your workstation.

## Protocol changes

Use the observed identity and report descriptor before any device-specific write.
Preserve bytes outside the feature being changed. Back up before writing, reject
stale expected state, verify by reading back, and keep recovery failures visible.
Add captured protocol fixtures and a focused regression test for new packet shapes.
Do not replace incomplete radio reads with zero-filled USB buffers.

Private research notes, proprietary installers, firmware, machine captures and
personal profiles do not belong in a pull request. Tests contain only the small
reference payloads documented in [tests/fixtures/README.md](tests/fixtures/README.md).

Open an issue before adding other devices or firmware flashing. This project is
currently dedicated to the AULA F87 Pro.
