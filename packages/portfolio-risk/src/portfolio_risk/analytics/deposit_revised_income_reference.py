"""Independent source and complete chronological GL publication verifier.

Production transport must call these gates around immutable partition storage.
This module performs no native financial callback and is not an execution backend.
"""
from copy import deepcopy
from dataclasses import dataclass
import hashlib
import json
import math
import polars as pl
from portfolio_risk.analytics.dated_ledger import DATED_SCHEMAS
from . import _deposit_revision_source_reference as source
from . import _deposit_revision_calendar_reference as calendar

SCHEMA='deposit-revised-income-ledger-1'
SEAL=object()
LIMIT=32*1024*1024
SCHEMAS=dict(DATED_SCHEMAS,deposit_revision={
    **dict.fromkeys('scenario run_id position_id review_json'.split(),pl.String),
    'ordinal':pl.Int64,
    **dict.fromkeys('original_basis revised_basis basis_revision cash'.split(),pl.Float64),
    **dict.fromkeys('original_yield_retained accounting_standard_approval capital_ftp_eligible'.split(),pl.Boolean)})

def digest(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode()).hexdigest()

@dataclass(frozen=True)
class CheckedControl:
    raw:dict
    prepared:dict
    receipt:dict
    tables:dict
    digest:str
    seal:object

def _body(c):return dict(raw=c.raw,prepared=c.prepared,receipt=c.receipt,tables=c.tables)

def _check_control(raw,c,check):
    source.tick(check)
    if type(c) is not CheckedControl or c.seal is not SEAL:raise ArithmeticError('private independently checked deposit control required')
    source.checks.exact(raw,c.raw,'exact queued original deposit request')
    if digest(_body(c))!=c.digest:raise ArithmeticError('private checked source/calendar/GL control changed')

