"""Raw independent Card2 source, gross/basis and compact-arena verification."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import card_credit_v2_reference as oracle

FIXTURE=Path(__file__).resolve().parents[2]/'portfolio-model-core/tests/fixtures/card_credit_v2_proof.json'
CASES=json.loads(FIXTURE.read_text())['cases']
ACCEPTED=[i for i,c in enumerate(CASES) if c['expected'] is not None]
REJECTED=[i for i,c in enumerate(CASES) if c['expected'] is None]


def raw(i=0):return deepcopy(CASES[i]['raw'])


def expected(i=0):
    r=oracle.reconstruct(raw(i));r['retained_capacity_charge']=oracle._size(r)+65536
    return r


def expanded(result):
    return dict(economic=result['economic'],trace=[dict(atomic_action=a['atomic_action'],ordinal=a['ordinal'],source_kind=a['source']['kind'],recognition_indices=a['source']['recognition_indices'],cash_event_id=a['source']['cash_event_id'],interest_recovery_index=a['source']['interest_recovery_index']) for a in result['actions']],
        daily=[dict(ordinal=d['ordinal'],snapshot=d['snapshot']) for d in result['daily']],events=result['events'],shared_intents=result['shared_intents'],income_steps=result['income_steps'],reporting=result['reporting'],closing=result['closing'],complete=result['complete'])


@pytest.mark.parametrize('i',ACCEPTED,ids=[CASES[i]['name'] for i in ACCEPTED])
def test_complete_independent_source_and_expanded_original_proof(i):
    request=raw(i);before=deepcopy(request);result=oracle.reconstruct(request)
    oracle._equal(expanded(result),CASES[i]['expected'])
    assert request==before
    receipt=oracle.verify(request,expected(i));assert receipt['compact_arena_ranges_recomputed']
    assert receipt['original_eir_distinct_from_basis_realization']
    assert not receipt['mixed_ledger_adoption'] and not receipt['empirical_validation']


@pytest.mark.parametrize('i',REJECTED,ids=[CASES[i]['name'] for i in REJECTED])
def test_original_source_rejections_preserve_guards(i):
    with pytest.raises(ArithmeticError):oracle.reconstruct(raw(i))


def test_positive_original_eir_and_principal_free_right_hand():
    r=oracle.reconstruct(raw());start=raw()['contract']['as_of_ordinal'];p=r['daily'][10]['snapshot'];k=math.log1p(.02)
    assert p['core']['live'][0]['principal']==0.
    assert p['core']['live'][0]['accrued_interest']==pytest.approx(1.)
    carry=100*math.expm1(k*10/365)
    assert p['live_interest_basis']==pytest.approx(carry-1.,abs=1e-12)
    assert p['original_eir_income']==pytest.approx(carry,abs=1e-12)
    assert r['daily'][20]['snapshot']['original_eir_income']==pytest.approx(carry+carry*math.expm1(k*10/365),abs=1e-12)
    # No new original yield after exact source default on day20.
    assert r['daily'][30]['snapshot']['original_eir_income']==r['daily'][20]['snapshot']['original_eir_income']
    assert r['closing']['cash_received']==pytest.approx(101.)
    assert r['closing']['reviewed_basis_income']>0.
    assert all(a['ordinal']>=start for a in r['actions'])


def test_zero_eir_par_collection_basis_income_is_separate():
    r=oracle.reconstruct(raw(5))
    assert r['closing']['original_eir_income']==0.
    assert r['closing']['reviewed_basis_income']==pytest.approx(1.)
    assert r['closing']['cash_received']==pytest.approx(101.)
    assert all(e['original_eir_income']==0. for e in r['events'])
    assert any(e['reviewed_basis_income']>0. for e in r['events'])


def test_each_atomic_arena_partition_and_daily_commit_is_complete():
    r=oracle.reconstruct(raw(10));cursors={k:0 for k in ('events','shared_intents','income_steps')}
    for i,a in enumerate(r['actions']):
        assert a['atomic_action']==i+1
        for k,left in cursors.items():
            span=a[k];assert span['start']==left and span['end']>=left;cursors[k]=span['end']
        assert all(e['atomic_action']==a['atomic_action'] for e in r['events'][a['events']['start']:a['events']['end']])
        assert a['committed']['action_count']==a['atomic_action']
    assert all(cursors[k]==len(r[k]) for k in cursors)
    previous=0
    for d in r['daily']:
        assert d['actions']['start']==previous;previous=d['actions']['end']
        if previous:oracle._equal(d['snapshot'],r['actions'][previous-1]['committed'])
    assert previous==len(r['actions'])


def test_terminal_transfer_and_cash_are_one_atomic_action():
    r=oracle.reconstruct(raw(0));action=next(a for a in r['actions'] if a['source']['kind']=='native_terminal_principal_cash')
    events=r['events'][action['events']['start']:action['events']['end']]
    assert [e['event'] for e in events]==['reviewed_principal_basis_to_interest_right','actual_terminal_principal_cash']
    assert len({e['atomic_action'] for e in events})==1
    assert all(e['original_eir_income']==e['reviewed_basis_income']==0. for e in events)
    assert action['committed']['core']['live'][0]['book_adjustment']==0.


def test_posting_source_is_events_shared_intents_are_evidence():
    r=oracle.reconstruct(raw(8));posted={}
    for e in r['events']:
        assert abs(sum(l['amount'] for l in e['lines']))<1e-9
        for l in e['lines']:posted[l['account']]=posted.get(l['account'],0.)+l['amount']
    assert posted['cash']==pytest.approx(r['closing']['cash_received'])
    assert r['shared_intents']
    doubled=posted['cash']+sum(p['amount'] for i in r['shared_intents'] for p in i['postings'] if p['account']=='cash')
    assert doubled>r['closing']['cash_received']


def test_retained_claim_has_gross_and_basis_separate_even_net_zero():
    request=raw(8);request['interest_recoveries']=[];request['review']['cost']=100.;request['review']['original_annual_effective_rate']=0.
    r=oracle.reconstruct(request)
    assert r['complete'] is False
    claim=r['closing']['core']['claims'][0]
    assert claim['outstanding_accrued_interest']>0.
    assert r['closing']['claim_interest_basis'][claim['claim_id']]<0.
    assert oracle.carrying(r['closing'])==pytest.approx(0.,abs=1e-12)


def test_existing_card1_guard_unchanged():
    from test_card_credit_reference import invalid
    from portfolio_risk.analytics import card_credit_reference as card1
    with pytest.raises(ArithmeticError):card1.reconstruct(invalid('negative_tail_basis'))


TAMPERS=[
    ('raw_request','review','economic_source','revision'),('economic','recognition',0,'interest'),
    ('economic','statements',0,'minimum_payment'),('economic','default','accrued_interest'),
    ('acquisition_opening','core','live',0,'principal'),('acquisition_opening','core','live',0,'book_adjustment'),
    ('actions',0,'events','start'),('actions',0,'events','end'),('actions',0,'income_steps','end'),
    ('actions',0,'source','recognition_indices',0),('actions',0,'source','kind'),
    ('actions',0,'committed','original_eir_income'),('actions',0,'committed','core','live',0,'accrued_interest'),
    ('events',0,'atomic_action'),('events',0,'lines',0,'amount'),('events',0,'original_yield_anchor','revision'),
    ('income_steps',0,'request','original_annual_effective_rate'),('income_steps',0,'result','rows',0,'effective_interest'),
    ('daily',10,'snapshot','live_interest_basis'),('daily',10,'actions','start'),('daily',10,'actions','end'),
    ('reporting','cash_received'),('closing','reviewed_basis_income'),('closing','claim_interest_basis','interest-claim'),
    ('admitted_work',),('reserved_bytes',),('mixed_ledger_adoption',),('complete',),
]


@pytest.mark.parametrize('path',TAMPERS,ids=['/'.join(map(str,p)) for p in TAMPERS])
def test_compact_source_state_and_range_tampering_rejects(path):
    i=8 if 'default' in path or 'claim_interest_basis' in path else 0
    r=expected(i);cursor=r
    for k in path[:-1]:cursor=cursor[k]
    value=cursor[path[-1]];cursor[path[-1]]=not value if type(value) is bool else value+'x' if isinstance(value,str) else value+1 if type(value) is int else value+.01
    with pytest.raises(ArithmeticError):oracle.verify(raw(i),r)


@pytest.mark.parametrize('path',[('admitted_work',),('reserved_bytes',),('actions',0,'atomic_action'),('actions',0,'events','end'),('daily',0,'ordinal'),('actions',0,'source','recognition_indices',0)])
def test_same_value_float_identity_is_rejected(path):
    r=expected();cursor=r
    for k in path[:-1]:cursor=cursor[k]
    cursor[path[-1]]=float(cursor[path[-1]])
    with pytest.raises(ArithmeticError):oracle.verify(raw(),r)


def test_cancellation_and_no_native_financial_callback(monkeypatch):
    from portfolio_risk.core import model_native,quant_native,decision_native
    from portfolio_risk.analytics import card_credit_reference as card1
    def forbidden(*a,**k):pytest.fail('independent Card2 verifier invoked native/old income financial callback')
    monkeypatch.setattr(model_native,'run',forbidden);monkeypatch.setattr(quant_native,'term_call',forbidden);monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden);monkeypatch.setattr(card1,'reconstruct',forbidden)
    assert oracle.verify(raw(),expected())['verified']
    with pytest.raises(InterruptedError):oracle.verify(raw(),expected(),lambda:True)


def invalid(kind):
    r=raw();c=r['contract']
    if kind=='pricing':c['identity']['measure']='pricing'
    elif kind=='pcd':r['review']['accounting_basis']='inferred_pcd'
    elif kind=='missing_source_revision':r['review']['economic_source']['revision']='wrong-source'
    elif kind=='post_default_action':c['steps'].append(dict(ordinal=c['as_of_ordinal']+21,action=dict(kind='payment',amount=.1)))
    elif kind=='unpaired_policy':r['review']['policy']['basis']='transfer_until_reviewed_chargeoff'
    elif kind=='principal_recovery':c['collection_fraction']=.4
    elif kind=='positive_principal_default':
        r=raw(8);r['contract']['steps']=[s for s in r['contract']['steps'] if s['action']['kind']!='payment']
    elif kind=='false_default_closure':
        r['termination']={'kind':'source_default'}
    elif kind=='wrong_channel_policy':r['policy']['principal_basis']='automatic_basis_haircut'
    elif kind=='principal_overflow':r['review']['cost']=1e15;r['review']['original_annual_effective_rate']=10.
    elif kind=='long_horizon':c['horizon_days']=65
    elif kind=='unknown_field':r['amount_override']=0.
    elif kind=='same_float_date':r['reporting_ordinal']=float(r['reporting_ordinal'])
    elif kind=='capacity_metadata':
        c['horizon_days']=64
        for ident in [r['review']['identity'],r['review']['original_yield_anchor'],r['review']['economic_source'],r['review']['policy']['identity'],r['policy']['review']]:
            ident['id']=ident['revision']='"'*100000
        c['identity']['source_revision']=r['review']['economic_source']['revision']
    else:raise AssertionError(kind)
    return r


INVALID=['pricing','pcd','missing_source_revision','post_default_action','unpaired_policy','principal_recovery','positive_principal_default','false_default_closure','wrong_channel_policy','principal_overflow','long_horizon','unknown_field','same_float_date','capacity_metadata']


@pytest.mark.parametrize('kind',INVALID)
def test_strict_card2_domain_and_preflight(kind):
    with pytest.raises(ArithmeticError):oracle.reconstruct(invalid(kind))


@pytest.mark.parametrize('charge',[0,True,1.,10**12])
def test_capacity_metadata_never_bypasses_complete_financial_comparison(charge):
    r=expected();r['retained_capacity_charge']=charge
    with pytest.raises(ArithmeticError):oracle.verify(raw(),r)


def multi_class():
    r=raw();c=r['contract'];c['opening_principal']=[40.,30.,30.]
    c['rates']['cash_advance']=.2;c['rates']['promotional']=.15
    provisional=deepcopy(c);provisional['steps']=[s for s in provisional['steps'] if not(s['ordinal']==r['termination']['ordinal'] and s['action']['kind']=='payment')]
    source=oracle.card.project(provisional);point=source['points'][r['termination']['ordinal']-c['as_of_ordinal']]
    interest=sum(point['billed_interest']+point['unbilled_interest'],0.)
    for step in c['steps']:
        if step['ordinal']==r['termination']['ordinal'] and step['action']['kind']=='payment':step['action']['amount']=interest
    return r


def test_three_classes_statement_does_not_earn_interest_twice():
    r=oracle.reconstruct(multi_class());day10=r['daily'][10]['snapshot'];sourcepoint=r['economic']['points'][10]
    assert all(v==0. for v in sourcepoint['principal'])
    assert all(v>0. for v in sourcepoint['billed_interest'])
    assert day10['core']['live'][0]['accrued_interest']==pytest.approx(sum(sourcepoint['billed_interest']))
    assert len(r['economic']['recognition'])==30
    assert r['complete'] is True
    assert r['closing']['cash_received']<101.
