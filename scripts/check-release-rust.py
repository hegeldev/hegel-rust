#!/usr/bin/env python3
import re
import subprocess
import sys


version = subprocess.check_output(["rustc", "--version"], text=True).strip()
match = re.match(r"rustc (\d+)\.(\d+)\.(\d+)(?:\s|$)", version)
if match is None or tuple(map(int, match.groups())) < (1, 89, 0):
    sys.exit(f"Release and Wasm builds require Rust >= 1.89.0; found {version}")
print(version)
