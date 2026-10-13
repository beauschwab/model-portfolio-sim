"""Default native mixed-mortgage durable publication; no test binary overrides."""
import io
import json
import pytest
from app import main,worker,persistence as db,maintenance,reporting,store
from app.schemas import ModeledBalanceStressRequest
from app.artifacts import ObjectReference
from test_persistence import durable
SCHEMA='mortgage-mixed-accounting-1'


def queued(raw=None):
    specification=main.modeled_balance_stress_example(SCHEMA)['specification'] if raw is None else raw
    submitted=main.modeled_balance_stress(ModeledBalanceStressRequest(expected_revision=0,specification=specification))
    db.WORKER='mortgage-mixed-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return submitted['id'],db.REPO.claim(db.WORKER),specification


def test_actual_native_publication_reopens_source_completion_and_full_gl(durable,monkeypatch):
    from portfolio_risk.analytics import model_contracts,balance_stream,balance_stress,model_ledger
    from portfolio_risk.core import model_native,quant_native
    from portfolio_risk.analytics import consumer_credit_reference,card_credit_reference,card_credit_v2_reference
    def forbidden(*args,**kwargs):raise AssertionError('unrelated or Python financial execution callback')
    for name in ('project_mortgage_state','account_mortgage_credit','analyze_mortgage_graph','account_card_credit','account_card_credit_v2'):
        monkeypatch.setattr(model_contracts,name,forbidden)
    monkeypatch.setattr(model_native,'run',forbidden);monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(balance_stream,'_python',forbidden);monkeypatch.setattr(balance_stress,'_simulate',forbidden)
    for module in (consumer_credit_reference,card_credit_reference,card_credit_v2_reference):monkeypatch.setattr(module,'verify',forbidden)
    monkeypatch.setattr(model_ledger,'validate_spec',forbidden)
    assert model_ledger.binary_path().parent.name=='release'
    jid,job,raw=queued();worker.execute(job);row=db.REPO.job(jid)
    assert row['status']=='done',row['detail']
    result=db.CODEC.load(db.result_ref(jid));execution=result['execution'];gate=execution['validation']
    for key in ('native_mortgage_source_recomputed','single_daily_gl_verified','pending_memo_phase_verified','complete_unweighted_tail_verified','ordered_mixed_journal_recomputed'):assert gate[key] is True
    for key in ('production_adoption','mixed_ledger_adoption','accounting_standard_approval','capital_ftp_eligible','closing_capital_eligible'):assert gate[key] is False
    assert execution['backend']=='rust' and execution['model_version']==SCHEMA
    assert json.loads(db.CODEC.objects.get(execution['producer_completion']))=={'complete':True,'protocol':3}
    assert json.loads(db.CODEC.objects.get(execution['input']))['request']==raw
    source=json.loads(db.CODEC.objects.get(execution['modeled_source']))
    assert source['source_receipt']['sources'][0]['native']['source']['raw_request']['acquisition']['original_yield_anchor']==raw['sources'][0]['request']['source']['acquisition']['original_yield_anchor']
    for name in ('journal','trial_balance','mortgage_daily','mortgage_lineage','mortgage_closing','closing_statements'):
        assert reporting.table(jid,'/'+name,offset=0,limit=2)['rows']
    reachable=maintenance.reachable(db.REPO,db.CODEC.objects)
    assert execution['producer_completion']['key'] in reachable and execution['modeled_source']['key'] in reachable
    before=store.job_result(jid);db.stop();db.start(durable);assert store.job_result(jid)==before


@pytest.mark.parametrize('target',['input_type','input_sign_zero','source_yield_epsilon','source_origin',
    'completion_bool','completion_float','receipt_false','receipt_true','lineage_weight_epsilon',
    'balanced_journal','borrower_memo','drop_table','expired_deadline','extended_deadline',
    'cancel_after_upload','lease_after_upload','source_after_upload'])
