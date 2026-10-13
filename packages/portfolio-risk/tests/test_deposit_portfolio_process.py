"""Default executable owns source2 economics; persisted source/GL replay is mandatory."""
import json,subprocess,sys
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_ledger

ROOT=Path(__file__).resolve().parents[3]
SCHEMA='deposit-performing-portfolio-ledger-2'
TRUE=('physical_deposit_portfolio_sources_recomputed','original_yield_continuations_recomputed','full_unweighted_gross_origins_verified','ordered_pooled_same_sim_journal_replayed','persisted_partitions_replayed')
def raw(name=SCHEMA):return json.loads((ROOT/'examples/models'/f'{name}.json').read_text())
@pytest.mark.parametrize('name',[SCHEMA,SCHEMA+'-90-day'])
def test_default_source2_native_route_reopens_complete_pooled_partitions(tmp_path,monkeypatch,name):
    # A fresh process does not inherit the package's Python-reference fixture.
    code="""
import json,sys
from pathlib import Path
from portfolio_risk.analytics import model_ledger
from portfolio_risk.core.runtime import RunConfig
assert RunConfig().compute_backend == 'rust'
assert model_ledger.binary_path().parent.name == 'release'
def forbidden(*args,**kwargs):raise AssertionError('unowned Python financial production callback')
model_ledger.validate_spec=forbidden;model_ledger._finalize=forbidden
request=json.loads(Path(sys.argv[1]).read_text())
print(model_ledger.run_model_ledger(request,Path(sys.argv[2]),partition_rows=29,timeout=120.))
"""
    child=subprocess.run([sys.executable,'-c',code,str(ROOT/'examples/models'/f'{name}.json'),str(tmp_path)],capture_output=True,text=True,timeout=150.)
    assert child.returncode==0,child.stderr
    path=Path(child.stdout.strip())
    manifest=json.loads(path.read_text());header=json.loads((path.parent/'prepared-source.json').read_text())
    assert all(manifest['validation'][k] is True for k in TRUE)
    assert model_ledger.exact_json_identity(header['source_receipt']['raw_request'],raw(name))
    assert header['schema']=='deposit-performing-portfolio-workflow-2'
    assert all(manifest['validation'][k] is False for k in ('accounting_standard_approval','empirical_validation','installed_adoption','saved_book_adoption','capital_ftp_eligible'))
    assert manifest['validation'].get('journal_replayed') is None
    assert len(manifest['tables']['journal']['parts'])>1
    assert json.loads((path.parent/'producer-completion.json').read_text())=={'complete':True,'protocol':3}
@pytest.mark.parametrize('case',['missing_owner','missing_named','unknown_prefix','changed_terms','generic_cash'])
def test_default_source2_refuses_unowned_or_partial_histories_before_publication(tmp_path,case):
    r=raw()
    if case=='missing_owner':r['positions'].pop()
    elif case=='missing_named':r['positions'][1]['paths'].pop()
    elif case=='unknown_prefix':r['positions'][1]['paths'][1]['source']['continuation']['current']['drivers'][0]['reference_rate']+=.01
    elif case=='changed_terms':r['positions'][1]['paths'][1]['source']['continuation']['current']['segments'][0]['opening_paid_rate']+=.01
    else:r['specification']['positions'][0]['rate']=.01
    with pytest.raises((ValueError,RuntimeError,ArithmeticError)):model_ledger.run_model_ledger(r,tmp_path,timeout=30.)
    assert not list(tmp_path.rglob('manifest.json'))
def test_original_native_bytes_reject_duplicate_selected_source2_envelope():
    request=json.dumps(dict(request=raw(),execution_timeout_ms=60000),separators=(',',':'))
    request=request.replace('"execution_timeout_ms":60000','"execution_timeout_ms":60000,"execution_timeout_ms":60000')
    p=subprocess.run([str(model_ledger.binary_path())],input=request.encode(),capture_output=True,timeout=30.)
    assert p.returncode!=0 and not p.stdout and b'duplicate' in p.stderr.lower()
