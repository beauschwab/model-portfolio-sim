"""Build the optional Rust ledger pilot; no external native dependencies."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
subprocess.run(['cargo', 'build', '--release', '--locked', '--manifest-path',
                str(root/'packages/portfolio-ledger-native/Cargo.toml')], check=True)
