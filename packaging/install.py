#!/usr/bin/env python3
"""Install both binaries and desktop assets for the current user."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def install(binaries, assets, prefix):
    for name in ("aula-f87pro", "aula-f87pro-gui"):
        source = binaries / name
        if not source.is_file():
            raise FileNotFoundError(f"Missing executable: {source}")
    target = prefix / "bin"
    target.mkdir(parents=True, exist_ok=True)
    for name in ("aula-f87pro", "aula-f87pro-gui"):
        with tempfile.TemporaryDirectory(prefix=".aula-install-", dir=target) as folder:
            temporary = Path(folder) / name
            shutil.copy2(binaries / name, temporary)
            temporary.chmod(0o755)
            os.replace(temporary, target / name)
    icon = Path("icons/hicolor/scalable/apps/aula-f87pro.svg")
    destination = prefix / "share" / icon
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(assets / icon, destination)
    desktop = "applications/io.github.aula_f87pro.Control.desktop"
    executable = str(target / "aula-f87pro-gui")
    executable = executable.replace("\\", "\\\\").replace('"', '\\"').replace("`", "\\`").replace("$", "\\$").replace("%", "%%")
    text = (assets / desktop).read_text().replace("Exec=aula-f87pro-gui", f'Exec="{executable}"')
    launcher = prefix / "share" / desktop
    launcher.parent.mkdir(parents=True, exist_ok=True)
    launcher.write_text(text)
    if shutil.which("desktop-file-validate"):
        subprocess.run(["desktop-file-validate", str(launcher)], check=True)
    if shutil.which("update-desktop-database"):
        subprocess.run(["update-desktop-database", str(launcher.parent)], check=True)
    print(f"Installed: {target / 'aula-f87pro-gui'}")
    return target


def main():
    root = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".local")
    parser.add_argument("--bin-dir", type=Path, default=root / "bin")
    parser.add_argument("--udev", action="store_true", help="install HID access rules through graphical Polkit authentication")
    args = parser.parse_args()
    install(args.bin_dir.resolve(), root / "share", args.prefix.resolve())
    if args.udev:
        rule = root / "share/udev/rules.d/70-aula-f87pro.rules"
        subprocess.run(["pkexec", "install", "-Dm644", str(rule), "/etc/udev/rules.d/70-aula-f87pro.rules"], check=True)
        subprocess.run(["pkexec", "udevadm", "control", "--reload-rules"], check=True)
        subprocess.run(["pkexec", "udevadm", "trigger", "--subsystem-match=hidraw"], check=True)
    print("Open AULA F87 Pro from the application menu. Reconnect the keyboard after installing access rules.")


if __name__ == "__main__":
    main()
