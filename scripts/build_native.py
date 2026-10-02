"""Build the optional Rust backend reproducibly; Python/NumPy remain sufficient."""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
subprocess.run(['cargo', 'build', '--release', '--locked', '--manifest-path',
                str(root / 'packages/portfolio-risk-native/Cargo.toml')], check=True)
crate = root / 'packages/portfolio-risk-native'
# Keep the adapted SciPy controller's license beside locally built binaries.
shutil.copyfile(crate / 'THIRD_PARTY_NOTICES.md',
                crate / 'target/release/portfolio-risk-native-NOTICES.md')
