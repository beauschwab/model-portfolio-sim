"""Configure and launch the local OpenRouter analyst pilot without printing secrets."""
import argparse
import hashlib
import os
from pathlib import Path
import secrets
import subprocess
import sys

from dotenv import dotenv_values, load_dotenv

ROOT = Path(__file__).resolve().parents[1]
ENV_FILE = ROOT / 'apps/api/.env.analysts'
DEFAULT_EXECUTABLE = ROOT / '.data/analyst-runtime/v1.57.0/goose-package' / ('goose.exe' if os.name == 'nt' else 'goose')
DEFAULT_MODEL = 'anthropic/claude-sonnet-5.5'
sys.path.insert(0, str(ROOT / 'apps/api'))


def configure(executable, env_file=ENV_FILE):
    from app.analyst_config import GOOSE_VERSION
    executable = Path(executable).resolve(strict=True)
    flags = {'creationflags': subprocess.CREATE_NO_WINDOW} if os.name == 'nt' else {}
    result = subprocess.run([str(executable), '--version'], capture_output=True, text=True, timeout=10, **flags)
    if result.returncode or result.stdout.strip() not in {GOOSE_VERSION, 'goose ' + GOOSE_VERSION}:
        raise ValueError('Install the reviewed Goose 1.57.0 binary; see docs/embedded-analysts.md')
    values = {
        'WORKBENCH_ANALYST_ENABLED': '1',
        'WORKBENCH_ANALYST_SECRET': secrets.token_urlsafe(48),
        'WORKBENCH_GOOSE_EXECUTABLE': executable.as_posix(),
        'WORKBENCH_GOOSE_SHA256': hashlib.sha256(executable.read_bytes()).hexdigest(),
        'WORKBENCH_GOOSE_PROVIDER': 'openrouter',
        'WORKBENCH_GOOSE_MODEL': DEFAULT_MODEL,
        'WORKBENCH_ANALYST_API_URL': 'http://127.0.0.1:8000',
        'WORKBENCH_ANALYST_LOCAL_USER': 'local-user',
        'OPENROUTER_API_KEY': '',
    }
    existing = dotenv_values(env_file, interpolate=False) if env_file.exists() else {}
    missing = {key: value for key, value in values.items() if key not in existing}
    if missing:
        # Append only missing settings. Never replace a credential or signing secret.
        fd = os.open(env_file, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
        with os.fdopen(fd, 'a', encoding='utf-8') as stream:
            stream.write('\n# Local analyst settings. Keep this ignored file private.\n')
            for key, value in missing.items():
                escaped = value.replace('\\', '\\\\').replace("'", "\\'")
                stream.write(f"{key}='{escaped}'\n")
    print(f'Local configuration ready: {env_file}')
    print('Set OPENROUTER_API_KEY in that file, then run bun run check:analysts:live and bun run dev:analysts.')


def load_local_config():
    # Process-manager settings take precedence; do not read a general .env implicitly.
    load_dotenv(ENV_FILE, override=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['configure', 'serve'])
    parser.add_argument('--executable', type=Path, default=DEFAULT_EXECUTABLE)
    args = parser.parse_args()
    if args.command == 'configure':
        configure(args.executable)
        return
    load_local_config()
    from app.analyst_config import runtime_config
    config = runtime_config()
    print(f"Analysts: Goose {config['version']} / {config['provider']} / {config['model']}")
    import uvicorn
    # Match dev:api's working directory and persistence defaults.
    os.chdir(ROOT / 'apps/api')
    # Uvicorn's Windows reloader selects a Selector loop, which cannot launch ACP
    # subprocesses. A single Windows server uses the required Proactor loop.
    options = {'reload': True, 'reload_dirs': ['app']} if os.name != 'nt' else {'reload': False}
    uvicorn.run('app.main:app', host='127.0.0.1', port=8000, workers=1, **options)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, FileNotFoundError, subprocess.TimeoutExpired) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
