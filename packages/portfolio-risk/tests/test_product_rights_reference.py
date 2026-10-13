"""Independent one-pool gross-rights/basis replay, never a finance backend."""
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess
import pytest
from portfolio_risk.analytics import product_rights_reference as oracle

ROOT=Path(__file__).resolve().parents[3]

def raw():return json.loads((ROOT/'examples/models/product-mixed-card-credit-2.json').read_text())

@pytest.fixture(scope='module')
def prepared():
    from portfolio_risk.analytics.model_contracts import prepare_product_units
    r=raw();product=r['performing']['candidate']['product'];p=prepare_product_units(product)
    return dict(schema='product-strategy-preparation-1',units=oracle.dated.candidate._unit_identities(product,p),preparation=p)

@pytest.fixture(scope='module')
def result(prepared):return oracle.reconstruct(raw(),prepared)

def forbidden(*a,**k):raise AssertionError('native finance callback in independent verifier')

def test_raw_money_once_gross_basis_atomic_and_complete_pool(prepared,result,monkeypatch):
    from portfolio_risk.core import model_native,quant_native,decision_native
    monkeypatch.setattr(model_native,'run',forbidden);monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    r=raw();before=deepcopy(r);original=deepcopy(result);receipt=oracle.verify(r,result)
    assert receipt['verified'] and receipt['card_v2_rights_owned'] and receipt['capital_ftp_eligible'] is False
    assert receipt['closing_capital_eligible'] is True and result['closing_capital_eligible'] is True
    assert r==before and result==original
    assert receipt==oracle.verify(json.loads(json.dumps(r)),json.loads(json.dumps(result)))
    assert all(c['cash']==pytest.approx(1102.2506993647357,abs=1e-9) for c in result['closing_statements'])
    assert result['admitted_work']==196384
    assert result['preparation']['product_reservation']==dict(work=185864,retained_bytes=55056108,full_end_ordinal=739996,paths=2)
    assert not result['product_lineage']
    assert len(result['product_rights_acquisition_lineage'])==2
    p=raw()['product_owners'][0]['position_id'];start=r['performing']['backbook']['dated']['as_of_ordinal']
    day10=next(s['unweighted_rights'] for s in result['product_rights_daily'] if s['scenario']=='baseline' and s['ordinal']==start+10)
    assert day10['core']['live'][0]['principal']==0.
    assert day10['core']['live'][0]['accrued_interest']==pytest.approx(1.)
    assert day10['live_interest_basis']<0.
    for row in result['product_rights_lineage']:
        if row['transaction_number'] is None:continue
        postings=[j for j in result['journal'] if j['scenario']==row['scenario'] and j['transaction_number']==row['transaction_number']]
        assert postings and all(j['event'].startswith('card_rights_atomic:') for j in postings)
        assert abs(sum(j['value'] for j in postings))<1e-9
    assert any(j['gl_account']=='live_interest_basis_adjustment' and j['instrument_id']==p for j in result['journal'])
    assert {j['gl_account'] for j in result['journal']}>={'pnl:original_effective_interest','pnl:reviewed_basis_realization'}

