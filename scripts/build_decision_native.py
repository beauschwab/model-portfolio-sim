"""Build the optional decision prototype. Requires CMake, C++ tools and libclang.

uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
try:
    import cmake
    env['CMAKE'] = str(Path(cmake.CMAKE_BIN_DIR) / ('cmake.exe' if os.name == 'nt' else 'cmake'))
except ImportError:
    pass
if not env.get('LIBCLANG_PATH'):
    try:
        import clang.cindex
        env['LIBCLANG_PATH'] = clang.cindex.Config.library_path
    except ImportError:
        pass
mode = 'clippy' if '--clippy' in sys.argv else 'test' if '--test' in sys.argv else 'build'
command = ['cargo', mode, '--release', '--locked', '--manifest-path',
           str(root / 'packages/portfolio-decision-native/Cargo.toml')]
if mode == 'clippy':
    command += ['--', '-D', 'warnings']
subprocess.run(command, env=env, check=True)
if mode == 'build':
    # The decision/workflow binaries statically include the product runtime.
    shutil.copyfile(root / 'packages/portfolio-risk-native/THIRD_PARTY_NOTICES.md',
                    root / 'packages/portfolio-decision-native/target/release/portfolio-decision-native-NOTICES.md')
