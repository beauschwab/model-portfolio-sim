"""Profile the public raw-lifecycle adapter after warmup; inclusive timings."""
import argparse
import cProfile
import io
import pstats
from pathlib import Path
from contextlib import redirect_stdout

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--positions',type=int,default=60000)
    parser.add_argument('--paths',type=int,default=8)
    parser.add_argument('--output',default='docs/reviews/2026-10-01-owned-adapter-profile.txt')
    args=parser.parse_args()
    import numba
    from benchmark_balance_sheet import fixture
    from portfolio_risk import demo
    from portfolio_risk.core.runtime import RunConfig,run_context
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.analytics.kpis import compute_kpis
    numba.set_num_threads(4)
    bs,_=fixture(args.positions);rates,quotes=demo.demo_market();history=demo.demo_deposit_history()
    bs['hedges']=demo.demo_hedge_book(asof=bs['asof'])
    def run():
        accounting=run_balance_sheet_nii(bs,rates,quotes,history,horizon=27,seed=29,asof=bs['asof'])
        return compute_kpis(bs,rates,quotes,history,accounting['monthly'],seed=29)
    with run_context(RunConfig(args.paths,args.paths,27,compute_backend='rust')),redirect_stdout(io.StringIO()):
        run();profile=cProfile.Profile();profile.enable();run();profile.disable()
    output=io.StringIO();pstats.Stats(profile,stream=output).sort_stats('cumtime').print_stats(35)
    Path(args.output).write_text(output.getvalue(),encoding='utf-8')
    print(output.getvalue())

if __name__=='__main__':main()
