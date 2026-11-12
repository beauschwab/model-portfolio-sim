"""Local setup preserves credentials and launches a subprocess-capable server."""
import os
from pathlib import Path
from types import SimpleNamespace

from dotenv import dotenv_values


def helper(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[3] / 'scripts'))
    import analysts
    return analysts


def test_setup_preserves_existing_private_configuration(tmp_path, monkeypatch, capsys):
    module = helper(monkeypatch)
    executable = tmp_path / 'goose.exe'
    executable.write_bytes(b'verified fixture')
    monkeypatch.setattr(module.subprocess, 'run', lambda *a, **kw: SimpleNamespace(returncode=0, stdout='goose 1.57.0'))
    env_file = tmp_path / '.env.analysts'
    env_file.write_text("OPENROUTER_API_KEY='private-key'\nWORKBENCH_GOOSE_MODEL='approved/model'\n", encoding='utf-8')
    module.configure(executable, env_file)
    first = env_file.read_bytes()
    values = dotenv_values(env_file)
    assert values['OPENROUTER_API_KEY'] == 'private-key'
    assert values['WORKBENCH_GOOSE_MODEL'] == 'approved/model'
    assert len(values['WORKBENCH_ANALYST_SECRET']) >= 32
    module.configure(executable, env_file)
    assert env_file.read_bytes() == first
    output = capsys.readouterr().out
    assert 'private-key' not in output and values['WORKBENCH_ANALYST_SECRET'] not in output


def test_launcher_keeps_windows_subprocess_support(monkeypatch):
    module = helper(monkeypatch)
    from app import analyst_config
    import uvicorn
    monkeypatch.setattr(module.sys, 'argv', ['analysts.py', 'serve'])
    monkeypatch.setattr(module, 'load_local_config', lambda: None)
    monkeypatch.setattr(module.os, 'chdir', lambda _: None)
    monkeypatch.setattr(analyst_config, 'runtime_config', lambda: dict(version='1.57.0', provider='openrouter', model='approved'))
    calls = []
    monkeypatch.setattr(uvicorn, 'run', lambda *args, **kw: calls.append(kw))
    module.main()
    assert calls[0]['host'] == '127.0.0.1'
    assert calls[0]['workers'] == 1
    if os.name == 'nt':
        assert calls[0]['reload'] is False
