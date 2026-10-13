"""Independent tagged product sources and actual pooled native journal gates."""
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess
import pytest
from portfolio_risk.analytics import product_event_reference as oracle

ROOT=Path(__file__).resolve().parents[3]


def raw():
    return json.loads((ROOT/'examples/models/product-mixed-product-credit-1.json').read_text())


@pytest.fixture(scope='module')
def prepared():
    # This supplies only the upstream native receipt. Verification independently
    # reconstructs its complete raw economics before using any coefficients.
    from portfolio_risk.analytics.model_contracts import prepare_product_units
    r=raw();p=prepare_product_units(r['performing']['candidate']['product'])
    identities=oracle.dated.candidate._unit_identities(r['performing']['candidate']['product'],p)
    return dict(schema='product-strategy-preparation-1',units=identities,preparation=p)


@pytest.fixture(scope='module')
def pure_result(prepared):return oracle.reconstruct(raw(),prepared)


def forbidden(*a,**k):raise AssertionError('native callback inside independent mixed product reconstruction')


def test_raw_economics_to_one_pool_full_tail_and_source_identity(prepared,pure_result,monkeypatch):
    from portfolio_risk.core import model_native,quant_native,decision_native
    monkeypatch.setattr(model_native,'run',forbidden);monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    r=raw();o=deepcopy(pure_result);before_raw=deepcopy(r);before_result=deepcopy(o)
    receipt=oracle.verify(r,o)
    assert receipt==dict(verified=True,mixed_product_credit_only=True,ordered_mixed_journal_recomputed=True,
        complete_cash_rights_basis_tail_verified=True,dynamic_credit_acceptance=False,empirical_validation=False)
    assert r==before_raw and o==before_result
    assert receipt==oracle.verify(json.loads(json.dumps(r)),json.loads(json.dumps(o)))
    product=r['performing']['candidate']['product'];face=r['performing']['candidate']['allocation'][0]['face']
    expected=1000.-face*product['units'][0]['quote_price_per_face_unit']+face*1.0445+101.
    for owner in r['product_owners']:
        request=owner['runs'][0]['paths'][0]['request']['request']
        expected+=owner['allocated_face']/owner['unit_face']*(-request['acquisition']['cost']+50.)
    assert all(c['cash']==pytest.approx(expected,abs=1e-9) and c['equity']==pytest.approx(expected,abs=1e-9) for c in o['closing_statements'])
    assert len([j for j in o['journal'] if j['event']=='opening' and j['gl_account']=='cash'])==2
    assert {s['native']['kind'] for s in o['product_sources']}=={'installment','card'}
    assert len(o['product_acquisition_lineage'])==4
    assert all(s['native']['result']['closing']['claims'][0]['outstanding_principal']==0. for s in o['product_sources'])
    start=r['performing']['backbook']['dated']['as_of_ordinal']
    assert o['full_end_ordinal']==start+60 and o['preparation']['product_reservation']['full_end_ordinal']==start+1080
    assert o['admitted_work']>o['preparation']['product_reservation']['work']


@pytest.mark.parametrize('target',['journal_order','journal_money','trial','closing','daily','exposure','ledger',
    'dated_rights','reporting_rights','event_lineage','candidate_acquisition','product_source','product_kind',
    'product_daily','statement','commitment','product_lineage','source_indices_float','product_acquisition',
    'reserve_work','reserve_bytes','reserve_paths_float','reserve_end_float','report_float','full_float','owner_float',
    'reporting_ordinals_float','recovery_delay_float',
    'purchase_tx_float','core_clock','combined_clock','producer_persisted','producer_dynamic','producer_capital','scope'])
