"""Bounded acquired-mortgage source publication; no journal/capital adoption."""
import os
from pathlib import Path
import pytest
from app import worker,persistence as db
from test_persistence import durable
from test_model_runs import queued

@pytest.fixture(autouse=True)
def selected_public_binary(monkeypatch):
    override=os.environ.get('MORTGAGE_CREDIT_TEST_BINARY')
    if override:
        from portfolio_risk.core import model_native
        path=Path(override).resolve()
        assert path.name in {'portfolio-lifecycle','portfolio-lifecycle.exe'}
        monkeypatch.setattr(model_native,'binary_path',lambda:path)


@pytest.mark.parametrize('contract',['contract','original_contract'])
def test_mortgage_unknown_fixed_rate_field_fails_in_native_before_publication(durable,monkeypatch,contract):
    from app import model_routes
    from portfolio_risk.analytics import mortgage_credit_reference
    raw=model_routes.example('mortgage-credit-accounting-1')
    raw['request'][contract]['rate']['annual_rate']=0.
    submitted=model_routes.submit(model_routes.ModelRunRequest(
        schema_name=raw['schema_name'],request=raw['request'],expected_revision=0))
    db.WORKER='model-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    job=db.REPO.claim(db.WORKER)
    def forbidden(*args,**kwargs):
        pytest.fail('invalid native raw input reached independent publication verification')
    monkeypatch.setattr(mortgage_credit_reference,'verify',forbidden)
    worker.execute(job);row=db.REPO.job(submitted.id)
    assert row['status']!='done' and row['result'] is None
    assert 'unknown field' in row['detail']

@pytest.mark.parametrize('target',[
    'original_cash','current_cash','source_hash','original_anchor','carry','reward',
    'disposal','date','generator_hash','generator_bytes','summary_flag','receipt_flag',
    'raw_yield_epsilon','original_terms_epsilon','source_terms_epsilon'])
def test_persisted_mortgage_sources_and_original_anchor_cannot_be_forged(durable,monkeypatch,target):
    from app.model_tables import format_sources,restore_sources
    jid,job=queued('mortgage-credit-accounting-1')
    original=db.CODEC.dump
    def corrupt(value):
        if isinstance(value,dict) and 'model_result' in value:
            if target=='summary_flag':value['execution']['summary']['mixed_ledger_adoption']=0
            elif target=='receipt_flag':value['execution']['source_verification']['capital_ftp_eligible']=0
            else:
                execution=value['execution']
                source=restore_sources(value['model_result'],execution['table_layouts'])
                if target=='original_cash':source['original_source']['native_projection']['cashflows'][0]['interest']+=1.
                elif target=='current_cash':source['source']['native_projection']['cashflows'][0]['principal']+=1.
                elif target=='source_hash':source['original_source_raw_hash']='f'*64
                elif target=='original_anchor':source['raw_request']['acquisition']['original_yield_anchor']['revision']='forged'
                elif target=='carry':source['intervals'][0]['opening']['gross_carrying'][0]+=1.
                elif target=='reward':source['total_earned_eir']+=1.
                elif target=='disposal':source['default_closeouts'][0]['interest_economic_disposal']+=1.
                elif target=='date':source['full_end_ordinal']+=1
                elif target=='generator_hash':source['source']['intervals'][0]['financial_source_sha256']='f'*64
                elif target=='raw_yield_epsilon':source['raw_request']['acquisition']['original_annual_effective_rate']+=1e-10
                elif target=='original_terms_epsilon':source['raw_request']['original_contract']['original_payment']+=1e-10
                elif target=='source_terms_epsilon':
                    from portfolio_risk.analytics import mortgage_credit_reference
                    source['original_source']['native_projection']['original_principal']+=1e-10
                    source['original_source_receipt_hash']=mortgage_credit_reference.source_hash(source['original_source'])
                else:source['admission']['generator_code_hash_bytes']+=1
                value['model_result'],execution['table_layouts']=format_sources(source)
        return original(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt)
    worker.execute(job)
    row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None,row['detail']

@pytest.mark.parametrize('phase',[1,2])
def test_mortgage_independent_source_replay_cancellation_prevents_manifest(durable,monkeypatch,phase):
    from portfolio_risk.analytics import mortgage_credit_reference as reference
    jid,job=queued('mortgage-credit-accounting-1');original=reference.verify;calls=[0]
    def interrupted(raw,result,check=lambda:False):
        calls[0]+=1;steps=[0]
        def checking():
            steps[0]+=1
            if calls[0]==phase and steps[0]==16:worker.STOP.set()
            return check()
        return original(raw,result,check=checking)
    monkeypatch.setattr(reference,'verify',interrupted)
    worker.execute(job);row=db.REPO.job(jid)
    assert calls[0]==phase and row['status']!='done' and row['result'] is None
    assert 'cancelled' in row['detail']


@pytest.mark.parametrize('source_kind',['ledger','credit_ledger','cashflows'])
def test_analytical_mortgage_source_does_not_imply_journal_capital_or_ftp_adoption(durable,source_kind):
    from app import treasury_store
    jid,job=queued('mortgage-credit-accounting-1');worker.execute(job)
    row=db.REPO.job(jid)
    assert row['status']=='done',row['detail']
    result=db.CODEC.load(db.result_ref(jid));receipt=result['execution']['source_verification']
    assert receipt['analytical_mortgage_source_only'] is True
    assert receipt['mixed_ledger_adoption'] is False and receipt['capital_ftp_eligible'] is False
    with pytest.raises(ValueError):treasury_store.template(jid,source_kind)
