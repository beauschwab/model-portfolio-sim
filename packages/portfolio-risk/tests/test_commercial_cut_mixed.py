"""Explicit raw cut authority, independent replay and bounded admission."""
from copy import deepcopy
from pathlib import Path
import json
import pytest
from portfolio_risk.analytics.commercial_cut_mixed_reference import raw_prefix,_window,_observed_prefix
from portfolio_risk.core.commercial_cut_mixed_native import command,run
ROOT=Path(__file__).resolve().parents[3]
def request():return json.loads((ROOT/'examples/models/commercial-cut-mixed-journal-1.json').read_text())

def test_raw_future_candidate_preserves_complete_origin_authority():
    assert raw_prefix(request())==1

@pytest.mark.parametrize('target',['allocation','original_quote','past_acquisition','original_eir','old_owner','observed_grid'])
def test_raw_changed_surviving_or_future_authority_refuses(target):
    raw=request();new=raw['revised_candidate'];u=new['commercial']['units'][0]
    if target=='allocation':new['allocation'][0]['face']+=1.
    elif target=='original_quote':u['graph']['positions'][0]['original_signed_cash_quote']+=1.
    elif target=='past_acquisition':new['commercial']['units'][1]['graph']['as_of_ordinal']=raw['cut_ordinal']
    elif target=='original_eir':u['graph']['positions'][0]['histories'][0]['physical'][0]['source']['source']['acquisition']['original_annual_effective_rate']+=.001
    elif target=='old_owner':new['owners'][0]['source']['revision']='changed'
    else:new['commercial']['reporting_ordinals'][1]+=1
    with pytest.raises((ArithmeticError,ValueError)):raw_prefix(raw)

def test_zero_allocation_still_validates_full_unweighted_authority():
    raw=request()
    for r in(raw['completed_candidate'],raw['revised_candidate']):r['allocation'][0]['face']=0.
    assert raw_prefix(raw)==1
    raw['revised_candidate']['commercial']['units'][0]['graph']['positions'][0]['histories'][0]['physical'][0]['source']['credit']['draw_stream']['stream_revision']='foreign'
    with pytest.raises(ArithmeticError):raw_prefix(raw)

@pytest.mark.parametrize('sign',[1.,-1.])
def test_hand_window_keeps_signed_gross_rights_and_prior_pnl_in_original_gl(sign):
    rows=[]
    def post(day,number,event,gl,instrument,value):
        rows.append(dict(scenario='baseline',day=day,transaction_number=number,account='bank',event=event,gl_account=gl,instrument_id=instrument,value=float(value)))
    post(0,1,'opening','cash','',1000.);post(0,1,'opening','opening_equity','',-1000.)
    right='commercial_fee_receivable'if sign>0. else'commercial_fee_payable'
    contra='commercial_acquired_fee_asset_valuation_adjustment'if sign>0. else'commercial_acquired_fee_liability_valuation_adjustment'
    post(1,2,'commercial_nonintegral_fee',right,'held',6.*sign)
    post(1,2,'commercial_nonintegral_fee',contra,'held',-sign)
    post(1,2,'commercial_nonintegral_fee','cash','',-5.*sign)
    post(2,3,'commercial_nonintegral_fee',contra,'held',.5*sign)
    post(2,3,'commercial_nonintegral_fee','pnl:commercial_nonintegral_fee_income','held',-.5*sign)
    post(5,4,'commercial_nonintegral_fee',right,'held',-6.*sign)
    post(5,4,'commercial_nonintegral_fee','cash','',6.*sign)
    post(5,4,'commercial_nonintegral_fee',contra,'held',.5*sign)
    post(5,4,'commercial_nonintegral_fee','pnl:commercial_nonintegral_fee_income','held',-.5*sign)
    full=dict(journal=rows,ledger=[],summary=[dict(minimum_cash=995.)],trial_balance=[],closing_statements=[],consolidated=[],attribution=[],path=[dict(day=2),dict(day=5)],commercial_fee_daily=[dict(day=2,unweighted=6.*sign),dict(day=5,unweighted=0.)])
    got=_window(full,dict(scenarios=[]),2,lambda:False)
    opening=[r for r in got['journal']if r['event']=='native_verified_cut_opening']
    assert all(r['transaction_number']==0 and r['day']==2 for r in opening)
    balance={r['gl_account']:r['value']for r in opening}
    assert balance[right]==6.*sign and balance[contra]==-.5*sign
    assert balance['pnl:commercial_nonintegral_fee_income']==-.5*sign
    assert balance['opening_equity']==-1000.
    assert sum(r['value']for r in opening)==pytest.approx(0.)
    assert [r['transaction_number']for r in got['journal']if r['day']>2]==[4]*4
    assert got['summary']==full['summary'] and got['path']==full['path']

