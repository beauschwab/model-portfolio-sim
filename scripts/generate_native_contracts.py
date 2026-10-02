"""Generate ABI names/layout metadata from native-abi-8.json, or check drift."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]


def artifacts():
    spec=json.loads((ROOT/'docs/contracts/native-abi-8.json').read_text())
    operations=spec['operations']
    if [o['id'] for o in operations] != list(range(1,32)): raise ValueError('ABI 8 must retain IDs 1..31')
    py='"""Generated ABI 8 contract; run scripts/generate_native_contracts.py."""\nfrom enum import IntEnum\n\nABI_VERSION = 8\n\n\nclass Op(IntEnum):\n'
    py+=''.join(f'    {o["name"]} = {o["id"]}\n' for o in operations)
    py+='\n\nLAYOUTS = '+repr({o['id']:o for o in operations})+'\n'
    rs='//! Generated ABI 8 operation names. Do not edit.\n'
    rs+='pub const ABI_VERSION: u32 = 8;\n'
    rs+=''.join(f'pub const {o["name"]}: u32 = {o["id"]};\n' for o in operations)
    rs+='\npub fn input_count(op: u32) -> Option<usize> {\n    match op {\n'
    rs+=''.join(f'        {o["name"]} => Some({len(o["inputs"])}),\n' for o in operations if 'variants' not in o)
    rs+='        BEHAVIORAL => Some(0), // mode-dependent layout, validated by the native dispatcher\n        _ => None,\n    }\n}\n'
    md='# Native product ABI 8 layouts\n\nGenerated from `native-abi-8.json`. '+spec['buffer_contract']+'\n\n'
    md+='Symbols: P paths, T captured months, S instruments, K CSR events, H reporting months, F forwards, C factors, Q quotes, J shocks. Empty optional buffers retain a live pointer. Integer/bool values are transported losslessly as f64 within the validated domain.\n\n'
    for o in operations:
        md+=f'## {o["id"]}: {o["name"]}\n\n'
        variants=o.get('variants',[{'inputs':o['inputs'],'name':'standard'}])
        for v in variants:
            if 'variants' in o:md+=f'Variant {v["name"]}:\n\n'
            md+='| Input index | Name | Shape | Unit |\n|---|---|---|---|\n'
            for i,f in enumerate(v['inputs']):md+=f'| {i} | {f["name"]} | {f["shape"]} | {f["unit"]} |\n'
            md+='\n'
        md+='| Output index | Name | Shape | Unit |\n|---|---|---|---|\n'
        for i,f in enumerate(o['outputs']):md+=f'| {i} | {f["name"]} | {f["shape"]} | {f["unit"]} |\n'
        md+='\n'
    return {ROOT/'packages/portfolio-risk/src/portfolio_risk/core/abi_generated.py':py,
            ROOT/'packages/portfolio-risk-native/src/abi_operations.rs':rs,
            ROOT/'docs/contracts/native-abi-layouts.md':md}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--check',action='store_true');args=parser.parse_args()
    bad=[]
    for path,text in artifacts().items():
        if args.check:
            if not path.exists() or path.read_text(encoding='utf-8')!=text:bad.append(path.relative_to(ROOT))
        else:path.write_text(text,encoding='utf-8')
    if bad:raise SystemExit('Native ABI contract drift: '+', '.join(map(str,bad)))
    print('Native ABI contracts '+('verified' if args.check else 'generated'))


if __name__=='__main__':main()