@pytest.mark.parametrize('target',['journal_order','journal_value','trial','closing','daily','report','source','source_charge','action_range','action_index_float','atomic_float','day_float','source_index_float','receipt_evidence','receipt_event','receipt_income','acquisition','basis','right','statement','commitment','weight','reserve','capital_missing','capital_true','capital_float','row_capital','gross_flag','producer_persisted','producer_dynamic','scope','closing_missing','closing_false','closing_integer'])
def test_adversarial_complete_output_reject(result,target):
    r=raw();o=deepcopy(result)
    if target=='journal_order':o['journal'].reverse()
    elif target=='journal_value':o['journal'][0]['value']+=1
    elif target=='trial':o['trial_balance'][0]['balance']+=1
    elif target=='closing':o['closing_statements'][0]['cash']+=1
    elif target=='daily':o['daily_balance'][0]['assets']+=1
    elif target=='report':o['reporting_end_ordinal']=float(o['reporting_end_ordinal'])
    elif target=='source':o['product_sources'][0]['native']['result']['economic']['collected_principal']+=1
    elif target=='source_charge':o['product_sources'][0]['native']['result']['retained_capacity_charge']=1
    elif target=='action_range':o['product_rights_lineage'][0]['native_action']['events']['end']+=1
    elif target=='action_index_float':o['product_rights_lineage'][0]['action_index']=0.
    elif target=='atomic_float':o['product_rights_lineage'][0]['native_action']['atomic_action']=1.
    elif target=='day_float':o['product_rights_daily'][0]['unweighted_rights']['last_ordinal']=float(o['product_rights_daily'][0]['unweighted_rights']['last_ordinal'])
    elif target=='source_index_float':
        row=next(x for x in o['product_rights_lineage'] if x['native_action']['source']['recognition_indices']);row['native_action']['source']['recognition_indices'][0]=float(row['native_action']['source']['recognition_indices'][0])
    elif target=='receipt_evidence':o['product_rights_lineage'][0]['shared_intents_evidence_only'].append({})
    elif target=='receipt_event':o['product_rights_lineage'][0]['complete_events'][0]['lines'][0]['amount']+=1
    elif target=='receipt_income':o['product_rights_lineage'][0]['native_income_steps'][0]['result']['rows'][0]['effective_interest']+=1
    elif target=='acquisition':o['product_rights_acquisition_lineage'][0]['acquisition']['cost']+=1
    elif target=='basis':o['product_rights_daily'][10]['unweighted_rights']['live_interest_basis']=0.
    elif target=='right':o['product_rights_daily'][10]['unweighted_rights']['core']['live'][0]['accrued_interest']=0.
    elif target=='statement':
        row=next(x for x in o['product_daily'] if x['product_receipts'] and x['product_receipts']['statement_lineage']);row['product_receipts']['statement_lineage'][0]['interest_recognized_again']=True
    elif target=='commitment':o['product_daily'][0]['product_receipts']['commitment_source']['funded_contingent_draw']=1.
    elif target=='weight':o['product_rights_daily'][0]['source_weight']=.5
    elif target=='reserve':o['preparation']['product_reservation']['retained_bytes']-=1
    elif target=='capital_missing':o.pop('capital_ftp_eligible')
    elif target=='capital_true':o['capital_ftp_eligible']=True
    elif target=='capital_float':o['capital_ftp_eligible']=0.
    elif target=='row_capital':o['product_rights_daily'][0]['capital_ftp_eligible']=True
    elif target=='gross_flag':o['product_rights_daily'][0]['gross_rights_and_signed_basis_separate']=False
    elif target=='closing_missing':o.pop('closing_capital_eligible')
    elif target=='closing_false':o['closing_capital_eligible']=False
    elif target=='closing_integer':o['closing_capital_eligible']=1
    elif target=='producer_persisted':o['independent_persisted_verification']=True
    elif target=='producer_dynamic':o['dynamic_mixed_acceptance']=True
    else:o['scope']='approved empirical'
    with pytest.raises(ArithmeticError):oracle.verify(r,o)


def bind_case(r,index):
    cases=json.loads((ROOT/'packages/portfolio-model-core/tests/fixtures/card_credit_v2_proof.json').read_text())['cases']
    source=cases[index]['raw'];start=r['performing']['backbook']['dated']['as_of_ordinal'];delta=start-source['contract']['as_of_ordinal']
    def dates(v,key=''):
        if isinstance(v,dict):return {k:dates(x,k) for k,x in v.items()}
        if isinstance(v,list):return [dates(x,key) for x in v]
        if key.endswith('ordinal') and v is not None:return v+delta
        return v
    owner=r['product_owners'][0]
    for run in owner['runs']:
        req=dates(deepcopy(source));req['schema']='card-credit-accounting-2';req['review']['accounting_basis']='supplied_non_pcd_gross_cost-1'
        req['contract']['identity'].update(position_id=owner['position_id'],run_id=run['ledger_run_id'])
        req['review'].update(position_id=owner['position_id'],economic_source=deepcopy(owner['source_identity']),original_yield_anchor=deepcopy(owner['original_yield_anchor']))
        req['contract']['identity']['source_revision']=owner['source_identity']['revision'];req['reporting_ordinal']=start+req['contract']['horizon_days']
        run['paths'][0]['request']['request']=req
    return r

@pytest.mark.parametrize('case',[i for i in range(12) if i!=6])
def test_full_physical_default_yield_premium_and_actual_collection_cases(prepared,case):
    r=bind_case(raw(),case);o=oracle.reconstruct(r,prepared);assert oracle.verify(r,o)['card_v2_rights_owned']
    assert all(oracle.rights.complete(s['native']['result']['closing']) for s in o['product_sources'])
    assert all(abs(x['reconciliation_error'])<1e-9 for x in o['closing_statements'])

