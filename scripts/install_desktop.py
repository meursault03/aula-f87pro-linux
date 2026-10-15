#!/usr/bin/env python3
"""Build an optimized desktop application and install it for the current user."""
from pathlib import Path
import subprocess
import sys

project = Path(__file__).resolve().parent.parent
subprocess.run(["cargo", "build", "--locked", "--release", "--features", "gui", "--bins"], cwd=project, check=True)
subprocess.run([sys.executable, str(project / "packaging/install.py"), "--bin-dir", str(project / "target/release"), *sys.argv[1:]], check=True)
