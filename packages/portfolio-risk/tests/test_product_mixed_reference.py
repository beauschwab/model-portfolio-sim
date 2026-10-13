"""Independent mixed-source hand fixtures; native parity is a separate gate."""
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess

import pytest

from portfolio_risk.analytics import product_mixed_reference as oracle

ROOT = Path(__file__).resolve().parents[3]


def hand_case():
    back = json.loads((ROOT/'examples/models/dated-ledger-1.json').read_text())
    back['specification']['positions'][0]['maturity_day'] = 0
    back['specification']['accounts'][0].update(cash=50., equity=150.)
    raw = dict(backbook=back, candidate=dict(policy=dict(identity=dict(id='eir-review', revision='1'))))
    spec = oracle._normalize(back['specification'])
    placeholder = deepcopy(spec['positions'][0]); placeholder.update(id='candidate', balance=0.)
    spec['positions'].append(placeholder)
    owners, runs, events, end = oracle._backbook(back, oracle._normalize(back['specification']), lambda:False)
    for event in events:
        event['owner'] = owners[event['event']['position_id']]
    asof = back['dated']['as_of_ordinal']; acq = []; pools = {}; signs = {}
    for run in runs:
        owner = dict(position_id='candidate', account_id='a', currency='USD', direction='receipt',
            ownership='fully_scenario_generated', source_id='candidate-source', source_revision='1')
        purchase = 'purchase:'+run
        acq.append(dict(event_id=purchase, run_id=run, position_id='candidate', account_id='a', currency='USD',
            source=dict(id='candidate-source', revision='1'), path_id='one', source_weight=1., allocation_factor=1.,
            principal=40., cost=39., ordinal=asof, original_yield_anchor=dict(id='original-yield', revision='1')))
        signs[purchase] = 1.
        for day, pc, ic, earned, book in [(1, 0., 0., 2., 1.), (2, 40., 2., 0., 0.)]:
            event_id = 'candidate:'+run+':'+str(day)
            event = deepcopy(events[0]['event'])
            event.update(position_id='candidate', event_id=event_id, payment_ordinal=asof+day, day_offset=day,
                scenario=dict(kind='named', scenario_id=run), measure='physical',
                accrual_period=dict(start_ordinal=asof+day-1, end_ordinal=asof+day),
                components=dict(principal=pc, cash_interest=ic, accrued_interest=earned, book_amortization=book, fees=0.))
            segment = dict(start_ordinal=asof+day-1, end_ordinal=asof+day, accrued_interest=earned, book_amortization=book)
            events.append(dict(event=event, owner=owner, selector=run, segments=[segment]))
            pools[run, event_id] = purchase
    events.sort(key=lambda e:e['event']['payment_ordinal'])
    return raw, spec, events, acq, end, pools, signs


def test_hand_acquisition_and_backbook_share_one_cash_gl():
    raw, spec, events, acq, end, pools, signs = hand_case()
    result = oracle._replay(raw, spec, events, acq, end, pools, signs, lambda:False)
    close = result['closing_statements'][0]
    assert close['cash'] == pytest.approx(153.3)
    assert close['equity'] == pytest.approx(153.3)
    assert close['liabilities'] == 0.
    assert len([j for j in result['journal'] if j['event']=='opening' and j['gl_account']=='cash']) == 2
    acquisition = [j for j in result['journal'] if j['scenario']=='baseline' and j['event']=='dated_acquisition']
    assert [(j['gl_account'], j['value']) for j in acquisition] == [('asset_principal',40.), ('book_adjustment',-1.), ('cash',-39.)]
    assert all(abs(r['balance']) < 1e-9 for r in result['trial_balance'] if r['instrument_id']=='candidate' and not r['gl_account'].startswith('pnl:'))
    assert result['reported_daily_balance'] == [r for r in result['daily_balance'] if r['day']==30]