@pytest.mark.parametrize('variant',['zero_face','half_face','weighted','zero_path','future','negative_yield'])
def test_scaling_future_and_zero_tail_conservation(prepared,variant):
    r=raw();owner=r['product_owners'][0]
    if variant in ('zero_face','half_face'):owner['allocated_face']=0. if variant=='zero_face' else 50.
    elif variant in ('weighted','zero_path'):
        run=owner['runs'][0];p=deepcopy(run['paths'][0]);run['paths'][0]['source_weight']=.25 if variant=='weighted' else 1.;p.update(path_id='second-path',source_weight=.75 if variant=='weighted' else 0.);run['paths'].append(p)
        for run in owner['runs']:
            for path in run['paths']:
                req=path['request']['request'];req['contract']['horizon_days']=26;req['reporting_ordinal']=req['contract']['as_of_ordinal']+26
    elif variant=='negative_yield':r=bind_case(r,1)
    else:
        for run in owner['runs']:
            req=run['paths'][0]['request']['request'];c=req['contract'];c['as_of_ordinal']+=5;c['horizon_days']-=5;req['review']['acquisition_ordinal']+=5
            c['rates']['promotional_expiry_ordinal']+=5
            for cycle in c['cycles']:cycle['statement_ordinal']+=5;cycle['due_ordinal']+=5
            for step in c['steps']:step['ordinal']+=5
            req['termination']['ordinal']+=5
    o=oracle.reconstruct(r,prepared);assert oracle.verify(r,o)['verified']
    if variant=='zero_face':assert all(x['transaction_number'] is None for x in o['product_rights_lineage'])
    if variant=='future':assert all(x['ordinal']>=owner['runs'][0]['paths'][0]['request']['request']['review']['acquisition_ordinal'] for x in o['product_rights_daily'])

