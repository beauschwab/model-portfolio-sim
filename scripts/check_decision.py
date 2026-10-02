"""Small real-engine smoke and comparison; run from repository root."""
import json
import numba
from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.strategy.decision import DecisionSession
from portfolio_risk.strategy.optimizer import optimize_balance_sheet

numba.set_num_threads(2)
bs = model_balance_sheet(scale=.001, basis='amortized_cost', include_markets_bs=True)
sr, vp = demo_market()
s = DecisionSession(books={k: bs[k].head(2) for k in ('mbs','loans','debt','deposits','cds')},
    asof=bs['asof'], swap_rates=sr, vol_pts=vp, config=RunConfig(16,16,6),
    mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history(),
    extras={'mm':bs['mm'], 'equity':bs['equity']},
    constraints={'lcr_min':.01,'nsfr_min':.01,'cet1_min':.001,'eve_limit':1.,'max_total_assets':1e7,'cash_budget':1e7})
try:
    r = s.update(version=0)
    print(json.dumps(r, indent=2))
    r = s.update(version=s.version, edits={f"loans:{bs['loans']['id'][0]}":{'coupon_or_spread':.06}})
    print('EDIT',r['work'],r['timings_ms'])
    r = s.update(version=s.version, templates={'cml_fixed_5y':{'spread_bp':250.}})
    print('TEMPLATE',r['work'],r['timings_ms'])
    ref = optimize_balance_sheet(list(zip(s.libraries,s.bases)), lcr_min=.01,nsfr_min=.01,cet1_min=.001,eve_limit=1.,max_total_assets=1e7,cash_budget=1e7)
    print('REFERENCE', ref)
finally:
    s.close()