def test_uploaded_source_receipt_completion_and_partitions_recheck_before_commit(durable,monkeypatch,target):
    import polars as pl
    from portfolio_risk.analytics import balance_stream
    jid,job,_=queued();dump=db.CODEC.dump;load=db.CODEC.load
    def replace(value):
        ref=dump(value)
        if not isinstance(value,dict) or value.get('execution',{}).get('model_version')!=SCHEMA:return ref
        changed=load(ref);execution=changed['execution']
        if target.startswith('input_') or target.startswith('source_') and target!='source_after_upload' or target.startswith('completion_'):
            key='input' if target.startswith('input_') else 'producer_completion' if target.startswith('completion_') else 'modeled_source'
            data=json.loads(db.CODEC.objects.get(execution[key]))
            if target=='input_type':data['request']['owners'][0]['allocated_face']=int(data['request']['owners'][0]['allocated_face'])
            elif target=='input_sign_zero':data['request']['sources'][0]['request']['source']['contract']['opening_investor_principal'][1]=-0.
            elif target=='source_yield_epsilon':data['source_receipt']['sources'][0]['native']['source']['raw_request']['acquisition']['original_annual_effective_rate']+=1e-12
            elif target=='source_origin':data['source_receipt']['sources'][0]['source_id']='different-source'
            elif target=='completion_bool':data['complete']=1
            else:data['protocol']=3.0
            execution[key]=ObjectReference(db.CODEC.objects.put(json.dumps(data,allow_nan=False).encode(),'bin'))
        elif target in ('lineage_weight_epsilon','balanced_journal','borrower_memo'):
            name={'lineage_weight_epsilon':'mortgage_lineage','balanced_journal':'journal','borrower_memo':'mortgage_daily'}[target]
            part=changed[name]['parts'][0];frame=pl.read_parquet(io.BytesIO(db.CODEC.objects.get(part['ref'])));rows=frame.to_dicts()
            if target=='lineage_weight_epsilon':rows[0]['source_weight']+=1e-15
            elif target=='balanced_journal':
                for line in rows[:2]:
                    line['debit']+=1. if line['debit'] else 0.;line['credit']+=1. if line['credit'] else 0.
            else:
                memo=json.loads(rows[0]['unweighted_book_json']);memo['borrower_principal'][0]=0.;rows[0]['unweighted_book_json']=json.dumps(memo)
            stream=io.BytesIO();pl.DataFrame(rows,schema=frame.schema).write_parquet(stream);part['ref']=db.CODEC.objects.put(stream.getvalue(),'parquet')
        elif target=='receipt_false':execution['validation']['accounting_standard_approval']=0
        elif target=='receipt_true':execution['validation']['native_mortgage_source_recomputed']=1
        elif target=='drop_table':changed.pop('mortgage_lineage')
        elif target=='expired_deadline':execution['publication_deadline_unix']=0.
        elif target=='extended_deadline':execution['publication_deadline_unix']+=3600.
        elif target=='cancel_after_upload':db.REPO.cancel(jid)
        elif target=='lease_after_upload':db.REPO.release(db.WORKER)
        else:monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
        return dump(changed)
    monkeypatch.setattr(db.CODEC,'dump',replace)
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None,row['detail']
    assert 'KeyError' not in str(row['detail'])


@pytest.mark.parametrize('target',['fixed_extra','gross_backbook'])
def test_invalid_native_raw_never_reaches_independent_verification(durable,monkeypatch,target):
    from portfolio_risk.analytics import mortgage_mixed_reference
    raw=main.modeled_balance_stress_example(SCHEMA)['specification']
    if target=='fixed_extra':raw['sources'][0]['request']['source']['original_contract']['rate']['annual_rate']=0.
    else:raw['dated']['dated']['sources'][0]['schedule']['events'][0]['components']['cash_interest']=10001.
    jid,job,_=queued(raw)
    def forbidden(*args,**kwargs):raise AssertionError('invalid native raw reached verification')
    monkeypatch.setattr(mortgage_mixed_reference,'verify_preparation',forbidden)
    worker.execute(job);row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None
    assert 'invalid native raw reached verification' not in str(row['detail'])


@pytest.mark.parametrize('phase',['upload','replay_cancel','final_lease'])
def test_durable_failure_has_no_result_or_manifest_adoption(durable,monkeypatch,phase):
    from portfolio_risk.analytics import mortgage_mixed_reference as reference
    jid,job,_=queued()
    if phase=='upload':
        put=db.CODEC.objects.put
        def broken(data,suffix):
            if suffix=='parquet':raise OSError('injected mortgage upload failure')
            return put(data,suffix)
        monkeypatch.setattr(db.CODEC.objects,'put',broken)
    elif phase=='replay_cancel':
        verify=reference.verify_stream
        def interrupted(raw,control,frames,check=lambda:False):
            ticks=[0]
            def checking():
                ticks[0]+=1
                if ticks[0]==12:worker.STOP.set()
                return check()
            return verify(raw,control,frames,checking)
        monkeypatch.setattr(reference,'verify_stream',interrupted)
    else:
        from app import model_jobs
        verify=model_jobs.verify_modeled_persisted
        def lost(*args,**kwargs):
            result=verify(*args,**kwargs);db.REPO.release(db.WORKER);return result
        monkeypatch.setattr(model_jobs,'verify_modeled_persisted',lost)
    worker.execute(job);row=db.REPO.job(jid);assert row['status']!='done' and row['result'] is None


@pytest.mark.parametrize('kind',['ledger','credit_ledger','cashflows'])
def test_verified_component_does_not_grant_capital_or_ftp(durable,kind):
    from app import treasury_store
    jid,job,_=queued();worker.execute(job);row=db.REPO.job(jid)
    assert row['status']=='done',row['detail']
    with pytest.raises(ValueError):treasury_store.template(jid,kind)