@pytest.mark.parametrize('bad',['unpaid_tail','zero_weight_unpaid','allowance','unknown_kind','pricing','binding','missing_run','source_float','gross_net_zero','oversize'])
def test_raw_unsupported_admission_before_publication(prepared,bad):
    r=raw();owner=r['product_owners'][0];p=owner['runs'][0]['paths'][0];req=p['request']['request']
    if bad in ('unpaid_tail','zero_weight_unpaid'):
        req['contract']['steps']=req['contract']['steps'][:1]+[dict(ordinal=req['contract']['as_of_ordinal']+11,action=dict(kind='default'))];req['termination']=dict(kind='source_default');req['review']['policy'].update(accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff')
        if bad=='zero_weight_unpaid':p['source_weight']=0.;q=deepcopy(owner['runs'][1]['paths'][0]);q['request']['request']['contract']['identity']['run_id']=owner['runs'][0]['ledger_run_id'];q['path_id']='positive-path';owner['runs'][0]['paths'].append(q)
    elif bad=='allowance':owner['initial_allowance_review']=dict(review=dict(id='review',revision='1'),amount=0.)
    elif bad=='unknown_kind':p['request']['kind']='generic'
    elif bad=='pricing':req['contract']['identity']['measure']='pricing'
    elif bad=='binding':req['review']['principal']=50.
    elif bad=='missing_run':owner['runs'].pop()
    elif bad=='source_float':req['reporting_ordinal']=float(req['reporting_ordinal'])
    elif bad=='gross_net_zero':req['contract']['steps'].pop(1)
    else:owner['unit_id']='\\"'*1000000
    with pytest.raises(ArithmeticError):oracle.reconstruct(r,prepared)


def test_cancellation_and_old_output_boundary(prepared):
    with pytest.raises(InterruptedError):oracle.reconstruct(raw(),prepared,check=lambda:True)
    old=json.loads((ROOT/'examples/models/product-mixed-product-credit-1.json').read_text())
    a=oracle.legacy.reconstruct(old,prepared);b=oracle.reconstruct(old,prepared)
    assert json.dumps(a,separators=(',',':'))==json.dumps(b,separators=(',',':'))
    assert 'capital_ftp_eligible' not in b and 'product_rights_daily' not in b


def process(r):
    binary=Path(os.environ.get('PRODUCT_RIGHTS_TEST_BINARY',str(ROOT/'packages/portfolio-decision-native/target/release'/('portfolio-strategy.exe' if os.name=='nt' else 'portfolio-strategy'))))
    completed=subprocess.run([str(binary)],input=json.dumps(dict(op='analyze_product_mixed_product_credit',schema='product-mixed-product-credit-1',execution_timeout_ms=30000,request=r)),text=True,capture_output=True,timeout=35)
    value=json.loads(completed.stdout)
    if 'ok' not in value:raise ArithmeticError(value.get('error',completed.stderr))
    return value['ok']

@pytest.mark.parametrize('case',[i for i in range(12) if i!=6])
def test_process_complete_raw_card2_source_and_atomic_single_gl(case):
    r=bind_case(raw(),case);o=process(r);assert oracle.verify(r,o)['card_v2_rights_owned']

@pytest.mark.parametrize('variant',['canonical','zero_face','half_face','weighted','future'])
def test_process_actual_scale_and_acquisition(variant):
    r=raw();owner=r['product_owners'][0]
    if variant=='zero_face':owner['allocated_face']=0.
    elif variant=='half_face':owner['allocated_face']=50.
    elif variant=='weighted':
        run=owner['runs'][0];p=deepcopy(run['paths'][0]);run['paths'][0]['source_weight']=.25;p.update(path_id='second-path',source_weight=.75);run['paths'].append(p)
        for run in owner['runs']:
            for path in run['paths']:
                req=path['request']['request'];req['contract']['horizon_days']=26;req['reporting_ordinal']=req['contract']['as_of_ordinal']+26
    elif variant=='future':
        for run in owner['runs']:
            req=run['paths'][0]['request']['request'];c=req['contract'];c['as_of_ordinal']+=5;c['horizon_days']-=5;req['review']['acquisition_ordinal']+=5;c['rates']['promotional_expiry_ordinal']+=5
            for cycle in c['cycles']:cycle['statement_ordinal']+=5;cycle['due_ordinal']+=5
            for step in c['steps']:step['ordinal']+=5
            req['termination']['ordinal']+=5
    o=process(r);assert oracle.verify(r,o)['verified']


def test_compound_unpaid_gross_default_claim_reject_even_net_carry_zero(prepared):
    with pytest.raises(ArithmeticError,match='unretired'):oracle.reconstruct(bind_case(raw(),6),prepared)


@pytest.mark.parametrize('bad',['compound_unpaid','allowance','pricing','missing_run','legacy_tag','claim_recovery_missing'])
def test_process_raw_unadopted_or_unretired_sources_reject(bad):
    r=raw();o=r['product_owners'][0];p=o['runs'][0]['paths'][0];q=p['request']['request']
    if bad=='compound_unpaid':r=bind_case(r,6)
    elif bad=='allowance':o['initial_allowance_review']=dict(review=dict(id='r',revision='1'),amount=0.)
    elif bad=='pricing':q['contract']['identity']['measure']='pricing'
    elif bad=='missing_run':o['runs'].pop()
    elif bad=='legacy_tag':p['request']['kind']='card'
    else:
        r=bind_case(r,8)
        for run in r['product_owners'][0]['runs']:run['paths'][0]['request']['request']['interest_recoveries']=[]
    with pytest.raises(ArithmeticError):process(r)


def test_cooperative_cancellation_during_comparison(result):
    checks=0
    def stop():
        nonlocal checks
        checks+=1
        return checks>25000
    with pytest.raises(InterruptedError):oracle.verify(raw(),result,check=stop)


def mixed_installment():
    r=raw()
    for run in r['product_owners'][0]['runs']:
        q=run['paths'][0]['request']['request'];q['contract']['horizon_days']=20;q['reporting_ordinal']=q['contract']['as_of_ordinal']+20
        for cycle in q['contract']['cycles']:cycle['due_ordinal']=q['contract']['as_of_ordinal']+20
    old=json.loads((ROOT/'examples/models/product-mixed-product-credit-1.json').read_text())
    owner=next(o for o in old['product_owners'] if o['runs'][0]['paths'][0]['request']['kind']=='installment')
    r['product_owners'].append(owner)
    r['combined_specification']['positions'].append(next(p for p in old['combined_specification']['positions'] if p['id']==owner['position_id']))
    return r


def test_joint_inventory_capacity_never_relaxed_to_admit_more_sources(prepared):
    with pytest.raises(ArithmeticError,match='joined escaped capacity'):oracle.reconstruct(mixed_installment(),prepared)


def test_process_joint_original_pool_installment_and_card2():
    with pytest.raises(ArithmeticError):process(mixed_installment())
