"""Independent source/FIFO/pooled replay; native producer is never the oracle."""
import json
from copy import deepcopy
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_ledger
from portfolio_risk.analytics import deposit_portfolio_reference as reference
from portfolio_risk.analytics.deposit_portfolio_transport import PortfolioDepositWriter, SCHEMAS
from portfolio_risk.analytics.deposit_revision_transport import VerificationFrames

ROOT = Path(__file__).resolve().parents[3]

@pytest.fixture(scope='module')
def captured(tmp_path_factory):
    raw=json.loads((ROOT/'examples/models/deposit-performing-portfolio-ledger-2.json').read_text())
    manifest_path=model_ledger.run_model_ledger(raw,tmp_path_factory.mktemp('source2-reference'),partition_rows=31,timeout=120.)
    manifest=json.loads(manifest_path.read_text())
    header=json.loads((manifest_path.parent/'prepared-source.json').read_text())
    return raw,header,manifest,manifest_path.parent

def control(captured):
    raw,header,_,_=captured
    return reference.verify_preparation(raw,header['prepared_request'],header['source_receipt'])

def frames(captured):
    _,_,manifest,root=captured
    writer=PortfolioDepositWriter(root,manifest['partition_rows'],schemas=SCHEMAS)
    for name,table in manifest['tables'].items():writer.parts[name]=deepcopy(table['parts'])
    return VerificationFrames(writer,lambda:json.loads((root/'producer-completion.json').read_text()))

def test_complete_independent_control_stream_is_private_and_repartition_safe(captured):
    raw,_,_,_=captured
    with pytest.raises(TypeError):reference.CheckedControl()
    receipt=reference.verify_stream(raw,control(captured),frames(captured))
    assert receipt['full_unweighted_gross_origins_verified'] is True
    assert receipt['ordered_pooled_same_sim_journal_replayed'] is True
    assert receipt['accounting_standard_approval'] is False
    assert receipt['capital_ftp_eligible'] is False

def test_browser_integerized_declared_f64_fields_preserve_exact_retained_raw(tmp_path):
    def browser(value):
        if type(value) is dict:return {k:browser(v) for k,v in value.items()}
        if type(value) is list:return [browser(v) for v in value]
        if type(value) is float and value.is_integer():return int(value)
        return value
    raw=browser(json.loads((ROOT/'examples/models/deposit-performing-portfolio-ledger-2.json').read_text()))
    assert type(raw['runs'][0]['severity']) is int
    path=model_ledger.run_model_ledger(raw,tmp_path,partition_rows=47,timeout=120.)
    manifest=json.loads(path.read_text());header=json.loads((path.parent/'prepared-source.json').read_text())
    reference.c.exact(header['source_receipt']['raw_request'],raw)
    assert type(header['prepared_request']['dated']['runs'][0]['severity']) is float
    assert manifest['validation']['persisted_partitions_replayed'] is True

@pytest.mark.parametrize('case',['severity','principal','original_yield','date'])
def test_raw_boolean_financial_and_date_aliases_are_not_cast_into_valid_sources(tmp_path,case):
    raw=json.loads((ROOT/'examples/models/deposit-performing-portfolio-ledger-2.json').read_text())
    continuation=raw['positions'][0]['paths'][0]['source']['continuation']
    if case=='severity':raw['runs'][0]['severity']=True
    elif case=='principal':continuation['anchor']['original_principal']=True
    elif case=='original_yield':continuation['anchor']['original_annual_effective_rate']=False
    else:continuation['revision_ordinal']=True
    with pytest.raises((ValueError,RuntimeError,ArithmeticError)):model_ledger.run_model_ledger(raw,tmp_path,timeout=30.)
    assert not list(tmp_path.rglob('manifest.json'))