def test_hand_candidate_pooled_rights_retire_instead_of_offsetting_daily_events():
    raw, spec, events, acq, end, pools, signs = hand_case()
    result = oracle._replay(raw, spec, events, acq, end, pools, signs, lambda:False)
    rights = [j for j in result['journal'] if j['scenario']=='baseline' and j['instrument_id']=='candidate' and j['gl_account'].startswith('dated_')]
    assert [(r['day'], r['gl_account'], r['value']) for r in rights] == [(1,'dated_interest_receivable',2.), (2,'dated_interest_receivable',-2.)]
    day1 = next(r for r in result['daily_balance'] if r['scenario']=='baseline' and r['day']==1)
    assert day1['assets'] == pytest.approx(153.01)


def test_hand_missing_backbook_principal_tail_fails():
    raw, spec, events, acq, end, pools, signs = hand_case()
    events = [e for e in events if e['event']['position_id']=='candidate']
    with pytest.raises(ArithmeticError, match='complete_principal_tail'):
        oracle._replay(raw, spec, events, acq, end, pools, signs, lambda:False)


@pytest.mark.parametrize('side,coupon,net,expected', [
    (-1.,-1., .05, (.05,0.,0.,0.)), (-1.,-1.,-.05, (0.,0.,0.,-.05)),
    (-1.,1.,-.05, (0.,-.05,0.,0.)), (-1.,1.,.05, (0.,0.,.05,0.)),
    (1.,-1.,-.05, (0.,-.05,0.,0.)), (1.,-1.,.05, (0.,0.,.05,0.))])
def test_signed_coupon_rights_preserve_economic_claim(side,coupon,net,expected):
    assert oracle._path_rights(side,coupon,net) == expected


def test_sign_changing_coupon_rejected_but_eir_is_independent():
    path = dict(recognition=[dict(interest=-2.)], cash=[dict(interest=-2.)], original_eir=.02)
    assert oracle._coupon_sign(path) == -1.
    path['cash'].append(dict(interest=1.))
    with pytest.raises(ArithmeticError, match='sign-changing'):
        oracle._coupon_sign(path)


def test_cancel_during_independent_mixed_replay():
    raw, spec, events, acq, end, pools, signs = hand_case()
    calls = 0
    def cancel():
        nonlocal calls
        calls += 1
        return calls > 4
    with pytest.raises(InterruptedError):
        oracle._replay(raw, spec, events, acq, end, pools, signs, cancel)


@pytest.mark.parametrize('mutation', [
    lambda b:b['dated']['sources'][0]['coverage'].clear(),
    lambda b:b['dated']['runs'].pop(),
    lambda b:b['dated']['sources'][0]['schedule']['events'][0]['adjustment'].update(kind='survival'),
    lambda b:b['dated']['accrual_plans'].clear(),
    lambda b:b['dated']['sources'][0]['schedule']['events'][0]['components'].update(fees=.1),
])
def test_raw_backbook_incomplete_or_overlaid_source_fails(mutation):
    raw, *_ = hand_case(); back = raw['backbook']; mutation(back)
    with pytest.raises(ArithmeticError):
        oracle._backbook(back, oracle._normalize(back['specification']), lambda:False)


