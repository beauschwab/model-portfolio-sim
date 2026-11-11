"""Agent harness: give a foundry agent a golden bank, then score its gate drafts.

    harness.py emit BANK POD_DIR        # pod with the bank's extracts and a task brief
    harness.py run BANK --agent "CMD"   # start a private API, run CMD against the pod, score it
    harness.py score BANK POD_DIR       # score evidence/gates/*.json against the bank's answer key
    harness.py reference BANK POD_DIR   # write the answer key as gate drafts (a perfect score)

The answer key is `expected_gates` in the bank's `bank.json`. Scoring reads only
the gate records defined by `foundry/schemas/gate-record.schema.json`; it never
calls a model. Gates G1 to G3 are scored. G4 is a narrative pack and is not.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
SCHEMA = json.loads((REPO / 'foundry' / 'schemas' / 'gate-record.schema.json').read_text(encoding='utf-8'))
GATES = {'G1-data-accepted': 'data-profiling', 'G2-mapping-approved': 'adapter-mapping', 'G3-reconciled': 'gl-reconciliation'}
DISPOSITIONS = set(SCHEMA['properties']['exceptions']['items']['properties']['proposed_disposition']['enum'])
PASS = 0.8


def spec(bank):
    return json.loads((HERE / 'banks' / bank / 'bank.json').read_text(encoding='utf-8'))


def structure(record, gate):
    """Schema problems that make a record unusable. Empty means well formed."""
    if not isinstance(record, dict): return ['record is not an object']
    problems = [f'missing {key}' for key in SCHEMA['required'] if key not in record]
    problems += [f'unknown field {key}' for key in record if key not in SCHEMA['properties']]
    if problems: return problems
    if record['gate'] != gate: problems.append(f'gate is {record["gate"]!r}, expected {gate!r}')
    for key in ('pod_id', 'core_release', 'skill', 'skill_version', 'drafted_at'):
        if not isinstance(record[key], str) or not record[key]: problems.append(f'{key} must be a nonempty string')
    if not isinstance(record['notes'], str): problems.append('notes must be a string')
    evidence = record['evidence']
    if not isinstance(evidence, list) or not evidence or any(
            not isinstance(e, dict) or set(e) != {'ref', 'supports'} or not all(isinstance(v, str) and v for v in e.values()) for e in evidence):
        problems.append('evidence needs at least one {ref, supports} entry')
    exceptions = record['exceptions']
    if not isinstance(exceptions, list) or any(
            not isinstance(e, dict) or not {'id', 'description', 'proposed_disposition'} <= set(e)
            or e['proposed_disposition'] not in DISPOSITIONS for e in exceptions):
        problems.append('exceptions need id, description and a known proposed_disposition')
    return problems


def matches(expected, exception, tolerance):
    words = f'{exception.get("id", "")} {exception.get("description", "")}'.lower()
    if 'amount_usd' in expected:
        amount = exception.get('amount_usd')
        return (isinstance(amount, (int, float)) and abs(abs(amount) - expected['amount_usd']) <= tolerance
                and expected['gl_account'] in words)
    return expected['field'].lower() in words and str(expected['count']) in words


def score_gate(bank_spec, gate, record):
    """Score one draft in [0, 1] with the reasons. A self-approved or malformed draft scores zero."""
    problems = structure(record, gate)
    if problems: return 0., problems
    if any(record[key] is not None for key in ('decision', 'approver', 'decided_at')):
        return 0., ['agents draft records only: decision, approver and decided_at must be null']
    expected = bank_spec['expected_gates'][gate]['exceptions']
    tolerance = max(bank_spec['reconciliation']['tolerance_abs_usd'], .01)
    drafted, findings, used = record['exceptions'], [], set()
    for item in expected:
        hit = next((i for i, e in enumerate(drafted) if i not in used and matches(item, e, tolerance)), None)
        if hit is None: findings.append(f'missed {item["id"]}')
        else: used.add(hit)
    spurious = len(drafted) - len(used)
    if spurious: findings.append(f'{spurious} exception(s) not in the answer key')
    recall = len(used) / len(expected) if expected else 1.
    precision = len(used) / len(drafted) if drafted else 1.
    open_items = sum(1 for i in used if drafted[i]['proposed_disposition'] == 'open')
    if open_items: findings.append(f'{open_items} known exception(s) left open without a proposed disposition')
    disposed = 1. - open_items / len(used) if used else 1.
    return round(.5 * recall + .35 * precision + .15 * disposed, 4), findings


def score(bank, pod):
    bank_spec, results = spec(bank), {}
    for gate in GATES:
        path = Path(pod) / 'evidence' / 'gates' / f'{gate}.json'
        if not path.exists():
            results[gate] = dict(score=0., findings=['no draft']); continue
        try: record = json.loads(path.read_text(encoding='utf-8'))
        except ValueError as exc:
            results[gate] = dict(score=0., findings=[f'invalid JSON: {exc}']); continue
        value, findings = score_gate(bank_spec, gate, record)
        results[gate] = dict(score=value, findings=findings)
    overall = round(sum(r['score'] for r in results.values()) / len(results), 4)
    return dict(bank=bank, overall=overall, passed=all(r['score'] >= PASS for r in results.values()), gates=results)


def reference(bank, pod):
    """The answer key as agent drafts: what a correct run of the three skills records."""
    bank_spec = spec(bank)
    folder = Path(pod) / 'evidence' / 'gates'
    folder.mkdir(parents=True, exist_ok=True)
    for gate, skill in GATES.items():
        exceptions = []
        for item in bank_spec['expected_gates'][gate]['exceptions']:
            if 'amount_usd' in item:
                exceptions.append(dict(id=item['id'], description=f'GL account {item["gl_account"]} has no position extract; ledger exceeds books',
                                       amount_usd=item['amount_usd'], proposed_disposition='out_of_scope', approved_disposition=None))
            else:
                disposition = 'explicit_default' if gate == 'G2-mapping-approved' else 'fix_at_source'
                exceptions.append(dict(id=item['id'], description=f'{item["count"]} rows have no {item["field"]}',
                                       amount_usd=None, proposed_disposition=disposition, approved_disposition=None))
        record = dict(gate=gate, pod_id=bank, drafted_at=dt.datetime(2026, 6, 10, tzinfo=dt.timezone.utc).isoformat(), core_release='golden',
                      skill=skill, skill_version='0.1.0', workspace_revision=None,
                      evidence=[dict(ref=f'evidence/{skill}.md', supports='Reference answer for the golden bank')],
                      exceptions=exceptions, decision=None, approver=None, decided_at=None, notes='Reference draft generated from bank.json.')
        (folder / f'{gate}.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')


def emit(bank, pod, api_url='http://127.0.0.1:8000'):
    bank_spec, pod = spec(bank), Path(pod)
    shutil.copytree(REPO / 'foundry' / 'pod-template', pod, dirs_exist_ok=True)
    shutil.copytree(HERE / 'banks' / bank / 'extracts', pod / 'data' / 'extracts', dirs_exist_ok=True)
    shutil.copy(HERE / 'banks' / bank / 'trial_balance.csv', pod / 'data' / 'trial_balance.csv')
    (pod / 'pod.yaml').write_text(f'''# Golden-bank evaluation pod. Synthetic data; generated by evals/golden_banks/harness.py.
bank:
  name: "{bank_spec['name']}"
  tier: {'large' if bank_spec['generator']['total_assets'] >= 5e10 else 'mid'}
  pod_id: {bank}
people:
  fde: Evaluation harness
pins:
  core_release: golden
  foundry_skills: "0.1.0"
mcp:
  profile: ops
  grants: [read, compute, edit, publish]
  api_url: {api_url}
intended_use:
  - Interest-rate risk (EVE/NII) monitoring for ALCO
products_in_scope: [mortgage, deposit]
data_sources:
  - {{name: retail_tape, product: mortgage+deposit, uri: data/extracts/retail_tape.csv, as_of: "2026-06-10"}}
  - {{name: trial_balance, product: gl, uri: data/trial_balance.csv, as_of: "2026-06-10"}}
reconciliation:
  tolerance_abs_usd: {bank_spec['reconciliation']['tolerance_abs_usd']}
  tolerance_rel: 0.0
''', encoding='utf-8')
    (pod / 'TASK.md').write_text(f'''# Golden-bank task: {bank_spec['name']}

{bank_spec['description']}

Run the foundry skills `data-profiling`, `adapter-mapping` and `gl-reconciliation`
(in `foundry/skills/`) against this pod and the Workbench API at `{api_url}`.
Draft one record per gate in `evidence/gates/` named `G1-data-accepted.json`,
`G2-mapping-approved.json` and `G3-reconciled.json`, following
`foundry/schemas/gate-record.schema.json`. Leave `decision`, `approver` and
`decided_at` null: only a person approves a gate.

Scoring conventions: name the affected field and row count in each data
exception; give the GL account number and `amount_usd` for each reconciliation
break. The non-tape books are loaded from `data/extracts/*.csv`
(see `evals/golden_banks/README.md` for the GL account map).
''', encoding='utf-8')
    return pod


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest='command', required=True)
    for name in ('emit', 'score', 'reference'):
        p = sub.add_parser(name); p.add_argument('bank'); p.add_argument('pod', type=Path)
    run = sub.add_parser('run'); run.add_argument('bank'); run.add_argument('--agent', required=True, help='shell command; sees GOLDEN_POD_DIR and WORKBENCH_MCP_API_URL')
    run.add_argument('--pod', type=Path)
    args = parser.parse_args(argv)
    if args.command == 'emit': print(emit(args.bank, args.pod)); return 0
    if args.command == 'reference': reference(args.bank, args.pod); return 0
    if args.command == 'run':
        import tempfile
        sys.path.insert(0, str(HERE))
        from suite import Stack
        pod = args.pod or Path(tempfile.mkdtemp(prefix=f'golden-pod-{args.bank}-'))
        with Stack() as stack:
            emit(args.bank, pod, stack.url)
            env = os.environ | dict(GOLDEN_POD_DIR=str(pod), WORKBENCH_MCP_API_URL=stack.url, WORKBENCH_API_URL=stack.url)
            code = subprocess.run(args.agent, shell=True, cwd=pod, env=env).returncode
            if code: print(f'agent command exited {code}', file=sys.stderr)
        args.pod = pod
    result = score(args.bank, args.pod)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
