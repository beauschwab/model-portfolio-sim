"""Generate the reviewed MCP route catalog without startup, storage or pricing.

Every route needs an explicit permission entry. New routes fail --check until reviewed.
"""
import argparse
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / 'apps/mcp/src/workbench_mcp'


def generate():
    sys.path.insert(0, str(ROOT / 'apps/api'))
    from app.main import app
    from app.model_routes import EXAMPLES
    api = app.openapi()
    policy = json.loads((TARGET / 'policy.json').read_text())
    operations = {}
    for path, methods in api['paths'].items():
        for method, operation in methods.items():
            if method not in {'get', 'post', 'put', 'patch', 'delete'}:
                continue
            key = f'{method.upper()} {path}'
            if key not in policy:
                raise ValueError(f'Unreviewed MCP permission: {key}')
            entry = policy[key]
            if entry['permission'] not in {'read', 'compute', 'edit', 'publish', 'cancel'}:
                raise ValueError(f'Unknown permission: {key}')
            groups = {}
            for parameter in operation.get('parameters', []):
                where = parameter['in']
                if where not in {'path', 'query'}:
                    raise ValueError(f'Unsupported parameter location: {key} {where}')
                group = groups.setdefault(where, {'type': 'object', 'properties': {},
                                                  'additionalProperties': False, 'required': []})
                group['properties'][parameter['name']] = parameter['schema']
                if parameter.get('required'):
                    group['required'].append(parameter['name'])
            props, required = dict(groups), [k for k, v in groups.items() if v['required']]
            body = operation.get('requestBody')
            if body:
                props['body'] = body['content']['application/json']['schema']
                if body.get('required'):
                    required.append('body')
            if entry['permission'] in {'edit', 'publish', 'compute'}:
                props['expected_revision'] = {'type': 'integer', 'minimum': 0,
                    'description': 'Revision read from workspace state; never guess or silently refresh.'}
                required.append('expected_revision')
            if entry.get('job'):
                props['idempotency_key'] = {'type': 'string', 'minLength': 1, 'maxLength': 128,
                    'description': 'Unique key for this logical submission; reuse only for an exact retry.'}
                required.append('idempotency_key')
            if entry.get('transport') == 'arrow':
                props['view'] = {'type': 'object', 'additionalProperties': False, 'properties': {
                    'offset': {'type': 'integer', 'minimum': 0, 'default': 0},
                    'limit': {'type': 'integer', 'minimum': 1, 'maximum': 1000, 'default': 100}}}
            name = method + '_' + re.sub('[^a-zA-Z0-9]+', '_', path).strip('_')
            description = operation.get('description', operation.get('summary', name))
            description += f' [{key}; permission={entry["permission"]}].'
            if entry.get('job'):
                description += ' Returns a durable job; poll get_jobs_jid, then inspect its manifest/tables. A timeout does not mean submission failed.'
            if entry.get('transport') == 'artifact':
                description += ' Returns an authenticated download reference; does not inline binary exports.'
            schema = {'type': 'object', 'properties': props, 'required': required,
                      'additionalProperties': False, '$defs': api['components']['schemas']}
            encoded = json.dumps(schema).replace('#/components/schemas/', '#/$defs/')
            # Retain only definitions reachable by this operation.
            schema = json.loads(encoded)
            needed, pending = {}, [schema['properties']]
            while pending:
                value = pending.pop()
                if isinstance(value, list): pending.extend(value)
                elif isinstance(value, dict):
                    if '$ref' in value:
                        ref = value['$ref'].removeprefix('#/$defs/')
                        if ref not in needed:
                            needed[ref] = schema['$defs'][ref]; pending.append(needed[ref])
                    pending.extend(v for k, v in value.items() if k != '$ref')
            schema['$defs'] = needed
            operations[name] = dict(method=method.upper(), path=path, description=description,
                                    inputSchema=schema, **entry)
    if set(policy) != {f'{v["method"]} {v["path"]}' for v in operations.values()}:
        raise ValueError('Stale MCP permission entry')
    catalog = dict(version=1, operations=operations, model_contracts=sorted(EXAMPLES))
    return json.dumps(catalog, indent=2, sort_keys=True) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--check', action='store_true')
    args = parser.parse_args(); content = generate()
    docs = [ROOT / 'docs/rust-production-contract.md', ROOT / 'docs/architecture.md',
            *sorted((ROOT / 'docs/models').glob('*.md'))]
    references = {str(p.relative_to(ROOT)).replace('\\', '/'): p.read_text(encoding='utf-8-sig') for p in docs}
    for name, value in [('catalog.json', content), ('references.json', json.dumps(references, indent=2, sort_keys=True) + '\n')]:
        path = TARGET / name
        if args.check:
            if not path.exists() or path.read_text(encoding='utf-8') != value:
                raise SystemExit(f'MCP {name} drift; run scripts/generate_mcp_contracts.py')
        else:
            path.write_text(value, encoding='utf-8')
    print(f'MCP catalog {"verified" if args.check else "generated"}: {len(json.loads(content)["operations"])} operations')