def full_request():
    c = json.loads((ROOT/'examples/models/product-candidate-journal-1.json').read_text())
    c['product']['units']=c['product']['units'][:1]
    c['owners']=c['owners'][:1];c['allocation']=c['allocation'][:1]
    c['specification']['positions']=c['specification']['positions'][:1]
    back = json.loads((ROOT/'examples/models/dated-ledger-1.json').read_text())
    start = c['as_of_ordinal']; days = c['reporting_end_ordinal']-start
    back['specification'].update(horizon_days=days,accounts=deepcopy(c['specification']['accounts']),scenarios=deepcopy(c['specification']['scenarios']))
    back['specification']['accounts'][0]['equity'] += 100.
    account=c['specification']['accounts'][0]['id']
    back['specification']['positions'][0].update(id='actual-backbook',maturity_day=0,account=account)
    back['dated'].update(as_of_ordinal=start,reporting_horizon_days=days)
    back['dated']['owners'][0]['position_id']='actual-backbook'
    back['dated']['owners'][0]['account_id']=account
    for r,m in zip(back['dated']['runs'],c['run_mapping'],strict=True):
        r['scenario_id']=m['ledger_scenario_id']
    source=back['dated']['sources'][0];source['schedule'].update(as_of_ordinal=start,horizon_days=days)
    e=source['schedule']['events'][0];e.update(position_id='actual-backbook',payment_ordinal=start+days,day_offset=days,
        accrual_period=dict(start_ordinal=start,end_ordinal=start+days))
    source['coverage'][0]['position_id']='actual-backbook'
    plan=back['dated']['accrual_plans'][0];plan['position_id']='actual-backbook';plan['segments'][0].update(start_ordinal=start,end_ordinal=start+days)
    combined=deepcopy(back['specification']);combined['positions']+=deepcopy(c['specification']['positions'])
    return dict(schema=oracle.SCHEMA,candidate={k:deepcopy(c[k]) for k in ('product','allocation','owners','policy','run_mapping')},
        backbook=back,combined_specification=combined,policy=dict(identity=dict(id='mixed-policy',revision='1'),
            expectation='separately_supplied_physical_expected_paths-1',phase='opening_acquisition_recognition_settlement-1',
            backbook_basis='reviewed_supplied_dated_recognition-1',tail='retain_complete-1',fees='reject',defaults='reject',marks='reject'),
        opening_identity=dict(id='actual-opening',revision='1'))


@pytest.fixture(scope='module')
def full_source():
    from portfolio_risk.analytics.model_contracts import prepare_product_units
    raw=full_request();p=prepare_product_units(raw['candidate']['product'])
    wrapper=dict(schema='product-strategy-preparation-1',units=oracle.candidate._unit_identities(raw['candidate']['product'],p),preparation=p)
    spec,owners,runs,back_events,capture,end,context=oracle._admit(raw,wrapper,lambda:False)
    combined,acquisitions,declared,events,pools,signs=oracle._combined(raw,owners,runs,back_events,capture,end)
    return raw,wrapper,spec,back_events,capture,end,context,combined,acquisitions,declared,events,pools,signs


def result_from_source(source):
    raw,wrapper,spec,back_events,capture,end,context,combined,acquisitions,declared,events,pools,signs=source
    tables=oracle._replay(raw,spec,events,acquisitions['events'],end,pools,signs,lambda:False)
    fullspec=deepcopy(spec);fullspec['horizon_days']=end-context['as_of_ordinal']
    backspec=oracle._normalize(raw['backbook']['specification']);backspec['horizon_days']=fullspec['horizon_days']
    _,work=oracle.candidate._journal_admission(context,wrapper['preparation'],[p['full_end_ordinal'] for p in capture['paths']],capture['admitted_daily_rows'],capture['admitted_work'])
    count=len(raw['backbook']['dated']['runs'])
    work+=(len(back_events)+sum(len(e['segments']) for e in back_events))*count+capture['admitted_daily_rows']*24
    work+=oracle._dated_work(fullspec,events,end,count,acquisitions['events'])
    preparation=dict(raw_request=deepcopy(raw),native_preparation=wrapper,candidate_capture=capture,combined_dated_request=combined,
        acquisitions=acquisitions,backbook_source_ends=declared,reporting_end_ordinal=context['reporting_end_ordinal'],full_end_ordinal=end,admitted_work=work)
    return deepcopy(dict(tables,schema=oracle.SCHEMA,preparation=preparation,protocol=3,complete=True,reporting_end_ordinal=context['reporting_end_ordinal'],
        full_end_ordinal=end,independent_persisted_verification=False,dynamic_mixed_acceptance=False,
        scope='single pooled native full-tail expected dated backbook/candidate GL with reviewed original opening and candidate EIR anchors; supplied dated backbook recognition, separate physical expectations, end-of-day cash constraints; no observed-settlement, accounting-standard, empirical, joint-credit/stochastic or independently persisted verification claim'))


