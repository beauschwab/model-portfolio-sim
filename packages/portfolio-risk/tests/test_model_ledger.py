"""Native source preparation with independent economic and saved journal gates."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_ledger as engine,consumer_schedule_reference as reference
from portfolio_risk.analytics.balance_stream import PartitionWriter
from portfolio_risk.analytics.ledger_native import validate_spec
ROOT=Path(__file__).resolve().parents[3]
def example(schema='graph-dated-ledger-1'):
    return json.loads((ROOT/'examples/models'/f'{schema}.json').read_text())
@pytest.fixture(scope='module')
def saved(tmp_path_factory):
    raw=example();path=engine.run_model_ledger(raw,tmp_path_factory.mktemp('modeled'),partition_rows=7)
    manifest=json.loads(path.read_text());source=json.loads((path.parent/manifest['modeled_source']['path']).read_text())
    return raw,source,manifest
def test_full_native_capture_recomputes_raw_physical_origins_and_journal(saved):
    raw,source,manifest=saved
    result=reference.verify_preparation(raw,source['prepared_request'],source['source_receipt'])
    assert result['physical_consumer_source_recomputed'] and not result['pricing_graph_independently_verified']
    assert manifest['validation']['journal_replayed'] and manifest['validation']['dated_source_reconciled']

def test_atomic_publication_retries_only_transient_windows_sharing_errors(tmp_path,monkeypatch):
    staging=tmp_path/'staging';staging.mkdir();final=tmp_path/'run';calls=[];checks=[]
    original=engine.os.replace
    def replace(source,target):
        calls.append(True)
        if len(calls)==1:
            error=PermissionError('temporary scanner lock');error.winerror=32;raise error
        original(source,target)
    monkeypatch.setattr(engine.os,'replace',replace)
    engine._publish_directory(staging,final,lambda:checks.append(True))
    assert final.is_dir() and len(calls)==2 and len(checks)==3

def test_retry_publication_cancellation_preserves_unpublished_staging(tmp_path,monkeypatch):
    staging=tmp_path/'staging';staging.mkdir();final=tmp_path/'run';checks=[]
    def replace(*args):
        error=PermissionError('temporary scanner lock');error.winerror=5;raise error
    def check():
        checks.append(True)
        if len(checks)==2:raise InterruptedError('cancelled during publication retry')
    monkeypatch.setattr(engine.os,'replace',replace)
    with pytest.raises(InterruptedError):engine._publish_directory(staging,final,check)
    assert staging.is_dir() and not final.exists()


def test_retry_publication_deadline_preserves_unpublished_staging(tmp_path,monkeypatch):
    staging=tmp_path/'staging';staging.mkdir();final=tmp_path/'run';checks=[]
    def replace(*args):
        error=PermissionError('temporary scanner lock');error.winerror=33;raise error
    def check():
        checks.append(True)
        if len(checks)==2:raise TimeoutError('deadline during sharing retry')
    monkeypatch.setattr(engine.os,'replace',replace)
    with pytest.raises(TimeoutError):engine._publish_directory(staging,final,check)
    assert staging.is_dir() and not final.exists()


def test_nonsharing_publication_failure_is_not_retried(tmp_path,monkeypatch):
    staging=tmp_path/'staging';staging.mkdir();calls=[]
    def replace(*args):calls.append(True);raise PermissionError('not a Windows sharing error')
    monkeypatch.setattr(engine.os,'replace',replace)
    with pytest.raises(PermissionError):engine._publish_directory(staging,tmp_path/'run',lambda:False)
    assert len(calls)==1 and staging.is_dir()


@pytest.mark.parametrize('changed',['binary','library','source'])
def test_sharing_retry_refences_native_and_source_identity(tmp_path,monkeypatch,changed):
    attempts=[];original=engine.os.replace
    def replace(source,target):
        if Path(target).name.startswith('run-') and not attempts:
            attempts.append(True)
            if changed=='source':monkeypatch.setattr(engine,'source_identity',lambda:'changed')
            else:
                path=engine.binary_path() if changed=='binary' else engine.library_path();hash_file=engine._hash
                monkeypatch.setattr(engine,'_hash',lambda p:'changed' if p==path else hash_file(p))
            error=PermissionError('temporary scanner lock');error.winerror=32;raise error
        original(source,target)
    monkeypatch.setattr(engine.os,'replace',replace)
    with pytest.raises(RuntimeError,match='identity changed before publication'):
        engine.run_model_ledger(example(),tmp_path)
    assert attempts and not list(tmp_path.glob('run-*'))


def test_publication_deadline_is_rechecked_after_identity_hashing(tmp_path,monkeypatch):
    from types import SimpleNamespace
    import time
    offset=[0.];publishing=[False];hash_file=engine._hash;publish=engine._publish_directory
    def slow_identity(path):
        digest=hash_file(path)
        if publishing[0]:offset[0]=1801.
        return digest
    def publication(staging,final,check,**kwargs):publishing[0]=True;publish(staging,final,check,**kwargs)
    monkeypatch.setattr(engine,'time',SimpleNamespace(perf_counter=lambda:time.perf_counter()+offset[0],sleep=time.sleep))
    monkeypatch.setattr(engine,'_hash',slow_identity)
    monkeypatch.setattr(engine,'_publish_directory',publication)
    with pytest.raises(TimeoutError):engine.run_model_ledger(example(),tmp_path)
    assert not list(tmp_path.glob('run-*'))


@pytest.mark.parametrize('failure',['deadline','source'])
def test_successful_late_rename_is_unpublished_on_final_fence_failure(tmp_path,monkeypatch,failure):
    from types import SimpleNamespace
    import time
    offset=[0.];renamed=[];replace=engine.os.replace
    monkeypatch.setattr(engine,'time',SimpleNamespace(perf_counter=lambda:time.perf_counter()+offset[0],sleep=time.sleep))
    def late(source,target):
        replace(source,target)
        if Path(target).name.startswith('run-'):
            renamed.append(Path(target))
            if failure=='deadline':offset[0]=1801.
            else:monkeypatch.setattr(engine,'source_identity',lambda:'replacement-source')
    monkeypatch.setattr(engine.os,'replace',late)
    with pytest.raises(TimeoutError if failure=='deadline' else RuntimeError):engine.run_model_ledger(example(),tmp_path)
    assert renamed and not list(tmp_path.glob('run-*')) and not list(tmp_path.rglob('manifest.json'))


def test_unpublished_cleanup_cannot_replace_preexisting_or_outside_directory(tmp_path):
    root=tmp_path/'output';root.mkdir();staging=root/'staging';staging.mkdir()
    existing=root/'run-existing';existing.mkdir();(existing/'keep.txt').write_text('preserve')
    outside=tmp_path/'run-outside'
    for final in (existing,outside):
        with pytest.raises(ValueError):engine._publish_directory(staging,final,lambda:False,owned_output_root=root)
    assert (existing/'keep.txt').read_text()=='preserve' and staging.is_dir() and not outside.exists()


def test_postrename_cleanup_keeps_terminal_exception_and_removes_manifest(tmp_path,monkeypatch):
    staging=tmp_path/'staging';staging.mkdir();(staging/'manifest.json').write_text('{}');final=tmp_path/'run-new';calls=[]
    def check():
        calls.append(True)
        if len(calls)==2:raise TimeoutError('original terminal deadline')
    def fail(*args):raise OSError('injected directory cleanup failure')
    monkeypatch.setattr(engine.shutil,'rmtree',fail)
    with pytest.raises(TimeoutError,match='original terminal deadline') as error:
        engine._publish_directory(staging,final,check,owned_output_root=tmp_path)
    assert not (final/'manifest.json').exists() and 'cleanup failed' in error.value.__notes__[0]
@pytest.mark.parametrize('target',['raw','physical','cash','origin','opening','policy','run','prepared'])
def test_tampered_modeled_economic_source_rejects(saved,target):
    raw,source,_=saved;changed=deepcopy(source);receipt=changed['source_receipt'];capture=receipt['capture']
    if target=='raw':receipt['raw_request']['graph']['currency']='EUR'
    elif target=='physical':next(s for s in receipt['graph']['native_sources'] if s['role']=='physical')['projection']['cashflows'][0]['principal']+=1
    elif target=='cash':capture['sources'][0]['schedule']['events'][0]['components']['cash_interest']+=1
    elif target=='origin':capture['accrual_plans'][0]['segments'][0]['origin_id']='invented-origin'
    elif target=='opening':capture['openings'][0]['principal']+=1
    elif target=='policy':capture['policy']['policy_revision']='different-review'
    elif target=='run':capture['sources'][0]['schedule']['events'][0]['scenario']['scenario_id']='different-run'
    else:changed['prepared_request']['specification']['accounts'][0]['equity']+=1
    with pytest.raises(ArithmeticError):reference.verify_preparation(raw,changed['prepared_request'],receipt)
@pytest.mark.parametrize('failure',['verify','cancel','deadline','source'])
def test_modeled_verification_failure_never_publishes(tmp_path,monkeypatch,failure):
    original=reference.verify_preparation;cancelled=[False]
    def fail(*args):
        result=original(*args)
        if failure=='verify':raise ArithmeticError('independent raw-source mismatch')
        if failure=='deadline':raise TimeoutError('independent verification timeout')
        cancelled[0]=True;return result
    if failure=='source':
        calls=[0]
        def identity():calls[0]+=1;return str(calls[0])
        monkeypatch.setattr(engine,'source_identity',identity)
    else:monkeypatch.setattr(reference,'verify_preparation',fail)
    with pytest.raises((ArithmeticError,InterruptedError,TimeoutError,RuntimeError)):
        engine.run_model_ledger(example(),tmp_path,cancelled=lambda:cancelled[0])
    assert not list(tmp_path.glob('run-*'))
@pytest.mark.parametrize('day_count',['act360','act365_fixed'])
def test_independent_actual_day_consumer_source_cases(tmp_path,day_count):
    raw=example()
    for p in raw['graph']['positions']:p['product']['contract']['day_count']=day_count
    assert engine.run_model_ledger(raw,tmp_path).is_file()


def test_unadmitted_accrual_convention_rejects_before_publication(tmp_path):
    raw=example();raw['graph']['positions'][0]['product']['contract']['day_count']='act_act_isda'
    with pytest.raises(RuntimeError,match='ACT360 or ACT365F'):engine.run_model_ledger(raw,tmp_path)
    assert not list(tmp_path.glob('run-*'))


@pytest.mark.parametrize('target',['explicit_opening','duplicate_owner','duplicate_run','source_schema',
    'capture_schema','graph_identity','exact_raw','physical_weight'])
def test_independent_consumer_admission_does_not_normalize_conflicting_sources(saved,target):
    original,source,_=saved;raw=deepcopy(original);changed=deepcopy(source);receipt=changed['source_receipt']
    if target=='explicit_opening':raw['specification']['positions'][0]['balance']=999.
    elif target=='duplicate_owner':
        raw['owners'].append(deepcopy(raw['owners'][0]));receipt['capture']['owners']=deepcopy(raw['owners'])
        changed['prepared_request']['dated']['owners'].append(deepcopy(changed['prepared_request']['dated']['owners'][0]))
    elif target=='duplicate_run':raw['run_mapping'].append(deepcopy(raw['run_mapping'][0]))
    elif target=='source_schema':receipt['schema']='forged-source'
    elif target=='capture_schema':receipt['capture']['schema']='forged-capture'
    elif target=='graph_identity':receipt['graph']['identity']['model_revision']='forged-model'
    elif target=='physical_weight':raw['graph']['positions'][0]['product']['histories'][0]['physical'][0]['weight']=.5
    else:receipt['raw_request']['graph']['bump_bp']+=1e-10
    if target!='exact_raw':receipt['raw_request']=deepcopy(raw)
    with pytest.raises(ArithmeticError):reference.verify_preparation(raw,changed['prepared_request'],receipt)


def test_consumer_verification_honors_boolean_cancellation(saved):
    raw,source,_=saved
    with pytest.raises(InterruptedError):
        reference.verify_preparation(raw,source['prepared_request'],source['source_receipt'],lambda:True)
