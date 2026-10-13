"""Static Python/Rust dependency rules. This script imports no financial modules."""
from __future__ import annotations

import argparse
import ast
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]
STORAGE={'app.persistence','app.repository','app.artifacts','app.storage_config','app.result_transport'}
HANDLERS={'app.store','app.operations','app.worker','app.main','app.decision_store','app.cohort_store','app.treasury_store',
          'app.pricing_jobs','app.income_jobs','app.strategy_jobs','app.ledger_jobs','app.job_queue','app.demo_inputs','app.telemetry'}
LEGACY={'portfolio_risk.'+x for x in ('config','conventions','curve','interfaces','kernels','lmm','pricing','scenarios','vol','prepay','cds','corp','deposits','hedges','mm','accounting','kpis','risk','stress','optimizer','strategies','unitlib')}


def imports(module, text, package=False):
    """Include deferred/TYPE_CHECKING imports; they are dependencies, not failures."""
    for n in ast.walk(ast.parse(text)):
        if isinstance(n,ast.Import):
            for a in n.names: yield n.lineno,a.name
        elif isinstance(n,ast.ImportFrom):
            if n.level:
                base=module.split('.') if package else module.split('.')[:-1]
                base=base[:len(base)-n.level+1]+(n.module.split('.') if n.module else [])
                target='.'.join(base)
            else:target=n.module or ''
            yield n.lineno,target
            for a in n.names:yield n.lineno,target+'.'+a.name
        elif isinstance(n,ast.Call) and n.args and isinstance(n.args[0],ast.Constant) and isinstance(n.args[0].value,str):
            if getattr(n.func,'id','')=='__import__' or getattr(n.func,'attr','')=='import_module':
                yield n.lineno,n.args[0].value


def violations(module,text,path=None,package=False):
    errors=[]
    for line,target in imports(module,text,package):
        rule=None
        if module.startswith('portfolio_risk') and (target=='app' or target.startswith(('app.','apps.'))):
            rule='engine cannot depend on API/UI implementations'
        elif module.startswith('workbench_mcp') and any(target==x or target.startswith(x+'.')
                for x in ('app','apps','portfolio_risk','sqlalchemy','sqlite3','ctypes')):
            rule='MCP uses HTTP only; no application internals, financial runtime or database ownership'
        elif module in STORAGE and any(target==x or target.startswith(x+'.') for x in HANDLERS):
            rule='storage cannot depend on feature handlers or application dispatch'
        elif module.startswith('app.') and target in LEGACY:
            rule='internal code uses canonical layered imports; flat aliases belong to compat.py'
        if rule:errors.append(f'{path or module}:{line}: {target}: {rule}')
    return errors


def graph(root):
    result={};errors=[]
    for folder,prefix in [('apps/api/app','app'),('packages/portfolio-risk/src/portfolio_risk','portfolio_risk'),
                          ('apps/mcp/src/workbench_mcp','workbench_mcp')]:
        for p in (root/folder).rglob('*.py'):
            parts=list(p.relative_to(root/folder).with_suffix('').parts)
            if parts[-1]=='__init__':parts.pop()
            module='.'.join([prefix]+parts);text=p.read_text(encoding='utf-8-sig')
            result[module]={target for _,target in imports(module,text,p.name=='__init__.py')}
            errors.extend(violations(module,text,p.relative_to(root),p.name=='__init__.py'))
    return {k:{x for x in v if x in result and x!=k} for k,v in result.items()},errors


def cycle_edges(g):
    """Every edge participating in a cycle, including cycles longer than two."""
    def reaches(start,target):
        todo=[start];seen=set()
        while todo:
            node=todo.pop()
            if node==target:return True
            if node in seen:continue
            seen.add(node);todo.extend(g.get(node,()))
        return False
    return sorted([a,b] for a,bs in g.items() for b in bs if reaches(b,a))


def native_domain_violations(root):
    """Ratchet the migrated domains without pretending legacy roots are migrated."""
    manifest = root / 'docs/contracts/native-domains.json'
    if not manifest.exists():
        return []
    layout = json.loads(manifest.read_text())
    source = root / layout['crate'] / 'src'
    errors = []
    for path in source.glob('*.rs'):
        if path.name not in layout['root_modules']:
            errors.append(f'{path.relative_to(root)}: new flat native module; choose its domain')
    for old, new in layout['relocations'].items():
        if (source / old).exists() or not (source / new).is_file():
            errors.append(f'{layout["crate"]}: migrated module must live only at {new}, not {old}')
    exports = (source / 'lib.rs').read_text()
    for old, new in layout['compatibility_exports'].items():
        declaration = f'pub use {new};' if old == new.split('::')[-1] else f'pub use {new} as {old};'
        if declaration not in exports:
            errors.append(f'{layout["crate"]}/src/lib.rs: missing compatibility export {old}')
    for path in (source / 'math').rglob('*.rs'):
        # Math uses explicit crate paths; grouped root imports would hide ownership.
        text = re.sub(r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"', '', path.read_text(), flags=re.S)
        if re.search(r'crate\s*::\s*\{', text):
            errors.append(f'{path.relative_to(root)}: math must use explicit domain imports')
        for target in re.findall(r'crate\s*::\s*([A-Za-z_]\w*)', text):
            if target != 'math':
                errors.append(f'{path.relative_to(root)}: math cannot depend on {target}')
        if re.search(r'std\s*::\s*(fs|net|process)\b', text):
            errors.append(f'{path.relative_to(root)}: math cannot own external IO or processes')
    return errors


def rust_violations(root):
    errors=[]
    for p in (root/'packages').glob('*/Cargo.toml'):
        for i,line in enumerate(p.read_text().splitlines(),1):
            if re.search(r'path\s*=.*(?:apps[/\\]|apps[/\\]web|apps[/\\]api)',line):errors.append(f'{p.relative_to(root)}:{i}: {line.strip()}: native packages cannot depend on application implementations')
    for p in (root/'packages').glob('*/src/**/*.rs'):
        for i,line in enumerate(p.read_text().splitlines(),1):
            if re.search(r'(?:use|extern crate)\s+(?:app|apps|web)::|(?:include!|include_str!|path\s*=).*apps[/\\]',line):errors.append(f'{p.relative_to(root)}:{i}: application source: native packages cannot depend on API/UI implementations')
    return errors + native_domain_violations(root)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--write-baseline',action='store_true');parser.add_argument('--python-only',action='store_true');args=parser.parse_args()
    g,errors=graph(ROOT);errors+=rust_violations(ROOT)
    baseline=ROOT/'docs/contracts/dependency-baseline.json';current=cycle_edges(g)
    if args.write_baseline:
        if errors:raise SystemExit('\n'.join(errors))
        baseline.write_text(json.dumps({'explanation':'Existing deferred reference/compatibility and worker-session edges; no new cycle edge is admitted without explicit review. Storage/handler edges are forbidden independently.','cycle_edges':current},indent=2)+'\n')
    allowed=json.loads(baseline.read_text())['cycle_edges']
    for a,b in current:
        if [a,b] not in allowed:errors.append(f'{a}: {b}: new cyclic dependency outside the reviewed baseline')
    if errors:raise SystemExit('\n'.join(errors))
    if not args.python_only:subprocess.run(['node',str(ROOT/'scripts/check_web_boundaries.mjs'),str(ROOT)],check=True)
    print(f'Python/Rust boundaries verified ({len(current)} documented cyclic edges)')


if __name__=='__main__':main()