def test_complete_source_and_public_receipt_without_native_callbacks(full_source,monkeypatch):
    result=result_from_source(full_source)
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*args,**kwargs):
        pytest.fail('mixed publication oracle called native finance')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    raw=full_source[0];receipt=oracle.verify(raw,result)
    assert receipt['mixed_performing_only'] and receipt['ordered_mixed_journal_recomputed']
    assert not receipt['dynamic_credit_acceptance'] and not receipt['empirical_validation']
    assert receipt==oracle.verify(json.loads(json.dumps(raw)),json.loads(json.dumps(result)))
    cost=sum(a['face']*u['quote_price_per_face_unit'] for a,u in zip(raw['candidate']['allocation'],raw['candidate']['product']['units'],strict=True))
    assert result['closing_statements'][0]['cash']==pytest.approx(1000-cost+20.89+100.3,abs=1e-9)


@pytest.mark.parametrize('mutate',[
    lambda r:r['journal'].reverse(),
    lambda r:r['journal'].pop(),
    lambda r:r['trial_balance'][0].update(balance=r['trial_balance'][0]['balance']+1.),
    lambda r:r['closing_statements'][0].update(cash=9999.),
    lambda r:r['dated_rights'].pop(),
    lambda r:r['reporting_rights'].pop(),
    lambda r:r['event_lineage'][0].update(source_revision='forged'),
    lambda r:r['acquisition_lineage'][0].update(source_weight=.5),
    lambda r:r['preparation']['acquisitions']['events'][0].update(cost=0.),
    lambda r:r['preparation']['candidate_capture']['paths'][0].update(source_weight=.5),
    lambda r:r['preparation']['combined_dated_request']['dated']['sources'][0]['schedule']['events'][0]['components'].update(principal=99.),
    lambda r:r['preparation']['backbook_source_ends'][0].update(original_container_end_ordinal=0),
    lambda r:r.update(complete=False),
    lambda r:r.update(dynamic_mixed_acceptance=True),
])
def test_complete_publication_rejects_tampered_source_journal_and_tails(full_source,mutate):
    result=result_from_source(full_source);mutate(result)
    with pytest.raises(ArithmeticError):
        oracle.verify(full_source[0],result)


@pytest.mark.parametrize('path',[
    ('reporting_end_ordinal',),
    ('preparation','reporting_end_ordinal'),
    ('preparation','acquisitions','reporting_ordinal'),
    ('preparation','backbook_source_ends',0,'original_container_end_ordinal'),
])
def test_same_value_float_cannot_replace_exact_mixed_date_identity(full_source,path):
    result=result_from_source(full_source);node=result
    for key in path[:-1]:
        node=node[key]
    node[path[-1]]=float(node[path[-1]])
    with pytest.raises(ArithmeticError,match='exact integer'):
        oracle.verify(full_source[0],result)


def execute_native(raw):
    # Explicit producer execution occurs only in tests, before the independent
    # verifier. The debug override is never an application runtime fallback.
    path=Path(os.environ.get('MIXED_TEST_BINARY',ROOT/'packages/portfolio-decision-native/target/release'/('portfolio-strategy.exe' if os.name=='nt' else 'portfolio-strategy')))
    assert path.is_file(), 'fresh native mixed artifact required; no skipped validation'
    envelope=dict(op='analyze_product_mixed_dated',schema=oracle.SCHEMA,execution_timeout_ms=30000,request=raw)
    completed=subprocess.run([str(path)],input=json.dumps(envelope,allow_nan=False),capture_output=True,text=True,timeout=40,check=True)
    response=json.loads(completed.stdout)
    assert 'ok' in response, response
    return response['ok']


@pytest.fixture(scope='module')
def native_mixed():
    raw=json.loads((ROOT/'examples/models/product-mixed-dated-1.json').read_text())
    return raw,execute_native(raw)


def test_fresh_native_complete_mixed_publication_gate(native_mixed,monkeypatch):
    raw,result=native_mixed
    from portfolio_risk.core import quant_native,decision_native,product_mixed_native
    def forbidden(*args,**kwargs):
        pytest.fail('independent mixed verification invoked a native financial callback')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    monkeypatch.setattr(product_mixed_native,'run',forbidden)
    receipt=oracle.verify(raw,result)
    assert receipt['verified'] and receipt['complete_cash_rights_basis_tail_verified']
    assert receipt==oracle.verify(json.loads(json.dumps(raw)),json.loads(json.dumps(result)))
    unit=raw['candidate']['product']['units'][0];face=raw['candidate']['allocation'][0]['face'];cost=face*unit['quote_price_per_face_unit']
    closing=result['closing_statements'][0]
    assert closing['cash']==pytest.approx(1000-cost+face*1.0445+101.,abs=1e-9)
    assert closing['equity']==pytest.approx(1100+face*1.0445-cost+1.,abs=1e-9)
    assert len([r for r in result['journal'] if r['event']=='opening' and r['gl_account']=='cash'])==2


