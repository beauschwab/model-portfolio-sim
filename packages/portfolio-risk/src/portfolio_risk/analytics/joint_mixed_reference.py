"""Independent complete source reconstruction and one chronological pooled GL.

This verifier uses raw equations; it never calls a native financial producer or
combines separately produced journals. Exact input identity is distinct from
the unchanged tolerance for independently calculated financial amounts.
"""
from copy import deepcopy
from collections import defaultdict
import json,math
from . import _deposit_portfolio_income_reference as c
from .deposit_portfolio_reference import CheckedControl,_seal,_identity
from .joint_source_bounds_reference import admission,MAX_STREAM
from .joint_deposit_source_reference import reconstruct as deposit_sources
from .joint_derivative_source_reference import reconstruct as derivative_sources
from .joint_pooled_replay_reference import replay
from .saved_derivative_mixed_reference import dated_work
from .saved_derivative_identity import loads

SCHEMA='saved-derivative-deposit-portfolio-mixed-1'
HEADER='schema model_schema raw_joint_request portfolio_prepared_request source_receipt'
RECEIPT=('schema original_request raw_portfolio raw_derivative_owners portfolio_prepared_request '
 'portfolio_usage portfolio_sources native_sources admission derivative_work_used derivative_capacity_used '
 'derivative_output_bytes same_sim_native_replay empirical_validation capital_ftp_eligible public_adoption')

def verify_preparation(raw,header,check=lambda:False):
    c.tick(check);c.keys(header,HEADER);c.exact(header['schema'],'model-ledger-preparation-1');c.exact(header['model_schema'],SCHEMA)
    c.exact(header['raw_joint_request'],raw);receipt=header['source_receipt'];c.keys(receipt,RECEIPT)
    c.exact(receipt['schema'],SCHEMA);c.exact(receipt['original_request'],raw)
    c.exact(receipt['raw_portfolio'],raw['portfolio']);c.exact(receipt['raw_derivative_owners'],raw['derivative_owners'])
    c.exact(receipt['same_sim_native_replay'],True)
    for flag in ('empirical_validation','capital_ftp_eligible','public_adoption'):c.exact(receipt[flag],False)
    # All selected raw sources, including zero cash/participation, are admitted
    # before independent source projection. No generator inputs come from flags.
    upper=admission(raw,check);c.exact(receipt['admission'],upper)
    prepared=header['portfolio_prepared_request'];c.exact(receipt['portfolio_prepared_request'],prepared)
    inputs=deposit_sources(raw['portfolio'],prepared,receipt['portfolio_sources'],receipt['portfolio_usage'],upper['deposit'],check)
    scenes,contexts=derivative_sources(raw['derivative_owners'],receipt['native_sources'],inputs[1],prepared['dated']['as_of_ordinal'],check)
    work=dated_work(dict(dated=prepared))
    for context in contexts:
        source=context['source'];opening=len(context['opening']);lines=sum(len(g['lines'])for g in context['groups'])
        daily=sum(len(p['financial_trial_balance'])+len(p['custody_trial_balance'])for p in source['daily'])
        work+=250000+opening+lines+daily+6*(opening+lines)*(inputs[1]['horizon_days']+1)
    c.exact(receipt['derivative_work_used'],work)
    if work>upper['deposit']['work_units']+upper['derivative_work_upper']:raise ArithmeticError('mapped derivative work exceeds prior reservation')
    used=receipt['derivative_capacity_used']
    if type(used)is not int or not 0<used<=upper['derivative_retained_upper']:raise ArithmeticError('mapped retained capacity domain')
    wire=sum(c.native_json_length(source)for source in receipt['native_sources']);c.exact(receipt['derivative_output_bytes'],wire)
    if wire>upper['derivative_retained_upper']:raise ArithmeticError('actual source serialization exceeds prior reservation')
    data=(inputs,scenes)
    if len(json.dumps(_identity(data),separators=(',',':'),allow_nan=False).encode())>MAX_STREAM:raise ArithmeticError('independent source/control retention32MiB')
    c.tick(check);control=object.__new__(CheckedControl)
    control._raw=deepcopy(raw);control._tables=data;control._pin=_seal(control._raw,data)
    return control

def verify_stream(raw,control,facade,check=lambda:False):
    from .joint_mixed_transport import SCHEMAS,JSON_CELLS
    c.tick(check)
    if type(control)is not CheckedControl:raise ArithmeticError('private checked joint sources required')
    c.exact(raw,control._raw)
    if control._pin!=_seal(control._raw,control._tables):raise ArithmeticError('private source control mutated')
    c.exact(facade.completion,dict(complete=True,protocol=3))
    c.exact(facade.schemas,{n:{k:str(v)for k,v in s.items()}for n,s in SCHEMAS.items()})
    counts=defaultdict(int)
    def rows(name):
        for frame in facade.frames(name):
            c.tick(check);c.exact({k:str(v)for k,v in frame.schema.items()},facade.schemas[name])
            if not 1<=frame.height<=65536:raise ArithmeticError('bounded persisted frame required')
            for row in frame.iter_rows(named=True):
                c.tick(check)
                for key in JSON_CELLS.get(name,()):row[key]=loads(row[key])
                if name=='journal':
                    debit=row.pop('debit');credit=row.pop('credit');tx=row.pop('transaction_id')
                    if not all(type(v)is float and math.isfinite(v)and v>=0. for v in(debit,credit))or debit and credit:raise ArithmeticError('financial journal debit/credit domain')
                    number=tx.rsplit(':',1)[-1]
                    if not number.isdigit()or tx!=row['scenario']+':'+str(int(number)):raise ArithmeticError('exact contiguous transaction identity')
                    row['transaction_number']=int(number);row['value']=debit-credit
                yield row
    cursors={name:iter(rows(name))for name in SCHEMAS}
    def emit(name,want):
        c.tick(check);got=next(cursors[name],None)
        if got is None:raise ArithmeticError('missing ordered joint row '+name)
        c.equal(got,want,name+'/'+str(counts[name]));counts[name]+=1
        for key in ('review','original_yield_anchor'):
            if key in want:c.exact(got[key],want[key])
        if name=='deposit_principal_lineage' or (name=='journal'and want['event']=='source_principal_basis_reclassification'):c.exact(got,want)
    inputs,scenes=control._tables
    rest=replay(*inputs[:7],check,inputs[7],scenes,emit)
    for name,values in rest.items():
        if name in ('reported_daily_balance','reporting_rights','acquisition_lineage','daily_balance'):continue
        for row in values:emit(name,row)
    for name,cursor in cursors.items():
        if next(cursor,None)is not None:raise ArithmeticError('extra persisted joint row '+name)
    c.tick(check);c.exact(facade.completion,dict(complete=True,protocol=3));c.exact(raw,control._raw)
    if control._pin!=_seal(control._raw,control._tables):raise ArithmeticError('private joint control changed during replay')
    return dict(schema='saved-derivative-deposit-portfolio-verification-1',verified=True,
        physical_deposit_portfolio_sources_recomputed=True,original_yield_continuations_recomputed=True,
        native_derivative_source_recomputed=True,full_unweighted_gross_origins_verified=True,
        ordered_pooled_same_sim_journal_replayed=True,single_daily_gl_verified=True,persisted_partitions_replayed=True,
        journal_lines=counts['journal'],accounting_standard_approval=False,empirical_validation=False,
        installed_adoption=False,saved_book_adoption=False,capital_ftp_eligible=False)
