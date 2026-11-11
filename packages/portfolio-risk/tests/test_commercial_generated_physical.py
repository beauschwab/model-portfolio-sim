"""Generated source catalogs, original draw proofs and path-specific reviews."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics.commercial_generated_physical_reference import validate,source,append_observed
from portfolio_risk.analytics.commercial_stochastic_reference import fit_pricing_law,generated_policy_sources

ROOT=Path(__file__).resolve().parents[3]

def identity(name):return dict(id=name,revision='1')

def draw_source(raw):
    result=deepcopy(raw['source']);at=result['current']['asof_ordinal']+1
    for role in ('original','current'):
        contract=result[role];period=deepcopy(contract['periods'][0])
        contract.update(contract=identity('generated-draw-contract'),asof_ordinal=at,reporting_end_ordinal=at+4,tail_end_ordinal=at+4,opening_principal=20.,principal_plan=dict(kind='supplied',principal_payments=[20.]))
        period.update(period_id='generated-draw-coupon',start_ordinal=at,end_ordinal=at+4,payment_ordinal=at+4,coupon=dict(kind='fixed',annual_rate=0.));contract['periods']=[period]
    result['acquisition'].update(identity=identity('generated-draw-acquisition'),cost=20.,original_annual_effective_rate=0.)
    return dict(review=identity('generated-draw'),draw_ordinal=at,eligible_origins=[True,False,False],source=result)

def prepare_law(law):
    for role in ('original','current'):
        contract=law['source']['source'][role];period=deepcopy(contract['periods'][0]);period.update(end_ordinal=contract['asof_ordinal']+30,payment_ordinal=contract['asof_ordinal']+35)
        contract['periods']=[period];contract['principal_plan']=dict(kind='supplied',principal_payments=[100.])
    law['coupon_loadings']=[];law['hazard_loadings']=[]
    raw=law['source'];raw['credit'].update(opening_undrawn_commitment=20.,max_jumps_per_path=1)
    raw['credit']['intervals']=raw['credit']['intervals'][:1];raw['credit']['intervals'][0]['end_year']=35/365.
    raw['accounting']['recognition']=raw['accounting']['recognition'][:1];raw['accounting_rates']=raw['accounting_rates'][:1]
    raw['performing_draw_tranches']=[draw_source(raw)]

def role_inputs(law):
    recipes=[]
    for variant in range(2):
        draws=deepcopy(law['source']['performing_draw_tranches'])
        for draw in draws:
            for role in ('original','current'):
                contract=draw['source'][role];contract['path']['id']=f'reviewed-draw-anchor-{variant}'
                for period in contract['periods']:
                    for o in period['coupon'].get('observations',[]):o['source']['path']=deepcopy(contract['path'])
        recipes.append(dict(identity=identity(f'recipe-{variant}'),performing_draw_tranches=draws))
    return dict(source_recipes=recipes,paths=[dict(path_key=i+1,review=identity(f'path-review-{i}'),source_recipe_index=i%2)for i in range(law['evaluation']['paths'])])

def graph_request(floating=False):
    raw=json.loads((ROOT/'examples/models/commercial-generated-graph-1.json').read_text());g=raw['positions'][0]['generated_policy']
    for law in [g['pricing_training'],g['original_physical'],*g['current_physical'],*g['current_pricing']]:prepare_law(law)
    if floating:
        original=json.loads((ROOT/'examples/models/commercial-generated-exercise-1.json').read_text())['source']['source']['current']['periods'][1]['coupon']
        for law in [g['pricing_training'],g['original_physical'],*g['current_physical'],*g['current_pricing']]:
            at=law['source']['source']['current']['asof_ordinal'];clock=(at+1-719163)*86400;draw=law['source']['performing_draw_tranches'][0]
            for role in ('original','current'):
                contract=draw['source'][role];coupon=deepcopy(original);coupon['reset_cutoff_unix_seconds']=clock;o=coupon['observations'][0]
                o.update(interest_start_ordinal=at+1,interest_end_ordinal=at+5,observation_start_ordinal=at+1,observation_end_ordinal=at+5,fixing_ordinal=at+1,selection_cutoff_unix_seconds=clock,rate=.02)
                o['source'].update(path=deepcopy(contract['path']),information_unix_seconds=clock,measure=contract['measure']);contract['periods'][0]['coupon']=coupon
            draw['source']['acquisition']['cost']=20.+20.*.02*4/365.
            law['time_ordinals'].insert(1,at+1);law['coupon_loadings']=[dict(draw_tranche_id='generated-draw',period_id='generated-draw-coupon',observation_index=0,driver_index=0,loading=1.)]
            # This source/proof control stays away from an exact exercise tie.
            for date in law['policy']['exercise_dates']:date['call_clean_price_factor']=1.01
    g['physical_path_inputs']=dict(schema='commercial-generated-physical-path-inputs-1',identity=identity('explicit-physical-path-inputs'),original_physical=role_inputs(g['original_physical']),current_physical=[dict(scenario_id=law['source']['source']['current']['scenario_id'],inputs=role_inputs(law))for law in g['current_physical']])
    return raw

def add_reviews(raw):
    g=raw['positions'][0]['generated_policy'];law=g['current_physical'][0];at=law['source']['source']['current']['asof_ordinal'];terms=law['source']['credit']['intervals'][0]
    terms['hazards_per_year'][0][3]=100.;terms['loss_given_default']=[1.]*3
    actions=dict(policy=dict(identity=identity('path-1-credit-review'),accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff',allowance_shortfall='expense_at_reviewed_chargeoff',excess_cash_recovery='recognize_in_earnings'),
        allowance_reviews=[dict(path_key=1,review=dict(review=identity('path-1-allowance'),recognition_ordinal=at+30,phase='before_credit',target_amount=50.))],
        chargeoffs=[dict(path_key=1,review=dict(review=identity('path-1-chargeoff'),claim_id='commercial-default:1',recognition_ordinal=at+30,principal_amount=10.,book_adjustment_amount=0.,accrued_interest_reversal=0.))],
        legal_waivers=[dict(path_key=1,review=identity('path-1-waiver'),recognition_ordinal=at+30,claim_id='commercial-default:1',principal_amount=5.,accrued_interest_amount=0.,future_cash_disposition='retain_supplied_voluntary_recovery_cash-1')])
    g['physical_path_inputs']['current_physical'][0]['inputs']['paths'][0]['reviewed_credit_actions']=actions
    return raw

@pytest.mark.parametrize('target',['coverage','recipe','anchor','foreign_review','inventory','basis'])
def test_independent_physical_catalog_refuses_incomplete_or_foreign_source_before_generation(target):
    raw=add_reviews(graph_request());g=raw['positions'][0]['generated_policy'];selected=g['physical_path_inputs']
    if target=='coverage':selected['original_physical']['paths'].pop()
    elif target=='recipe':selected['current_physical'][0]['inputs']['paths'][0]['source_recipe_index']=99
    elif target=='anchor':selected['current_physical'][0]['inputs']['source_recipes'][1]['performing_draw_tranches'][0]['source']['original']['path']['id']='changed-original'
    elif target=='foreign_review':selected['current_physical'][0]['inputs']['paths'][0]['reviewed_credit_actions']['allowance_reviews'][0]['path_key']=2
    elif target=='basis':g['current_physical'][0]['regression_policy']='native_observable_linear_svd_train_evaluate-1'
    else:g['current_physical'][0]['source']['performing_draw_tranches'][0]['source']['current']['periods'][0]['period_id']='different-inventory'
    with pytest.raises(ArithmeticError):validate(g)

@pytest.mark.parametrize('reviewed,floating',[(False,False),(True,False),(False,True)])
def test_native_generated_draws_have_independent_original_source_book_and_graph_replay(reviewed,floating):
    from portfolio_risk.analytics.model_contracts import analyze_commercial_graph
    from portfolio_risk.analytics.commercial_generated_graph_reference import verify
    raw=graph_request(floating)
    if reviewed:add_reviews(raw)
    actual=analyze_commercial_graph(raw);checked=verify(raw,actual)
    assert checked['physical_accounting_recomputed']
    proof=actual['original_acquisition_proofs'][0]
    assert proof['conditional_tranche_proofs'][0]['activation_probability']==1.
    assert proof['conditional_tranche_proofs'][0]['conditional_cash_quote']==pytest.approx(20.+(20.*.02*4/365. if floating else 0.),abs=1e-12)
    assert actual['raw_source']['positions'][0]['generated_policy']['physical_path_inputs']['schema']=='commercial-generated-physical-path-inputs-1'
    if reviewed:
        first=next(s for s in checked['sources']if s['role']=='physical' and s['path_key']==1)
        close=first['groups'][-1]['next']['reviewed_credit_state'];assert close['allowance']==40. and len(close['legal_waivers'])==1
    changed=deepcopy(actual);s=next(s for s in changed['native_sources']if s['role']=='physical');s['source_request']['performing_draw_tranches'][0]['source']['original']['path']['id']='foreign-original'
    with pytest.raises(ArithmeticError):verify(raw,changed)


def test_observed_feature_catalog_hides_future_draw_activation_and_lineage():
    raw=graph_request();g=raw['positions'][0]['generated_policy'];law=g['current_physical'][0];inputs=g['physical_path_inputs']['current_physical'][0]['inputs'];r=source(law,inputs,0)
    date=dict(ordinal=law['source']['source']['current']['asof_ordinal'],timing='after_scheduled_cash');point=dict(draw_tranches=[],canonical_legal_coupons=[]);features=[];append_observed(r,point,date,100.,features)
    assert features[-4:]==[0.,0.,0.,0.]
    changed=deepcopy(r);changed['performing_draw_tranches'][0]['source']['current']['path']['id']='future-label';other=[];append_observed(changed,point,date,100.,other);assert other==features


def candidate_request(review_day=40,with_draws=False):
    raw=json.loads((ROOT/'examples/models/commercial-generated-candidate-journal-1.json').read_text());g=raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']
    for law in [g['pricing_training'],g['original_physical'],*g['current_physical'],*g['current_pricing']]:
        at=law['source']['source']['current']['asof_ordinal'];r=law['source'];r['credit'].update(opening_undrawn_commitment=20.,max_jumps_per_path=1)
        draw=draw_source(r)
        for role in ('original','current'):
            contract=draw['source'][role];contract.update(reporting_end_ordinal=at+2,tail_end_ordinal=at+2);contract['periods'][0].update(end_ordinal=at+2,payment_ordinal=at+2)
        r['performing_draw_tranches']=[draw]if with_draws else [];r['credit']['intervals'][-1]['end_year']=(2 if with_draws else 1)/365.
        if not with_draws:r['credit']['opening_undrawn_commitment']=0.
        law['policy']['exercise_dates']=[]
    g['physical_path_inputs']=dict(schema='commercial-generated-physical-path-inputs-1',identity=identity('candidate-physical-path-inputs'),original_physical=role_inputs(g['original_physical']),current_physical=[dict(scenario_id=law['source']['source']['current']['scenario_id'],inputs=role_inputs(law))for law in g['current_physical']])
    # A selected path review after all contractual tails proves that admission
    # and journal replay use the selected source, rather than the empty template.
    at=raw['as_of_ordinal'];g['physical_path_inputs']['current_physical'][0]['inputs']['paths'][0]['reviewed_credit_actions']=dict(policy=dict(identity=identity('selected-late-book-review'),accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff',allowance_shortfall='expense_at_reviewed_chargeoff',excess_cash_recovery='recognize_in_earnings'),allowance_reviews=[dict(path_key=1,review=dict(review=identity('selected-late-allowance'),recognition_ordinal=at+review_day,phase='after_credit',target_amount=3.))],chargeoffs=[],legal_waivers=[])
    return raw


def test_selected_path_review_candidate_persists_complete_late_review_and_source_anchors(tmp_path):
    from test_commercial_generated_candidate import run_generated
    from portfolio_risk.analytics.commercial_generated_candidate_reference import verify_stream,verify_preparation
    raw,prepared,frames=run_generated(candidate_request(),tmp_path)
    actual=verify_stream(raw,prepared,frames)
    assert actual['verified'] and actual['persisted_partitions_replayed']
    assert prepared['receipt']['full_end_ordinal']==raw['as_of_ordinal']+40
    assert prepared['receipt']['physical_paths']==256
    seen=0
    for frame in frames.frames('commercial_closing'):
        for row in frame.iter_rows(named=True):
            if row['scenario']=='baseline' and row['path_key']==1:
                book=json.loads(row['unweighted_book_json']);assert book['reviewed_credit_state']['allowance']==3.;seen+=1
    assert seen==1
    changed=deepcopy(prepared['receipt']);changed['full_end_ordinal']-=1
    native_spec={k:([vars(row) for row in v]if k in ('accounts','positions','scenarios')else deepcopy(v))for k,v in prepared['spec'].items()}
    with pytest.raises(ArithmeticError,match='calendar/path coverage'):verify_preparation(raw,native_spec,changed)


def test_selected_path_review_capacity_refuses_before_any_candidate_publication(tmp_path):
    from test_commercial_generated_candidate import run_generated
    with pytest.raises(RuntimeError,match='generated policy role admission exceeds4B/128MiB'):
        run_generated(candidate_request(review_day=40,with_draws=True),tmp_path)
    assert not (tmp_path/'manifest.json').exists()


def test_native_generated_physical_application_rejects_a_foreign_regression_basis():
    from portfolio_risk.analytics.model_contracts import analyze_commercial_graph
    raw=graph_request()
    raw['positions'][0]['generated_policy']['current_physical'][0]['regression_policy']='native_observable_linear_svd_train_evaluate-1'
    with pytest.raises((RuntimeError,ValueError),match='must match Pricing training contract'):
        analyze_commercial_graph(raw)


def minimal_draw_candidate_request():
    raw=candidate_request(review_day=3,with_draws=True)
    g=raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']
    g['physical_path_inputs']['current_physical'][0]['inputs']['paths'][0].pop('reviewed_credit_actions')
    return raw


def test_minimal_generated_draw_candidate_refuses_work_before_journal_publication(tmp_path):
    from test_commercial_generated_candidate import run_generated
    raw=minimal_draw_candidate_request()
    # Complete source support never grants an over-budget journal receipt.
    # This is the minimal two-run128-path future draw; native source component
    # floors still put its genuine repeated owner passes above4B.
    with pytest.raises(RuntimeError,match='generated policy role admission exceeds4B/128MiB: work=4800566464,bytes=124155560'):
        run_generated(raw,tmp_path)
    assert not (tmp_path/'manifest.json').exists()


def test_counted_minimal_generated_draw_candidate_has_complete_native_and_persisted_replay(tmp_path,record_property):
    from test_commercial_generated_candidate import run_generated
    from portfolio_risk.analytics.commercial_generated_candidate_reference import verify_stream
    raw=minimal_draw_candidate_request()
    g=raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']
    g['physical_path_inputs']['source_work_policy']='counted_native_contract_factor_work-1'
    raw,prepared,frames=run_generated(raw,tmp_path)
    checked=verify_stream(raw,prepared,frames)
    assert checked['verified'] and checked['persisted_partitions_replayed']
    assert prepared['receipt']['physical_paths']==256
    assert prepared['receipt']['full_end_ordinal']==raw['as_of_ordinal']+30
    for name,cap in [('admitted_work_upper',4_000_000_000),('retained_bytes_upper',128<<20)]:
        assert 0<prepared['receipt'][name]<=cap
        record_property(name,prepared['receipt'][name])
    seen=0
    for frame in frames.frames('commercial_candidate_native_sources'):
        for row in frame.iter_rows(named=True):
            cell=json.loads(row['native_json'])
            draw=cell['source_request']['performing_draw_tranches'][0]
            assert draw['draw_ordinal']==raw['as_of_ordinal']+1
            assert draw['source']['acquisition']['cost']==20.
            cash=[c for c in cell['cash']if c['kind']=='draw']
            assert len(cash)==1 and cash[0]['principal']==-20.
            assert sum(c['principal'] for c in cell['cash']if c['event_id']=='generated-draw-coupon')==20.
            if cell['role']=='physical':seen+=1
    assert seen==256


def test_counted_large_draw_candidate_retains_aggregate_refusal_before_publication(tmp_path):
    from test_commercial_generated_candidate import run_generated
    raw=minimal_draw_candidate_request();g=raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']
    for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:
        law['training']['paths']=512;law['evaluation']['paths']=512
    g['physical_path_inputs'].update(source_work_policy='counted_native_contract_factor_work-1',original_physical=role_inputs(g['original_physical']),current_physical=[dict(scenario_id=law['source']['source']['current']['scenario_id'],inputs=role_inputs(law))for law in g['current_physical']])
    with pytest.raises(RuntimeError,match='aggregate|admission|preflight'):
        run_generated(raw,tmp_path)
    assert not (tmp_path/'manifest.json').exists()