@pytest.mark.parametrize('mutate',[
    lambda r:r['journal'].reverse(),
    lambda r:r['journal'].pop(),
    lambda r:r['dated_rights'].pop(),
    lambda r:r['reporting_rights'].pop(),
    lambda r:r['event_lineage'][0].update(input_revision='forged'),
    lambda r:r['acquisition_lineage'][0].update(source_weight=.5),
    lambda r:r['preparation']['candidate_capture']['paths'][0]['daily_income']['rows'][0].update(effective_interest=50.),
    lambda r:r['preparation']['acquisitions']['events'][0].update(coupon_sign=-1.),
    lambda r:r['preparation']['acquisitions']['events'][0].update(reporting_ordinal=0),
    lambda r:r['preparation']['combined_dated_request']['dated']['sources'][0]['schedule']['events'][0]['components'].update(principal=99.),
    lambda r:r['closing_statements'][0].update(equity=0.),
    lambda r:r['daily_balance'][0].update(cash=9999.),
    lambda r:r['exposures'][0].update(balance=200.),
    lambda r:r['ledger'][0].update(cash=0.),
    lambda r:r['preparation'].update(admitted_work=r['preparation']['admitted_work']+1),
    lambda r:r['preparation']['candidate_capture']['paths'][0].update(native_source_index=0.),
    lambda r:r['journal'][0].update(day=0.),
])
def test_native_persisted_tamper_fails_closed(native_mixed,mutate):
    raw,result=native_mixed;changed=deepcopy(result);mutate(changed)
    with pytest.raises(ArithmeticError):
        oracle.verify(raw,changed)


def test_actual_nonmonthly_reporting_day_is_retained_through_longer_cash_tail():
    raw,spec,events,acq,_,pools,signs=hand_case()
    start=raw['backbook']['dated']['as_of_ordinal'];spec['horizon_days']=45
    raw['backbook']['dated']['reporting_horizon_days']=45
    for e in events:
        if e['event']['position_id']=='p':
            e['event'].update(payment_ordinal=start+60,day_offset=60)
            e['event']['accrual_period']['end_ordinal']=start+60
            e['segments'][0]['end_ordinal']=start+60
    events.sort(key=lambda e:e['event']['payment_ordinal'])
    result=oracle._replay(raw,spec,events,acq,start+60,pools,signs,lambda:False)
    assert len(result['reported_daily_balance'])==2
    assert all(r['day']==45 for r in result['reported_daily_balance'])
    assert result['reported_daily_balance'][0]['cash']==pytest.approx(53.)
    assert result['reported_daily_balance'][0]['equity']==pytest.approx(153.225)
    assert result['closing_statements'][0]['cash']==pytest.approx(153.3)


def test_escaped_metadata_capacity_fails_before_candidate_daily_capture(full_source,monkeypatch):
    raw,wrapper,*_=deepcopy(full_source)
    escaped='\\"'*60
    # Entire retained projections appear in each conservative width. Repeated
    # bounded origins test the full escaped metadata count, not a row-size guess.
    for unit in wrapper['preparation']['units']:
        for source in unit['complete_purchase_result']['native_sources']:
            if source['role']=='physical':
                source['projection']['recognition']*=20
                for r in source['projection']['recognition']:
                    r['origin_id']=escaped
    spec=oracle._normalize(raw['combined_specification']);_,_,events,end=oracle._backbook(raw['backbook'],oracle._normalize(raw['backbook']['specification']),lambda:False)
    with pytest.raises(ArithmeticError,match='escaped metadata capacity'):
        oracle._capacity(raw,wrapper,spec,events,end)