def test_unweighted_observed_prefix_rejects_tamper_even_without_posted_money():
    old={'commercial_daily':[dict(day=2,position_id='held',unweighted_book=dict(principal=100.))]};new=deepcopy(old)
    _observed_prefix(old,new,2,{'held'},lambda:False)
    new['commercial_daily'][0]['unweighted_book']['principal']=99.
    with pytest.raises(ArithmeticError):_observed_prefix(old,new,2,{'held'},lambda:False)

def test_transport_original_scalar_deadline_and_size_are_bounded():
    raw=request();assert command(raw,timeout=.123)['execution_timeout_ms']==123
    with pytest.raises(ValueError):command(raw,timeout=float('nan'))
    raw['oversized']='x'*(2<<20)
    with pytest.raises(ValueError,match='2MiB'):run(raw)

@pytest.fixture(scope='module')
def fixed_result():
    return run(request())[0]


def test_native_original_joined_fixture_passes_complete_independent_replay(fixed_result):
    from portfolio_risk.analytics.commercial_cut_mixed_reference import verify
    receipt = verify(request(), fixed_result)
    assert receipt['verified'] and receipt['raw_resource_allowance_recomputed']
    assert receipt['complete_source_proofs'] == 27
    assert not receipt['empirical_validation']


@pytest.mark.parametrize('field', ['maximum_bytes', 'maximum_work',
    'typed_input_bytes_upper', 'source_phase_bytes_upper',
    'completed_preparation_bytes_upper', 'raw_prefix_work_reservation'])
def test_fixed_raw_resource_admission_cannot_be_forged(fixed_result, field):
    from portfolio_risk.analytics.commercial_fixed_capacity_reference import verify_packet_resources
    packet = deepcopy(fixed_result['native_packet'])
    packet['resource_admission'][field] += 1
    with pytest.raises(ArithmeticError, match='resource admission differs'):
        verify_packet_resources(request(), packet)


@pytest.mark.parametrize('field', ['raw_source', 'cut_tables', 'producer_completion'])
def test_fixed_persisted_source_and_journal_cannot_be_substituted(fixed_result, field):
    from portfolio_risk.analytics.commercial_cut_mixed_reference import verify
    changed = deepcopy(fixed_result)
    changed[field] = {}
    with pytest.raises(ArithmeticError):
        verify(request(), changed)


def test_fixed_zero_allocation_retains_complete_unweighted_source_proofs():
    from portfolio_risk.analytics.commercial_cut_mixed_reference import verify
    raw = request()
    for side in ('completed_candidate', 'revised_candidate'):
        raw[side]['allocation'][0]['face'] = 0.
    result, _ = run(raw)
    assert verify(raw, result)['complete_source_proofs'] == 27


def test_native_overbudget_complete_sources_refuse_without_fallback():
    raw = request()
    for side in ('completed_candidate', 'revised_candidate'):
        for unit in raw[side]['commercial']['units']:
            position = unit['graph']['positions'][0]
            histories = list(position['original_pricing'])
            for scenario in position['histories']:
                for role in ('physical', 'pricing', 'pricing_down', 'pricing_up'):
                    histories.extend(scenario[role])
            for history in histories:
                history['source']['credit']['max_jumps_per_path'] = 4096
    with pytest.raises(ValueError, match='exceeds128MiB'):
        run(raw)