def verify_preparation(raw,prepared,receipt,check=lambda:False):
    """Check exact original raw separately from typed financial/source clones."""
    source.tick(check)
    source.checks.keys(receipt,'raw_request original_source original_pv paths revisions admission cash_origins interest_rights_policy original_anchors revision_policy accounting_standard_approval installed_adoption capital_ftp_eligible')
    source.checks.exact(receipt['raw_request'],raw,'original selected raw request')
    # The contained mathematical gate expects its historical wrapper. Construct
    # this local view; it cannot confer authority on supplied financial arrays.
    financial_receipt={k:v for k,v in receipt.items() if k!='raw_request'}
    expected=calendar.verify_preparation(raw,dict(schema=SCHEMA,raw_request=raw,prepared_request=prepared,source_receipt=financial_receipt),check)
    tables={('path' if k=='daily_balance' else k):v for k,v in expected['tables'].items() if k not in {'reported_daily_balance','reporting_rights','acquisition_lineage'}}
    journal=[]
    for row in tables['journal']:
        new={k:v for k,v in row.items() if k not in ('transaction_number','value')}
        new.update(transaction_id=row['scenario']+':'+str(row['transaction_number']),debit=max(row['value'],0.),credit=max(-row['value'],0.))
        journal.append(new)
    tables['journal']=journal
    # Raw JSON number spelling is immutable; independently computed monetary
    # report cells follow their declared Float64 schema, including inactive zeros.
    # Keep metadata, dates, review objects and the financial tolerance unchanged.
    for table, rows in tables.items():
        for row in rows:
            for field, dtype in SCHEMAS[table].items():
                if dtype == pl.Float64 and field in row and row[field] is not None:
                    if type(row[field]) not in (int, float) or not math.isfinite(row[field]):
                        raise ArithmeticError('finite independent Float64 report/'+table+'/'+field)
                    row[field]=float(row[field])
    # Bound independent control storage explicitly; native output/RSS budgets
    # remain separate contracts, not evidence of Python or total-process RSS.
    body=dict(raw=deepcopy(raw),prepared=deepcopy(prepared),receipt=deepcopy(receipt),tables=tables)
    encoded=json.dumps(body,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode()
    if len(encoded)>LIMIT:raise ArithmeticError('independent source/control32MiB admission')
    source.tick(check)
    return CheckedControl(**body,digest=hashlib.sha256(encoded).hexdigest(),seal=SEAL)

def verify_workflow(raw,workflow,check=lambda:False):
    source.checks.keys(workflow,'schema model_schema prepared_request source_receipt')
    source.checks.exact(workflow['schema'],'model-ledger-preparation-1','public workflow schema')
    source.checks.exact(workflow['model_schema'],SCHEMA,'public deposit schema')
    return verify_preparation(raw,workflow['prepared_request'],workflow['source_receipt'],check)

def _json_cell(value):
    if type(value) is not str:raise ArithmeticError('exact JSON metadata cell required')
    def unique(pairs):
        d={}
        for k,v in pairs:
            if k in d:raise ArithmeticError('duplicate retained metadata key '+k)
            d[k]=v
        return d
    return json.loads(value,object_pairs_hook=unique,parse_constant=lambda v:(_ for _ in ()).throw(ArithmeticError('nonfinite metadata '+v)))

def verify_stream(raw,control,frames,check=lambda:False):
    """Consume repeatable bounded frame partitions without producer hydration."""
    _check_control(raw,control,check)
    if not hasattr(frames,'frames') or not hasattr(frames,'schemas') or not hasattr(frames,'completion'):
        raise ArithmeticError('bounded persisted frame and actual completion facade required')
    source.checks.exact(frames.completion,dict(complete=True,protocol=3),'actual persisted producer completion')
    schemas=frames.schemas
    if type(schemas) is not dict or set(schemas)-set(SCHEMAS):raise ArithmeticError('unknown financial partition schemas')
    for table,rows in control.tables.items():
        if rows and table not in schemas:raise ArithmeticError('missing complete deposit table '+table)
    work=0;bytes_seen=0
    for table,description in schemas.items():
        source.tick(check)
        expected_schema={k:str(v) for k,v in SCHEMAS[table].items()}
        source.checks.exact(description,expected_schema,'retained partition schema/'+table)
        expected=control.tables.get(table,[]);cursor=0
        for frame in frames.frames(table):
            source.tick(check)
            if not isinstance(frame,pl.DataFrame) or frame.height>65536:raise ArithmeticError('bounded Polars partition required')
            source.checks.exact({k:str(v) for k,v in frame.schema.items()},expected_schema,'actual partition schema/'+table)
            # Allocation refers to a single bounded partition only. Never concat
            # partitions or request a result dictionary containing all tables.
            for row in frame.iter_rows(named=True):
                source.tick(check);work+=len(row)+1
                bytes_seen+=len(json.dumps(row,separators=(',',':'),allow_nan=False).encode())
                if work>8000000 or bytes_seen>LIMIT:raise ArithmeticError('independent partition work/bytes admission')
                if cursor>=len(expected):raise ArithmeticError('added financial row/'+table)
                if table=='journal':
                    debit=row['debit'];credit=row['credit']
                    if not math.isfinite(debit) or not math.isfinite(credit) or debit<0. or credit<0. or (debit>0.)==(credit>0.):
                        raise ArithmeticError('nonzero one-sided journal posting required')
                if table=='deposit_revision':row=dict(row);row['review']=_json_cell(row.pop('review_json'))
                source.equal(row,expected[cursor],'ordered independent whole-GL/'+table)
                cursor+=1
        if cursor!=len(expected):raise ArithmeticError('omitted financial rows/'+table)
    # Re-read actual persisted completion and control identities after partitions;
    # a facade changing files between passes cannot manufacture a successful gate.
    source.checks.exact(frames.completion,dict(complete=True,protocol=3),'final actual producer completion')
    _check_control(raw,control,check)
    return dict(verified=True,physical_deposit_source_recomputed=True,original_yield_revision_recomputed=True,
        ordered_deposit_journal_recomputed=True,complete_gross_cash_origin_rights_verified=True,
        accounting_standard_approval=False,capital_ftp_eligible=False,installed_adoption=False)
