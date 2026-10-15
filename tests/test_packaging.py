import importlib.util
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


package = module("package", "scripts/package.py")
installer = module("installer", "packaging/install.py")


class PackagingTests(unittest.TestCase):
    def binaries(self, folder):
        folder.mkdir()
        for name in ("aula-f87pro", "aula-f87pro-gui"):
            path = folder / name
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o755)
        return folder

    def test_archive_contains_both_siblings_desktop_assets_and_no_local_captures(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            artifact = package.package("tar", self.binaries(root / "bin"), root / "dist", "0.1.0")
            with tarfile.open(artifact) as archive:
                names = set(archive.getnames())
                prefix = "aula-f87pro-0.1.0-linux-x86_64/"
                for required in ["bin/aula-f87pro", "bin/aula-f87pro-gui", "install.py",
                                 "share/applications/io.github.aula_f87pro.Control.desktop",
                                 "share/udev/rules.d/70-aula-f87pro.rules", "share/doc/aula-f87pro/LICENSE"]:
                    self.assertIn(prefix + required, names)
                self.assertFalse(any("analysis/" in name or name.endswith(".exe") for name in names))
                self.assertTrue(archive.getmember(prefix + "bin/aula-f87pro-gui").mode & 0o111)

    def test_user_install_preserves_sibling_layout_and_handles_spaces(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            prefix = root / "user installation"
            installer.install(self.binaries(root / "source"), ROOT / "packaging/share", prefix)
            for name in ("aula-f87pro", "aula-f87pro-gui"):
                subprocess.run([str(prefix / "bin" / name)], check=True)
            launcher = prefix / "share/applications/io.github.aula_f87pro.Control.desktop"
            self.assertIn(f'Exec="{prefix}/bin/aula-f87pro-gui"', launcher.read_text())
            self.assertTrue((prefix / "share/icons/hicolor/scalable/apps/aula-f87pro.svg").is_file())
            self.assertFalse((prefix / "share/udev").exists())

    def test_missing_cli_does_not_leave_a_partial_install(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with self.assertRaises(FileNotFoundError):
                installer.install(root, ROOT / "packaging/share", root / "prefix")
            self.assertFalse((root / "prefix").exists())


if __name__ == "__main__":
    unittest.main()