def test_complete_finance_identity_metadata_and_source_tamper_reject(pure_result,target):
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
    elif target=='product_source':o['product_sources'][0]['native']['result']['recovery']['principal_only_loss']+=1.
    elif target=='product_kind':o['product_sources'][0]['native']['kind']='card'
    elif target=='product_daily':o['product_daily'][0]['unweighted_owned']['live'][0]['principal']+=1.
    elif target in ('statement','commitment'):
        row=next(x for x in o['product_daily'] if x['product_receipts'] and x['product_receipts']['kind']=='card')
        if target=='statement':row['product_receipts']['statement_lineage'].append({'ordinal':o['full_end_ordinal']})
        else:row['product_receipts']['commitment_source']['funded_contingent_draw']+=1.
    elif target=='product_lineage':o['product_lineage'][0]['original_yield_anchor']['revision']='caller'
    elif target=='source_indices_float':
        row=next(x for x in o['product_lineage'] if 'source_recognition_indices' in x['native_lineage'] and x['native_lineage']['source_recognition_indices'])
        row['native_lineage']['source_recognition_indices'][0]=float(row['native_lineage']['source_recognition_indices'][0])
    elif target=='product_acquisition':o['product_acquisition_lineage'][0]['acquisition']['cost']+=1.
    elif target.startswith('reserve_'):
        key={'reserve_work':'work','reserve_bytes':'retained_bytes','reserve_paths_float':'paths','reserve_end_float':'full_end_ordinal'}[target]
        v=o['preparation']['product_reservation'][key];o['preparation']['product_reservation'][key]=float(v) if target.endswith('float') else v+1
    elif target=='report_float':o['reporting_end_ordinal']=float(o['reporting_end_ordinal'])
    elif target=='full_float':o['full_end_ordinal']=float(o['full_end_ordinal'])
    elif target=='owner_float':o['product_sources'][0]['owner_index']=0.
    elif target=='reporting_ordinals_float':
        values=o['preparation']['raw_request']['performing']['candidate']['product']['reporting_ordinals'];values[0]=float(values[0])
    elif target=='recovery_delay_float':
        c=o['product_sources'][-1]['native']['result']['raw_request']['contract'];c['recovery_delay_days']=float(c['recovery_delay_days'])
    elif target=='purchase_tx_float':
        row=o['product_acquisition_lineage'][0];row['purchase_transaction_number']=float(row['purchase_transaction_number'])
    elif target=='core_clock':o['product_sources'][0]['native']['result']['raw_request']['reporting_end_ordinal']+=1
    elif target=='combined_clock':o['preparation']['performing_preparation']['combined_dated_request']['dated']['reporting_horizon_days']+=1
    elif target=='producer_persisted':o['independent_persisted_verification']=True
    elif target=='producer_dynamic':o['dynamic_mixed_acceptance']=True
    elif target=='producer_capital':o['interim_default_capital_adoption']=True
    else:o['scope']='caller empirical approval'
    with pytest.raises(ArithmeticError):oracle.verify(r,o)


@pytest.mark.parametrize('bad',['tail','zero_tail','zero_weight_tail','review','double_owner','anchors','run','weight',
    'source_clock','regulatory','account_constraint','pcd','pricing','grace','draw','fee','unknown_kind','unknown','mixed_terms'])
