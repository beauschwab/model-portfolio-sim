"""60k mixed daily balance-sheet state, with streaming raw-output parity.

Uses the same process-tree memory guard as benchmark_streamed_balance. This is
not product Monte Carlo pricing: explicit daily research stress inputs are used.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import polars as pl
from polars.testing import assert_frame_equal
import benchmark_streamed_balance as harness
from portfolio_risk.analytics.balance_stress import MODEL_VERSION
from portfolio_risk.analytics.balance_stream import source_identity


def mixed_fixture(count,days):
    positions=[]
    for i in range(count):
        r=i%12
        common=dict(id=f'p:{i}',account='bank',balance=1.,rate=.04,payment_interval_days=30)
        if r<4:
            common.update(kind='loan',floating_beta=.7,annual_pd=.01 if i%1200==0 else 0.,
                          recovery_days=5,commitment=.1,draw_fraction=.05,risk_weight=1.,rsf_weight=.85)
        elif r<8:
            common.update(kind='deposit',rate=.015,floating_beta=.2,monthly_runoff=.003,
                          uninsured_fraction=.6,digital_fraction=.8,asf_weight=.9,lcr_outflow_weight=.15)
        elif r<10:
            common.update(kind='security',classification='afs' if r==8 else 'htm',duration=3.,
                          hqla_weight=.85,hqla_level='level2a',risk_weight=.2,rsf_weight=.15,
                          collateral_pool='agency',eligible_fraction=1.,haircut=.1,
                          maturity_day=15 if i%1200 in {8,9} else 0)
        elif r==10:
            common.update(kind='funding',rate=.025,maturity_day=10,rollover=.9,asf_weight=.5,lcr_outflow_weight=1.)
        else:
            common.update(kind='reverse_repo',rate=.025,maturity_day=20,rsf_weight=.1)
        positions.append(common)
    # Explicit synthetic opening accounts, independently balanced before execution.
    cash=count*.1
    asset=sum(p['balance'] for p in positions if p['kind'] in {'loan','security','reverse_repo'})
    liabilities=sum(p['balance'] for p in positions if p['kind'] in {'deposit','funding','repo'})
    dealer_inventory=dict(id='dealer-security',account='dealer',kind='security',balance=100.,rate=.04,
                          classification='trading',duration=2.,collateral_pool='dealer',eligible_fraction=1.)
    dealer_repo=dict(id='dealer-repo',account='dealer',kind='repo',balance=90.,rate=.025,maturity_day=7,rollover=.8)
    # Stay at exactly count positions: replace two bank positions, then recalculate.
    positions[-2:]=[dealer_inventory,dealer_repo]
    asset=sum(p['balance'] for p in positions if p['account']=='bank' and p['kind'] in {'loan','security','reverse_repo'})
    liabilities=sum(p['balance'] for p in positions if p['account']=='bank' and p['kind'] in {'deposit','funding','repo'})
    return dict(version=MODEL_VERSION,horizon_days=days,
        accounts=[dict(id='bank',entity='bank',currency='USD',cash=cash,equity=cash+asset-liabilities,
                       cash_floor=100.,tax_rate=.21,annual_fees=20.,annual_costs=30.,annual_dividends=10.,
                       lcr_floor=1.,nsfr_floor=1.,htm_asset_limit=.3),
                  dict(id='dealer',entity='dealer',currency='USD',cash=20.,equity=40.,cash_floor=5.)],
        positions=positions,
        netting_sets=[dict(id='csa',account='dealer',counterparty='cp',fair_value=10.,stress_loss=40.,margin_delay=2,annual_pd=.01)],
        policies=[dict(id='borrow',account='bank',kind='secured_funding',position='p:8',trigger_cash=cash+1.,limit=.5,funding_tenor_days=5),
                  dict(id='sell',account='bank',kind='sell',position='p:20',trigger_cash=cash+1.,limit=.4),
                  dict(id='support',account='bank',destination='dealer',kind='transfer',trigger_cash=25.,limit=50.),
                  dict(id='cut',account='bank',kind='cut_dividend',trigger_cash=cash+1.,limit=1.)],
        scenarios=[dict(name='joint',rate_shift=.02,spread_shift=.01,pd_multiplier=5.,migration_multiplier=3.,
                        deposit_flight=.02,rollover_loss=.2,market_shock=1.,draw_multiplier=1.)])


def compare(left,right):
    lm,rm=json.loads(left.read_text()),json.loads(right.read_text())
    counts={}
    def frames(path,table):
        for part in json.loads(path.read_text())['tables'][table]['parts']:
            yield pl.read_parquet(path.parent/part['path'])
    for table in lm['tables']:
        a,b=iter(frames(left,table)),iter(frames(right,table))
        af,bf=next(a,None),next(b,None);n=0
        while af is not None and bf is not None:
            size=min(af.height,bf.height)
            assert_frame_equal(af.head(size),bf.head(size),check_exact=False,rel_tol=1e-12,abs_tol=1e-8)
            n+=size
            af=af.slice(size) if size<af.height else next(a,None)
            bf=bf.slice(size) if size<bf.height else next(b,None)
        assert af is None and bf is None,table
        counts[table]=n
    return counts


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--child',action='store_true')
    parser.add_argument('--backend',choices=['python','rust'])
    parser.add_argument('--positions',type=int,default=60000)
    parser.add_argument('--days',type=int,default=30)
    parser.add_argument('--directory')
    parser.add_argument('--memory-mib',type=int,default=2048)
    args=parser.parse_args()
    if args.child:
        harness.fixture=mixed_fixture
        harness.child(args);return
    initial=source_identity();results=[]
    with tempfile.TemporaryDirectory(prefix='mixed-state-bench-') as directory:
        manifests=[]
        for backend in ['python','rust']:
            destination=Path(directory)/backend
            run=subprocess.run([sys.executable,__file__,'--child','--backend',backend,
                '--positions',str(args.positions),'--days',str(args.days),'--directory',str(destination),
                '--memory-mib',str(args.memory_mib)],capture_output=True,text=True)
            if run.returncode:
                raise RuntimeError(run.stderr[-3000:])
            results.append(json.loads(run.stdout));manifests.append(next(destination.rglob('manifest.json')))
            print(f'{backend}: {results[-1]["seconds"]:.2f}s / {results[-1]["peak_tree_rss_mib"]:.0f} MiB',flush=True)
        counts=compare(*manifests)
    assert source_identity()==initial
    out=Path('docs/reviews/2026-09-29-mixed-state-60000.json')
    out.write_text(json.dumps(dict(scope='one sample/backend; synthetic mixed six-kind daily book plus netting, policies, margins and tax; full partitioned journal replay; excludes Monte Carlo/product-pricer regeneration and API/S3',
        parity='all raw table rows compared in order at rtol=1e-12, atol=1e-8; no rounding',
        source_sha256=initial,rows_compared=counts,samples=results),indent=2)+'\n',encoding='utf-8')
    print(out)


if __name__=='__main__':
    main()
