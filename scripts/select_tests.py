"""Test tiers: the smoke tier and change-based suite selection.

python scripts/select_tests.py                 # list suites affected by changes against origin/main
python scripts/select_tests.py --run           # run them
python scripts/select_tests.py --smoke         # fast tier: contracts, web build, one test per engine area
python scripts/select_tests.py --paths a b     # select for explicit paths (used by the tests)

Selection only narrows what runs locally and on pull requests. `make test` and the nightly
workflow still run everything; a selected pass is not full-suite evidence.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
SMOKE_BUDGET_SECONDS = 300

ENGINE, API = 'packages/portfolio-risk', 'apps/api'
PYTEST = ['uv', 'run', '--extra', 'dev', 'python', '-m', 'pytest', '-q']


def cargo(manifest, *extra):
    return (['cargo', 'test', '--locked', '--manifest-path', f'{manifest}/Cargo.toml', *extra], '.')


# name -> commands as (argv, working directory). Order is the run order.
SUITES = {
    'architecture': [
        (['python', 'scripts/check_agent_contract.py'], '.'),
        (['python', 'scripts/check_architecture.py'], '.'),
        (['python', '-m', 'unittest', 'discover', '-s', 'scripts/tests'], '.'),
        (['python', 'scripts/generate_native_contracts.py', '--check'], '.'),
        (['uv', 'run', '--project', API, 'python', 'scripts/generate_http_contracts.py', '--check'], '.'),
        (['uv', 'run', '--project', API, 'python', 'scripts/generate_mcp_contracts.py', '--check'], '.'),
    ],
    'rust:model-core': [cargo('packages/portfolio-model-core')],
    'rust:compute-control': [cargo('packages/portfolio-compute-control')],
    'rust:verification': [cargo('packages/portfolio-verification-native')],
    'rust:ledger': [cargo('packages/portfolio-ledger-native')],
    'rust:risk': [cargo('packages/portfolio-risk-native')],
    'rust:decision': [cargo('packages/portfolio-decision-native')],
    'rust:service': [cargo('apps/api-rust', '--features', 'fault-injection')],
    'engine': [([*PYTEST, 'tests/'], ENGINE)],
    'api': [([*PYTEST, 'tests/'], API)],
    'mcp': [
        (['uv', 'run', '--project', 'apps/mcp', '--extra', 'dev', 'python', '-m', 'pytest', 'apps/mcp/tests', '-q'], '.'),
        (['uv', 'run', '--extra', 'dev', '--with-editable', '../mcp', 'python', '-m', 'pytest',
          'tests/test_agent_routes.py', 'tests/test_agent_mcp_integration.py', '-q'], API),
    ],
    'web': [(['bun', 'run', '--cwd', 'apps/web', 'build'], '.')],
}

# Longest matching prefix wins. Native crates name every suite that links or loads them; the
# Python suites load the release artifacts, so rebuild (`make build-native`) before running them.
NATIVE_PYTHON = ('engine', 'api')
RULES = {
    'packages/portfolio-model-core/': ('rust:model-core', 'rust:ledger', 'rust:risk', 'rust:decision', *NATIVE_PYTHON),
    'packages/portfolio-compute-control/': ('rust:compute-control', 'rust:ledger', 'rust:risk', 'rust:decision', *NATIVE_PYTHON),
    'packages/portfolio-verification-native/': ('rust:verification', 'rust:decision', 'rust:service'),
    'packages/portfolio-ledger-native/': ('rust:ledger', 'rust:decision', *NATIVE_PYTHON),
    'packages/portfolio-risk-native/': ('rust:risk', 'rust:decision', *NATIVE_PYTHON),
    'packages/portfolio-decision-native/': ('rust:decision', *NATIVE_PYTHON),
    'packages/portfolio-risk/': ('engine', 'api'),
    'apps/api-rust/': ('rust:service',),
    'apps/api/': ('api', 'mcp'),
    'apps/mcp/': ('mcp',),
    'apps/web/': ('web',),
    'scripts/': (),
    'docs/contracts/': (),
    'docs/': None,  # prose only
}
ARCHITECTURE_ROOTS = ('apps/', 'packages/', 'scripts/', 'docs/contracts/')
ARCHITECTURE_FILES = ('AGENTS.md', 'Makefile', 'package.json')

# One fast representative per engine area. They load the release artifacts, like the suites
# they stand in for, so the tier refuses to run without them rather than skip native behaviour.
SMOKE_TESTS = {
    ENGINE: [
        'tests/test_engine.py',                   # pricing, OAS round trip, zero-shock invariant
        'tests/test_conventions_corp.py',         # day counts, calendars, corporate deck
        'tests/test_market_curve.py',             # curves
        'tests/test_deposits.py',                 # deposit rate and attrition models
        'tests/test_deposit_terminal.py',         # deposit terminal modes and Rust parity
        'tests/test_forecast.py',                 # income forecasts
        'tests/test_whatif.py',                   # incremental what-if
        'tests/test_cohorts.py',                  # cohort build
        'tests/test_treasury.py',                 # capital and FTP
        'tests/test_swap_strategy_units.py',      # swap hedge units
        'tests/test_decision.py',                 # optimizer and Strategy Lab
        'tests/test_ledger_native.py',            # daily ledger
        'tests/test_native.py',                   # native kernel parity
        'tests/test_native_model_contracts.py',   # native model contracts
        'tests/test_production_backend.py',       # Rust backend default
        'tests/test_interface_compatibility.py',  # compatibility aliases
    ],
    API: [
        'tests/test_contracts.py', 'tests/test_structure_contracts.py', 'tests/test_native_routing.py',
        'tests/test_persistence.py', 'tests/test_worker_transport.py', 'tests/test_cohorts.py',
        'tests/test_forecasts.py', 'tests/test_balance_stress.py', 'tests/test_decision.py',
        'tests/test_treasury.py', 'tests/test_market_data.py',
    ],
}
NATIVE_ARTIFACTS = (
    'packages/portfolio-risk-native/target/release',
    'packages/portfolio-ledger-native/target/release',
    'packages/portfolio-decision-native/target/release',
)


def select(paths):
    """Return (suites in run order, whether an unmapped path requires the smoke tier)."""
    chosen, unmapped = set(), False
    for path in paths:
        path = path.replace('\\', '/')
        prefix = max((p for p in RULES if path.startswith(p)), key=len, default=None)
        if path.startswith(ARCHITECTURE_ROOTS) or path in ARCHITECTURE_FILES:
            chosen.add('architecture')
        if prefix is None:
            # Root prose is inert; anything else unmapped (CI, lockfiles, config) gets the smoke tier.
            if path not in ARCHITECTURE_FILES and not ('/' not in path and path.endswith('.md')):
                unmapped = True
        elif RULES[prefix]:
            chosen.update(RULES[prefix])
    return [name for name in SUITES if name in chosen], unmapped


def changed(base):
    """Committed, staged, unstaged and untracked changes relative to the merge base."""
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=ROOT, text=True).splitlines()
    merge_base = git('merge-base', base, 'HEAD')[0]
    return sorted({*git('diff', '--name-only', merge_base), *git('ls-files', '--others', '--exclude-standard')})


def native_built():
    return all(any((ROOT / d).glob(pattern) for pattern in ('*.dll', '*.so', '*.dylib')) for d in NATIVE_ARTIFACTS)


def smoke_commands():
    commands = [*SUITES['architecture'], *SUITES['web'], SUITES['mcp'][0]]
    commands += [([*PYTEST, *tests], cwd) for cwd, tests in SMOKE_TESTS.items()]
    return commands


def smoke():
    if not native_built():
        sys.exit('Native release artifacts are missing; run `make build-native` before the smoke tier.')
    started = time.monotonic()
    run(smoke_commands())
    elapsed = time.monotonic() - started
    print(f'Smoke tier passed in {elapsed:.0f}s (budget {SMOKE_BUDGET_SECONDS}s).')
    if elapsed > SMOKE_BUDGET_SECONDS:
        sys.exit('Smoke tier exceeded its budget; move the slowest test out of the tier.')


def run(commands):
    for argv, cwd in commands:
        print('+', ' '.join(argv), f'(in {cwd})' if cwd != '.' else '', flush=True)
        executable = shutil.which(argv[0])
        if executable is None:
            sys.exit(f'{argv[0]} is not installed')
        code = subprocess.run([executable, *argv[1:]], cwd=ROOT / cwd).returncode
        if code:
            sys.exit(code)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--base', default='origin/main')
    parser.add_argument('--paths', nargs='*')
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--smoke', action='store_true')
    args = parser.parse_args()
    if args.smoke:
        return smoke()
    paths = args.paths if args.paths is not None else changed(args.base)
    suites, unmapped = select(paths)
    if unmapped:
        print('smoke')
    for name in suites:
        print(name)
    if not suites and not unmapped:
        print('No suites selected: the change touches documentation only.', file=sys.stderr)
    if args.run:
        if unmapped:
            smoke()
        for name in suites:
            run(SUITES[name])


if __name__ == '__main__':
    main()
