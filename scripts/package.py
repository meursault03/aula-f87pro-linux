#!/usr/bin/env python3
"""Package already-built Linux x86_64 binaries; never downloads or installs software."""
import argparse
import hashlib
import re
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def copy(source, target, mode=None):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)
    if mode is not None:
        target.chmod(mode)


def stage(destination, binaries, system=False):
    prefix = destination / "usr" if system else destination
    for name in ("aula-f87pro", "aula-f87pro-gui"):
        copy(binaries / name, prefix / "bin" / name, 0o755)
    for path in (ROOT / "packaging/share").rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT / "packaging/share")
        if system and relative.parts[0] == "udev":
            target = prefix / "lib" / relative
        else:
            target = prefix / "share" / relative
        copy(path, target, 0o644)
    for name in ("LICENSE", "README.md", "README.pt.md", "CHANGELOG.md"):
        copy(ROOT / name, prefix / "share/doc/aula-f87pro" / name, 0o644)
    copy(ROOT / "packaging/THIRD_PARTY_NOTICES.txt", prefix / "share/doc/aula-f87pro/THIRD_PARTY_NOTICES.txt", 0o644)
    if not system:
        copy(ROOT / "packaging/install.py", destination / "install.py", 0o755)


def package(kind, binaries, output, version):
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="aula-package-") as temporary:
        root = Path(temporary)
        if kind == "tar":
            name = f"aula-f87pro-{version}-linux-x86_64"
            folder = root / name
            stage(folder, binaries)
            artifact = output / (name + ".tar.gz")
            with tarfile.open(artifact, "w:gz") as archive:
                archive.add(folder, arcname=name)
        else:
            if not shutil.which("dpkg-deb"):
                raise RuntimeError("Creating .deb packages requires dpkg-deb (build on Ubuntu 24.04).")
            stage(root, binaries, system=True)
            symbols = "".join(subprocess.check_output(["readelf", "--version-info", str(binaries / name)], text=True)
                for name in ("aula-f87pro", "aula-f87pro-gui"))
            versions = [tuple(map(int, v.split("."))) for v in re.findall(r"GLIBC_(\d+\.\d+(?:\.\d+)?)", symbols)]
            glibc = ".".join(map(str, max(versions)))
            control = root / "DEBIAN"
            control.mkdir()
            control.joinpath("control").write_text(f"""Package: aula-f87pro
Version: {version}
Architecture: amd64
Maintainer: meursault03 <meursault03@users.noreply.github.com>
Section: utils
Priority: optional
Depends: libgtk-4-1 (>= 4.10), libc6 (>= {glibc}), libgcc-s1, udev
Homepage: https://github.com/meursault03/aula-f87pro-linux
Description: Native Linux configuration for the AULA F87 Pro
 Lighting, key assignments, macros over USB and experimental 2.4 GHz support.
""")
            control.joinpath("postinst").write_text("#!/bin/sh\nset -e\nif command -v udevadm >/dev/null && [ -d /run/udev ]; then\n  udevadm control --reload-rules\n  udevadm trigger --subsystem-match=hidraw\nfi\n")
            control.joinpath("postinst").chmod(0o755)
            artifact = output / f"aula-f87pro_{version}_amd64.deb"
            subprocess.run(["dpkg-deb", "--build", "--root-owner-group", str(root), str(artifact)], check=True)
    print(artifact)
    return artifact


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=("tar", "deb"), default="tar")
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/release")
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--arch-recipe", type=Path, help="render a PKGBUILD beside this release tarball")
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if args.arch_recipe:
        checksum = hashlib.sha256(args.arch_recipe.read_bytes()).hexdigest()
        recipe = (ROOT / "packaging/arch/PKGBUILD.in").read_text().replace("@VERSION@", version).replace("@SHA256@", checksum)
        args.arch_recipe.with_name("PKGBUILD").write_text(recipe)
    else:
        package(args.format, args.bin_dir.resolve(), args.output.resolve(), version)


if __name__ == "__main__":
    main()
