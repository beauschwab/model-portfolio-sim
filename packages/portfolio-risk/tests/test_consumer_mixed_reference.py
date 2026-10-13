"""Raw same-GL consumer mixed source oracle and fresh native publication parity."""
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess
import pytest
from portfolio_risk.analytics import consumer_mixed_reference as oracle

ROOT=Path(__file__).resolve().parents[3]


def raw():return json.loads((ROOT/'examples/models/product-mixed-consumer-credit-1.json').read_text())


@pytest.fixture(scope='module')
def prepared():
    # Rust generates only this upstream component fixture. Full raw source
    # economics are independently reconstructed again inside the mixed oracle.
    from portfolio_risk.analytics.model_contracts import prepare_product_units
    r=raw();p=prepare_product_units(r['performing']['candidate']['product'])
    identities=oracle.dated.candidate._unit_identities(r['performing']['candidate']['product'],p)
    return dict(schema='product-strategy-preparation-1',units=identities,preparation=p)


@pytest.fixture(scope='module')
def pure_result(prepared):return oracle.reconstruct(raw(),prepared)


def forbidden(*a,**k):raise AssertionError('native financial callback inside independent mixed verifier')


def test_independent_full_source_one_gl_hand_closing_and_reserved_clock(prepared,pure_result,monkeypatch):
    from portfolio_risk.core import quant_native,decision_native,model_native
    monkeypatch.setattr(quant_native,'term_call',forbidden);monkeypatch.setattr(model_native,'run',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    r=raw();o=deepcopy(pure_result);receipt=oracle.verify(r,o)
    assert receipt==dict(verified=True,mixed_consumer_credit_only=True,ordered_mixed_journal_recomputed=True,
        complete_cash_rights_basis_tail_verified=True,dynamic_credit_acceptance=False,empirical_validation=False)
    assert receipt==oracle.verify(json.loads(json.dumps(r)),json.loads(json.dumps(o)))
    start=r['performing']['backbook']['dated']['as_of_ordinal'];report=r['performing']['candidate']['product']['reporting_ordinals'][-1]
    assert o['preparation']['consumer_reservation']==dict(work=149184,retained_bytes=50899616,full_end_ordinal=start+1080,paths=2)
    assert o['admitted_work']==159704 and o['full_end_ordinal']==report==start+60
    assert o['consumer_sources'][0]['native']['reporting']['ordinal']==start+40
    u=r['performing']['candidate']['product']['units'][0];face=r['performing']['candidate']['allocation'][0]['face']
    expected=1000-face*u['quote_price_per_face_unit']+face*1.0445+101.-102.+60.-10.
    assert o['closing_statements'][0]['cash']==pytest.approx(expected,abs=1e-9)
    assert o['closing_statements'][0]['equity']==pytest.approx(expected,abs=1e-9)
    assert sum(j['value'] for j in o['journal'] if j['scenario']=='baseline' and j['gl_account']=='cash')==pytest.approx(expected,abs=1e-9)
    assert len([j for j in o['journal'] if j['event']=='opening' and j['gl_account']=='cash'])==2
    assert len(o['consumer_acquisition_lineage'])==2
    assert all(row['purchase_transaction_number'] is not None for row in o['consumer_acquisition_lineage'])
    assert all(row['allowance_transaction_number'] is None for row in o['consumer_acquisition_lineage'])


@pytest.mark.parametrize('target',['journal_order','journal_money','trial','closing','daily','exposure','ledger',
    'dated_rights','reporting_rights','event_lineage','candidate_acquisition','consumer_source','consumer_daily',
    'consumer_lineage','consumer_acquisition','reserve_work','reserve_bytes','reserve_paths_float',
    'reserve_end_float','report_float','full_float','owner_float','purchase_tx_float','core_clock','combined_clock',
    'producer_persisted','producer_dynamic','producer_capital','scope'])
def test_source_order_finance_lineage_and_strict_metadata_tamper_reject(pure_result,target):
    r=raw();o=deepcopy(pure_result)
    if target=='journal_order':o['journal'].reverse()
    elif target=='journal_money':o['journal'][0]['value']+=1.
    elif target=='trial':o['trial_balance'][0]['balance']+=1.
    elif target=='closing':o['closing_statements'][0]['cash']+=1.
    elif target=='daily':o['daily_balance'][0]['assets']+=1.
    elif target=='exposure':o['exposures'][0]['balance']+=1.
    elif target=='ledger':o['ledger'][0]['cash']+=1.
    elif target=='dated_rights':o['dated_rights'].pop()
    elif target=='reporting_rights':o['reporting_rights'].pop()
    elif target=='event_lineage':o['event_lineage'][0]['source_revision']='caller'
    elif target=='candidate_acquisition':o['acquisition_lineage'][0]['source_weight']=.5
    elif target=='consumer_source':o['consumer_sources'][0]['native']['recovery']['principal_only_loss']+=1.
    elif target=='consumer_daily':o['consumer_daily'][0]['unweighted_owned']['live'][0]['principal']+=1.
    elif target=='consumer_lineage':o['consumer_lineage'][0]['original_yield_anchor']['revision']='caller'
    elif target=='consumer_acquisition':o['consumer_acquisition_lineage'][0]['acquisition']['cost']+=1.
    elif target.startswith('reserve_'):
        key={'reserve_work':'work','reserve_bytes':'retained_bytes','reserve_paths_float':'paths','reserve_end_float':'full_end_ordinal'}[target]
        v=o['preparation']['consumer_reservation'][key];o['preparation']['consumer_reservation'][key]=float(v) if target.endswith('float') else v+1
    elif target=='report_float':o['reporting_end_ordinal']=float(o['reporting_end_ordinal'])
    elif target=='full_float':o['full_end_ordinal']=float(o['full_end_ordinal'])
    elif target=='owner_float':o['consumer_sources'][0]['owner_index']=0.
    elif target=='purchase_tx_float':
        row=o['consumer_acquisition_lineage'][0];row['purchase_transaction_number']=float(row['purchase_transaction_number'])
    elif target=='core_clock':o['consumer_sources'][0]['native']['raw_request']['reporting_end_ordinal']+=1
    elif target=='combined_clock':o['preparation']['performing_preparation']['combined_dated_request']['dated']['reporting_horizon_days']+=1
    elif target=='producer_persisted':o['independent_persisted_verification']=True
    elif target=='producer_dynamic':o['dynamic_mixed_acceptance']=True
    elif target=='producer_capital':o['interim_default_capital_adoption']=True
    else:o['scope']='caller empirical approval'
    with pytest.raises(ArithmeticError):oracle.verify(r,o)


@pytest.mark.parametrize('bad',['tail','review','double_owner','anchors','run','weight','source_clock',
    'regulatory','account_constraint','pcd','pricing','after_tail_claim','report_floating','unknown'])
def test_raw_incomplete_or_overlaid_owner_domain_fails_closed(prepared,bad):
    r=raw();owner=r['consumer_owners'][0];paths=[p for run in owner['runs'] for p in run['paths']]
    pos=next(p for p in r['combined_specification']['positions'] if p['id']==owner['position_id'])
    if bad=='tail':
        for p in paths:p['request']['chargeoffs']=[]
    elif bad=='review':
        for p in paths:p['request']['opening_allowance']=40.
    elif bad=='double_owner':pos['balance']=1.
    elif bad=='anchors':paths[1]['request']['acquisition']['cost']=101.
    elif bad=='run':paths[0]['request']['contract']['identity']['run_id']='bad'
    elif bad=='weight':paths[0]['source_weight']=.5
    elif bad=='source_clock':
        for p in paths:p['request']['reporting_end_ordinal']+=1
    elif bad=='regulatory':pos['risk_weight']=1.
    elif bad=='account_constraint':r['combined_specification']['accounts'][0]['cet1_floor']=.1
    elif bad=='pcd':paths[0]['request']['acquisition']['accounting_basis']='pcd'
    elif bad=='pricing':paths[0]['request']['contract']['identity']['measure']='pricing'
    elif bad=='after_tail_claim':paths[0]['request']['chargeoffs'][0]['recognition_ordinal']+=1080
    elif bad=='report_floating':paths[0]['request']['reporting_end_ordinal']=float(paths[0]['request']['reporting_end_ordinal'])
    else:r['caller_verified']=True
    with pytest.raises(ArithmeticError):oracle.reconstruct(r,prepared)


def test_zero_allocation_retains_unweighted_complete_tail_and_null_purchase_identity(prepared):
    r=raw();r['consumer_owners'][0]['allocated_face']=0.
    o=oracle.reconstruct(r,prepared)
    assert all(a['purchase_transaction_number'] is None for a in o['consumer_acquisition_lineage'])
    assert all(a['allowance_transaction_number'] is None for a in o['consumer_acquisition_lineage'])
    assert len(o['consumer_sources'])==2 and len(o['consumer_daily'])==122
    assert all(s['native']['closing']['claims'][0]['outstanding_principal']==0. for s in o['consumer_sources'])
    for run in r['consumer_owners'][0]['runs']:run['paths'][0]['request']['chargeoffs']=[]
    with pytest.raises(ArithmeticError):oracle.reconstruct(r,prepared)


def test_cancellation_and_aggregate_reservation_before_raw_credit_projection(prepared,monkeypatch):
    with pytest.raises(InterruptedError):oracle.reconstruct(raw(),prepared,lambda:True)
    r=raw();owner=r['consumer_owners'][0]
    for run in owner['runs']:
        original=run['paths'][0];run['paths']=[]
        for i in range(32):
            p=deepcopy(original);p['path_id']=p['request']['path_id']='path-'+str(i);p['source_weight']=1/32;run['paths'].append(p)
    monkeypatch.setattr(oracle.credit,'reconstruct',forbidden)
    with pytest.raises(ArithmeticError,match='reservation|capacity'):oracle.reconstruct(r,prepared)


def execute_native(r):
    path=Path(os.environ.get('CONSUMER_MIXED_TEST_BINARY',ROOT/'packages/portfolio-decision-native/target/release'/('portfolio-strategy.exe' if os.name=='nt' else 'portfolio-strategy')))
    assert path.is_file(),'fresh actual decision CLI required; no skip or harness fallback'
    command=dict(op='analyze_product_mixed_consumer_credit',schema=oracle.SCHEMA,execution_timeout_ms=30000,request=r)
    completed=subprocess.run([str(path)],input=json.dumps(command,allow_nan=False),text=True,capture_output=True,timeout=40)
    assert completed.returncode==0,(completed.stdout,completed.stderr)
    response=json.loads(completed.stdout);assert 'ok' in response,response;return response['ok']


@pytest.fixture(scope='module')
def native():return execute_native(raw())


def test_fresh_actual_same_sim_native_complete_publication_gate(native,monkeypatch):
    from portfolio_risk.core import quant_native,decision_native,model_native,product_mixed_native
    monkeypatch.setattr(quant_native,'term_call',forbidden);monkeypatch.setattr(model_native,'run',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden);monkeypatch.setattr(product_mixed_native,'run',forbidden)
    receipt=oracle.verify(raw(),native)
    assert receipt['mixed_consumer_credit_only'] and receipt['complete_cash_rights_basis_tail_verified']
    assert receipt==oracle.verify(json.loads(json.dumps(raw())),json.loads(json.dumps(native)))


@pytest.mark.parametrize('table',['journal','trial_balance','closing_statements','consumer_sources','consumer_lineage','consumer_acquisition_lineage','dated_rights'])
def test_fresh_native_complete_ordered_table_coverage_tamper(native,table):
    changed=deepcopy(native);changed[table].pop()
    with pytest.raises(ArithmeticError):oracle.verify(raw(),changed)


@pytest.mark.parametrize('variant',['same_day','zero_allocation','weighted','future_acquisition','reviewed_allowance'])
def test_fresh_actual_owner_variants_keep_once_cash_and_complete_unweighted_claims(variant):
    r=raw();owner=r['consumer_owners'][0];start=r['performing']['backbook']['dated']['as_of_ordinal']
    if variant=='zero_allocation':owner['allocated_face']=0.
    elif variant=='weighted':
        owner['allocated_face']=50.
        # Complete recovery/chargeoff is day 15. The supplied shorter sources
        # retain that entire tail and admit distinct paths within the joined cap.
        for run in owner['runs']:
            c=run['paths'][0]['request'];c['contract']['horizon_days']=16
            c['contract']['payment_dates']=[d for d in c['contract']['payment_dates'] if d<=start+16]
            c['reporting_end_ordinal']=start+16
        for run in owner['runs'][:1]:
            a=deepcopy(run['paths'][0]);a['source_weight']=.25
            b=deepcopy(a);b['path_id']=b['request']['path_id']='payoff-path';b['source_weight']=.75
            b['request']['contract']['steps'][0]['action']={'kind':'payoff'};b['request']['chargeoffs']=[]
            run['paths']=[a,b]
    elif variant=='reviewed_allowance':
        owner['initial_allowance_review']=dict(review=dict(id='acquired-initial-allowance',revision='r1'),amount=40.)
        for run in owner['runs']:run['paths'][0]['request']['opening_allowance']=40.
    else:
        for run in owner['runs']:
            c=run['paths'][0]['request']
            if variant=='same_day':
                c['contract']['product']['recovery_delay_days']=0;c['chargeoffs'][0]['recognition_ordinal']=c['contract']['steps'][0]['ordinal']
            else:
                shift=70
                c['contract']['as_of_ordinal']+=shift;c['contract']['product']['vintage_ordinal']+=shift
                c['contract']['payment_dates']=[d+shift for d in c['contract']['payment_dates']]
                for step in c['contract']['steps']:step['ordinal']+=shift
                c['acquisition']['purchase_ordinal']+=shift
                for review in c['chargeoffs']+c['allowance_reviews']:review['recognition_ordinal']+=shift
                c['reporting_end_ordinal']=c['acquisition']['purchase_ordinal']
    result=execute_native(r);receipt=oracle.verify(r,result)
    assert receipt['complete_cash_rights_basis_tail_verified'] and receipt['ordered_mixed_journal_recomputed']
    if variant=='zero_allocation':
        assert all(a['purchase_transaction_number'] is None for a in result['consumer_acquisition_lineage'])
        assert len(result['consumer_sources'])==2
    elif variant=='weighted':assert len(result['consumer_sources'])==3
    elif variant=='future_acquisition':
        assert result['full_end_ordinal']==start+110
        assert all(not d['acquired'] for d in result['consumer_daily'] if d['ordinal']<start+70)
        assert all(d['owned']['claims']==[] for d in result['consumer_sources'][0]['native']['daily'][:1])
    elif variant=='reviewed_allowance':assert all(a['allowance_transaction_number'] is not None for a in result['consumer_acquisition_lineage'])


@pytest.mark.parametrize('bad',['unpaid_tail','zero_unpaid_tail','zero_weight_unpaid_tail','joined_capacity','pricing','pcd','overlaid_principal','missing_run','missing_allowance_review'])
def test_fresh_actual_wrapper_rejects_unsupported_or_incomplete_raw_sources(bad,prepared):
    r=raw();owner=r['consumer_owners'][0]
    if bad in ('unpaid_tail','zero_unpaid_tail'):
        if bad=='zero_unpaid_tail':owner['allocated_face']=0.
        for run in owner['runs']:run['paths'][0]['request']['chargeoffs']=[]
    elif bad in ('zero_weight_unpaid_tail','joined_capacity'):
        for run in owner['runs'][:1]:
            a=deepcopy(run['paths'][0]);b=deepcopy(a)
            b['path_id']=b['request']['path_id']='second-path'
            if bad=='zero_weight_unpaid_tail':
                a['request']['contract']['horizon_days']=b['request']['contract']['horizon_days']=16
                a['request']['contract']['payment_dates']=b['request']['contract']['payment_dates']=[a['request']['contract']['as_of_ordinal']+10]
                a['request']['reporting_end_ordinal']=b['request']['reporting_end_ordinal']=a['request']['contract']['as_of_ordinal']+16
                b['source_weight']=0.;b['request']['chargeoffs']=[]
            else:a['source_weight']=b['source_weight']=.5
            run['paths']=[a,b]
    elif bad=='pricing':owner['runs'][0]['paths'][0]['request']['contract']['identity']['measure']='pricing'
    elif bad=='pcd':owner['runs'][0]['paths'][0]['request']['acquisition']['accounting_basis']='pcd'
    elif bad=='overlaid_principal':
        next(p for p in r['combined_specification']['positions'] if p['id']==owner['position_id'])['balance']=1.
    elif bad=='missing_run':owner['runs'].pop()
    else:
        for run in owner['runs']:run['paths'][0]['request']['opening_allowance']=40.
    with pytest.raises(ArithmeticError):oracle.reconstruct(r,prepared)
    path=Path(os.environ.get('CONSUMER_MIXED_TEST_BINARY',ROOT/'packages/portfolio-decision-native/target/release'/('portfolio-strategy.exe' if os.name=='nt' else 'portfolio-strategy')))
    assert path.is_file(),'fresh actual decision CLI required; no skip or harness fallback'
    command=dict(op='analyze_product_mixed_consumer_credit',schema=oracle.SCHEMA,execution_timeout_ms=30000,request=r)
    completed=subprocess.run([str(path)],input=json.dumps(command,allow_nan=False),text=True,capture_output=True,timeout=40)
    assert completed.returncode!=0 or 'error' in json.loads(completed.stdout)