def test_raw_unsupported_or_incomplete_sources_reject_before_publication(prepared,bad):
    r=raw();owner=r['product_owners'][-1];paths=[p for run in owner['runs'] for p in run['paths']]
    p=paths[0]['request']['request'];pos=next(p for p in r['combined_specification']['positions'] if p['id']==owner['position_id'])
    if bad in ('tail','zero_tail','zero_weight_tail'):
        for path in paths:path['request']['request']['chargeoffs']=[]
        if bad=='zero_tail':owner['allocated_face']=0.
        if bad=='zero_weight_tail':
            for run in owner['runs']:
                other=deepcopy(run['paths'][0]);other['path_id']=other['request']['request']['path_id']='covered'
                other['source_weight']=1.;other['request']['request']['chargeoffs']=deepcopy(raw()['product_owners'][-1]['runs'][0]['paths'][0]['request']['request']['chargeoffs'])
                run['paths'][0]['source_weight']=0.;run['paths'].append(other)
    elif bad=='review':owner['initial_allowance_review']=None
    elif bad=='double_owner':pos['balance']=1.
    elif bad=='anchors':paths[-1]['request']['request']['acquisition']['cost']+=1.
    elif bad=='run':p['contract']['identity']['run_id']='caller'
    elif bad=='weight':paths[0]['source_weight']=.5
    elif bad=='source_clock':p['reporting_end_ordinal']+=1
    elif bad=='regulatory':pos['risk_weight']=1.
    elif bad=='account_constraint':r['combined_specification']['accounts'][0]['cet1_floor']=.1
    elif bad=='pcd':p['acquisition']['accounting_basis']='pcd'
    elif bad=='pricing':p['contract']['identity']['measure']='pricing'
    elif bad=='grace':p['contract']['initial_purchase_grace']=True
    elif bad=='draw':p['contract']['steps'][0]['action']={'kind':'purchase','amount':1.}
    elif bad=='fee':p['contract']['opening_fees'][0]=1.
    elif bad=='unknown_kind':paths[0]['request']['kind']='mortgage'
    elif bad=='unknown':r['auto_complete_claims']=True
    else:paths[-1]['request']['request']['contract']['rates']['purchase']+=.01
    with pytest.raises(ArithmeticError):oracle.reconstruct(r,prepared)


def test_cancellation_and_joined_escaped_admission_precede_product_projection(prepared,monkeypatch):
    with pytest.raises(InterruptedError):oracle.reconstruct(raw(),prepared,lambda:True)
    monkeypatch.setattr(oracle.credit,'reconstruct',forbidden);monkeypatch.setattr(oracle.card,'reconstruct',forbidden)
    r=raw()
    for owner in r['product_owners']:
        for run in owner['runs']:
            for path in run['paths']:
                req=path['request']['request'];req['contract']['horizon_days']=40;req['reporting_end_ordinal']=req['contract']['as_of_ordinal']+40
    with pytest.raises(ArithmeticError,match='capacity|reservation'):oracle.reconstruct(r,prepared)


def execute_native(r):
    binary=Path(os.environ.get('PRODUCT_EVENT_TEST_BINARY',ROOT/'packages/portfolio-decision-native/target/release'/('portfolio-strategy.exe' if os.name=='nt' else 'portfolio-strategy')))
    assert binary.is_file() and binary.name in ('portfolio-strategy.exe','portfolio-strategy'),'fresh actual Decision CLI required; no fallback or skip'
    envelope=dict(op='analyze_product_mixed_product_credit',schema=oracle.SCHEMA,execution_timeout_ms=30000,request=r)
    p=subprocess.run([str(binary)],input=json.dumps(envelope,allow_nan=False),capture_output=True,text=True,timeout=35)
    response=json.loads(p.stdout) if p.stdout.strip() else {'error':p.stderr}
    return response