@pytest.mark.parametrize('case',['bool_completion','float_protocol','extra_completion','raw_bool','raw_signed_zero','raw_integer_date','private_mutation','fake_control'])
def test_exact_control_raw_completion_and_private_state_refuse_aliases(captured,case):
    raw=deepcopy(captured[0]);checked=control(captured);facade=frames(captured)
    if case=='bool_completion':facade._completion=lambda:dict(complete=1,protocol=3)
    elif case=='float_protocol':facade._completion=lambda:dict(complete=True,protocol=3.)
    elif case=='extra_completion':facade._completion=lambda:dict(complete=True,protocol=3,claimed=True)
    elif case=='raw_bool':raw['runs'][0]['severity']=True
    elif case=='raw_signed_zero':raw['specification']['positions'][0]['rate']=-0.
    elif case=='raw_integer_date':raw['positions'][0]['paths'][0]['source']['continuation']['revision_ordinal']=float(raw['positions'][0]['paths'][0]['source']['continuation']['revision_ordinal'])
    elif case=='private_mutation':checked._tables[1]['accounts'][0]['cash']+=1.
    else:checked={'source_checked':True}
    with pytest.raises((ArithmeticError,ValueError)):reference.verify_stream(raw,checked,facade)

@pytest.mark.parametrize('case',['source_original','named_current','original_yield','origin_cash','source_after_principal','prepared_cash','allowance','missing_run'])
def test_source_and_generated_calendar_cannot_be_financial_authority(captured,case):
    raw,header,_,_=captured;h=deepcopy(header)
    native=h['source_receipt']['owners'][0]['paths'][0]['native']['native']
    if case=='source_original':native['original']['source']['cashflows'][0]['principal']+=.01
    elif case=='named_current':h['source_receipt']['owners'][1]['paths'][1]['native']['native']['current']['source']['cashflows'][0]['cash_interest']+=.01
    elif case=='original_yield':native['raw_request']['continuation']['anchor']['original_annual_effective_rate']+=.0001
    elif case=='origin_cash':h['source_receipt']['cash_origins'][0]['native_event_id']='forged'
    elif case=='source_after_principal':native['current']['atoms'][0]['principal_after']+=.01
    elif case=='prepared_cash':h['prepared_request']['dated']['sources'][0]['schedule']['events'][0]['components']['principal']+=.01
    elif case=='allowance':h['prepared_request']['specification']['positions'][0]['allowance']=1.
    else:h['source_receipt']['owners'][1]['paths'].pop()
    with pytest.raises((ArithmeticError,ValueError,KeyError)):reference.verify_preparation(raw,h['prepared_request'],h['source_receipt'])

@pytest.mark.parametrize('case',['drop_line','cash_line','transaction','tiny_basis','source_lineage','revision_twice','schema','cancel'])
def test_ordered_stream_and_gross_source_checkpoint_adversaries(captured,case):
    raw=captured[0];checked=control(captured);facade=frames(captured);read=facade.frames;changed=False
    def altered(name):
        nonlocal changed
        for frame in read(name):
            if not changed and ((case in ('drop_line','cash_line','transaction','tiny_basis') and name=='journal') or (case=='source_lineage' and name=='deposit_principal_lineage') or (case=='revision_twice' and name=='deposit_revision')):
                changed=True
                rows=frame.to_dicts()
                if case=='drop_line':rows=rows[1:]
                elif case=='cash_line':rows[0]['debit']+=.01
                elif case=='transaction':rows[0]['transaction_id']+='0'
                elif case=='tiny_basis':rows[0]['credit']+=1e-6
                elif case=='source_lineage':rows[0]['source_atom']='{}'
                else:rows=rows+[dict(rows[-1])]
                import polars as pl
                frame=pl.DataFrame(rows,schema=frame.schema)
            yield frame
    facade.frames=altered
    if case=='schema':facade.schemas['journal']['day']='Float64'
    check=(lambda:True) if case=='cancel' else (lambda:False)
    with pytest.raises((ArithmeticError,ValueError,InterruptedError)):reference.verify_stream(raw,checked,facade,check)
