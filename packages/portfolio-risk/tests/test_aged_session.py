"""Native transactional dated preparation with independent cash/yield expectations."""
from copy import deepcopy
import json
from pathlib import Path

import pytest

from portfolio_risk.strategy.aged import AgedSession
from portfolio_risk.strategy import aged, optimizer
from portfolio_risk.core.runtime import RunConfig, run_context

EXAMPLE=Path(__file__).resolve().parents[3]/'examples/models/aged-units-1.json'


def inputs():
    raw=json.loads(EXAMPLE.read_text())
    other=deepcopy(raw['templates'][0]);other['template_id']='second-contract'
    raw['templates'].append(other)
    buy=deepcopy(raw['purchases'][0]);buy['template_id']='second-contract'
    raw['purchases'].append(buy)
    bases=[dict(scenario_id='base',base=dict(nii=5.,dv01=0.,eve=1000.,l1=1e6,l2=0.,nco=1.,
        asf=1e6,rsf=1.,cet1=1000.,rwa=1.,ni=.2365,mv_assets=1000.))]
    return raw,bases,dict(cash_budget=100.,max_total_assets=100.)


def test_constraint_only_stage_reuses_native_preparation_and_is_transactional():
    raw,bases,constraints=inputs()
    with AgedSession(raw,bases,constraints) as session:
        initial=session.status()['preparation']
        staged=session.stage(constraints=dict(constraints,cash_budget=50.))
        assert staged['work']['templates_rebuilt']==0
        assert staged['work']['units_rebuilt']==0
        assert staged['work']['pricing_work']==0
        assert staged['work']['unchanged_request']
        assert session.version==0 and session.status()['pending']
        assert session.status()['preparation']==initial
        with pytest.raises(ValueError,match='already pending'):session.stage()
        with pytest.raises(ValueError,match='token'):session.publish(staged['token']+1)
        session.abort(staged['token'])
        assert not session.status()['pending'] and session.version==0
        assert session.status()['preparation']==initial
        staged=session.stage(constraints=dict(constraints,cash_budget=50.))
        assert session.publish(staged['token'])['version']==1
        assert session.evaluate()['work']['units_rebuilt']==0


def test_dirty_template_rebuild_is_selective_with_original_yield_and_abort():
    raw,bases,constraints=inputs()
    edited=deepcopy(raw)
    edited['templates'][0]['term']['coupon']=.1
    earned=.1*366./365.
    edited['purchases'][0]['original_annual_effective_yield']=(1.+earned)**(365./366.)-1.
    with AgedSession(raw,bases,constraints) as session:
        initial=session.status()['preparation']
        staged=session.stage(aged=edited)
        assert staged['work']['templates_rebuilt']==1
        assert staged['work']['units_rebuilt']==1
        assert not staged['work']['full_rebuild']
        assert staged['worst_case_nii_$']==pytest.approx(5.+100.*earned,abs=1e-8)
        unchanged=staged['aged_preparation']['units'][1]
        assert unchanged==initial['units'][1]
        assert staged['cash_event_replay'][-1]['interest_cash']==pytest.approx(100.*earned,abs=1e-8)
        session.abort(staged['token'])
        assert session.status()['preparation']==initial
        staged=session.stage(aged=edited)
        session.publish(staged['token'])
        assert session.status()['preparation']==staged['aged_preparation']


def test_market_changes_rebuild_all_and_bad_edits_leave_live_state_intact():
    raw,bases,constraints=inputs()
    with AgedSession(raw,bases,constraints) as session:
        initial=session.status()['preparation']
        edited=deepcopy(raw);edited['scenarios'][0]['collateral_curve']['discount_factors'][1]=.94
        staged=session.stage(aged=edited)
        assert staged['work']['templates_rebuilt']==2 and staged['work']['full_rebuild']
        session.abort(staged['token'])
        invalid=deepcopy(raw);invalid['purchases'][0]['quote_price_per_unit']=-1.
        with pytest.raises(ValueError):session.stage(aged=invalid)
        assert session.status()['preparation']==initial and not session.status()['pending']
        with pytest.raises(ValueError,match='stale'):
            session.native.call(op='update_aged',handle=session.handle,version=1)


def test_public_optimizer_dispatches_dated_source_and_never_builds_monthly_proxy():
    raw,bases,constraints=inputs()
    earned=.1*366./365.
    raw['templates'][0]['term']['coupon']=.1
    raw['purchases'][0]['original_annual_effective_yield']=(1.+earned)**(365./366.)-1.
    with run_context(RunConfig(compute_backend='rust')):
        lib=aged.build_library(raw)
        answer=optimizer.optimize_balance_sheet([(lib,bases[0]['base'])],**constraints,
            max_purchase_cost_assets=50.)
        assert answer['execution']['schema']=='strategy-aged-1'
        assert answer['total_purchase_cost_assets_$']==pytest.approx(50.,abs=1e-8)
        assert answer['worst_case_nii_$']==pytest.approx(5.+50.*earned,abs=1e-8)
        assert answer['replay'][0]['funding_ordinals']==answer['aged_preparation']['funding']['ordinals']
        with pytest.raises(ValueError,match='cannot be mixed'):
            optimizer.optimize_balance_sheet([(lib,bases[0]['base']),({},bases[0]['base'])])