def variant(name):
    r=raw();owner=r['product_owners'][-1];start=r['performing']['backbook']['dated']['as_of_ordinal']
    if name=='half_face':owner['allocated_face']=50.
    elif name=='weighted':
        # Keep original monetary cap; three distinct card paths, no installment
        # inventory, provide actual unequal path timing and weight coverage.
        removed=r['product_owners'].pop(0)['position_id']
        r['combined_specification']['positions']=[p for p in r['combined_specification']['positions'] if p['id']!=removed]
        owner['allocated_face']=50.;run=owner['runs'][0]
        a=deepcopy(run['paths'][0]);a['source_weight']=.25
        b=deepcopy(a);b['path_id']=b['request']['request']['path_id']='late-card-default';b['source_weight']=.75
        req=b['request']['request'];req['contract']['steps'][0]['ordinal']+=1;req['chargeoffs'][0]['recognition_ordinal']+=1
        run['paths']=[a,b]
    elif name=='zero_allocation':owner['allocated_face']=0.
    elif name in ('closure_tail','positive_yield_closure','negative_yield_closure','rejected_par_tail'):
        owner['initial_allowance_review']=None
        for run in owner['runs']:
            c=run['paths'][0]['request']['request'];contract=c['contract'];c['opening_allowance']=0.;c['chargeoffs']=[]
            c['acquisition']['cost']=101.
            contract['cycles']=[dict(statement_ordinal=start+10,due_ordinal=start+11)]
            contract['steps']=[dict(ordinal=start+10,action=dict(kind='payment',amount=100.)),
                dict(ordinal=start+11,action=dict(kind='payment',amount=1.)),dict(ordinal=start+11,action=dict(kind='set_limit',amount=0.))]
            c['termination']=dict(kind='reviewed_zero_rights_closure',ordinal=start+11,review=dict(id='reviewed-full-close',revision='r1'))
            if name=='positive_yield_closure':
                c['acquisition']['cost']=103.;c['acquisition']['original_annual_effective_rate']=.02
            elif name=='negative_yield_closure':
                c['acquisition']['cost']=90.;c['acquisition']['original_annual_effective_rate']=-.01
                contract['opening_principal']=[0.,0.,100.];contract['cycles']=[]
                contract['steps']=[dict(ordinal=start+1,action=dict(kind='payment',amount=100.)),dict(ordinal=start+1,action=dict(kind='set_limit',amount=0.))]
                c['termination']['ordinal']=start+1
            elif name=='rejected_par_tail':c['acquisition']['cost']=100.
    elif name=='future_purchase':
        for run in owner['runs']:
            c=run['paths'][0]['request']['request'];contract=c['contract'];contract['as_of_ordinal']+=5
            contract['rates']['promotional_expiry_ordinal']+=5
            for cycle in contract['cycles']:
                cycle['statement_ordinal']+=5;cycle['due_ordinal']+=5
            for step in contract['steps']:step['ordinal']+=5
            c['acquisition']['purchase_ordinal']+=5;c['reporting_end_ordinal']+=5
            for review in c['chargeoffs']+c['allowance_reviews']:review['recognition_ordinal']+=5
    elif name=='same_day_recovery':
        for run in owner['runs']:
            c=run['paths'][0]['request']['request'];c['contract']['recovery_delay_days']=0
            c['chargeoffs'][0]['recognition_ordinal']=c['contract']['steps'][0]['ordinal']
    elif name=='net_zero_recovery':
        for run in owner['runs']:run['paths'][0]['request']['request']['contract']['collection_cost']=60.
    else:raise AssertionError(name)
    return r


VARIANTS=['half_face','weighted','zero_allocation','closure_tail','positive_yield_closure',
    'negative_yield_closure','future_purchase','same_day_recovery','net_zero_recovery']


@pytest.mark.parametrize('name',VARIANTS)
def test_independent_actual_money_weights_rights_and_full_tail_variants(prepared,name):
    r=variant(name);o=oracle.reconstruct(r,prepared);oracle.verify(r,o)
    assert all(c['cash']==pytest.approx(c['equity'],abs=1e-9) for c in o['closing_statements'])
    if name=='zero_allocation':
        rows=[p for p in o['product_acquisition_lineage'] if p['position_id']==r['product_owners'][-1]['position_id']]
        assert all(p['purchase_transaction_number'] is None and p['allowance_transaction_number'] is None for p in rows)
        assert any(s['native']['result']['acquisition_opening']['live'][0]['principal']==100. for s in o['product_sources'] if s['native']['kind']=='card')
    elif name=='weighted':
        assert len(o['product_sources'])==3
        assert {p['source_weight'] for p in o['product_daily']}=={.25,.75,1.}
    elif name=='closure_tail':
        source=next(s['native']['result'] for s in o['product_sources'] if s['native']['kind']=='card')
        row=source['daily'][10]
        assert row['owned']['live'][0]['principal']==0. and row['owned']['live'][0]['accrued_interest']==pytest.approx(1.)
        assert any(x['product_receipts']['statement_lineage'] for x in o['product_daily'] if x['product_receipts'] and x['product_receipts']['kind']=='card')
        end=source['raw_request']['contract']['as_of_ordinal']+14
        assert all(not x['product_receipts']['statement_lineage'] for x in o['product_daily'] if x['ordinal']>end and x['product_receipts'] and x['product_receipts']['kind']=='card')
    elif name=='future_purchase':
        assert all(not x['acquired'] and x['product_receipts'] is None for x in o['product_daily'] if x['ordinal']<r['product_owners'][-1]['runs'][0]['paths'][0]['request']['request']['acquisition']['purchase_ordinal'] and x['position_id']==r['product_owners'][-1]['position_id'])


def test_unchanged_public_card_negative_principal_basis_guard(prepared):
    with pytest.raises(ArithmeticError):oracle.reconstruct(variant('rejected_par_tail'),prepared)


def test_installment_only_new_wire_preserves_old_consumer_financial_contract(prepared):
    from portfolio_risk.analytics import consumer_mixed_reference as old
    original=json.loads((ROOT/'examples/models/product-mixed-consumer-credit-1.json').read_text())
    legacy=old.reconstruct(original,prepared);tagged=deepcopy(original)
    tagged['schema']=oracle.SCHEMA;tagged['product_owners']=tagged.pop('consumer_owners')
    for owner in tagged['product_owners']:
        for run in owner['runs']:
            for path in run['paths']:path['request']=dict(kind='installment',request=path['request'])
    new=oracle.reconstruct(tagged,prepared)
    for name in old.TABLES:
        if name.startswith('consumer_'):continue
        oracle._check_equal(new[name],legacy[name],'unchanged-consumer/'+name)
    for native,previous in zip(new['product_sources'],legacy['consumer_sources'],strict=True):
        assert native['native']['kind']=='installment'
        oracle._check_equal(native['native']['result'],previous['native'],'unchanged-consumer/whole-source')
    assert old.verify(original,legacy)['mixed_consumer_credit_only']


def test_fresh_native_complete_same_sim_publication_parity(monkeypatch):
    r=raw();response=execute_native(r);assert 'ok' in response,response
    from portfolio_risk.core import model_native,quant_native,decision_native
    monkeypatch.setattr(model_native,'run',forbidden);monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    assert oracle.verify(r,response['ok'])['ordered_mixed_journal_recomputed']


@pytest.mark.parametrize('name',VARIANTS)
def test_fresh_native_complete_raw_source_variants(name):
    r=variant(name);response=execute_native(r);assert 'ok' in response,response
    assert oracle.verify(r,response['ok'])['complete_cash_rights_basis_tail_verified']


@pytest.mark.parametrize('bad',['unpaid','zero_unpaid','par_tail','pricing','grace','overlay','capacity'])
def test_fresh_native_strict_rejections(bad):
    r=variant('rejected_par_tail') if bad=='par_tail' else raw();owner=r['product_owners'][-1]
    if bad in ('unpaid','zero_unpaid'):
        for run in owner['runs']:
            for path in run['paths']:path['request']['request']['chargeoffs']=[]
        if bad=='zero_unpaid':owner['allocated_face']=0.
    elif bad=='pricing':owner['runs'][0]['paths'][0]['request']['request']['contract']['identity']['measure']='pricing'
    elif bad=='grace':owner['runs'][0]['paths'][0]['request']['request']['contract']['initial_purchase_grace']=True
    elif bad=='overlay':
        next(p for p in r['combined_specification']['positions'] if p['id']==owner['position_id'])['annual_pd']=.1
    elif bad=='capacity':
        for o in r['product_owners']:
            for run in o['runs']:
                for path in run['paths']:
                    req=path['request']['request'];req['contract']['horizon_days']=40;req['reporting_end_ordinal']=req['contract']['as_of_ordinal']+40
    response=execute_native(r);assert 'error' in response and 'ok' not in response,response
