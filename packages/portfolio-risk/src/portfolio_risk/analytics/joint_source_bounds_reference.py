"""Independent structural upper admission, before either financial projection."""
from copy import deepcopy
from . import _deposit_portfolio_income_reference as c
from .deposit_portfolio_reference import portfolio_admission
from .saved_derivative_mapping_reference import validate_mapping,require
from .saved_derivative_financial_reference import _admit as admit_accounting
from .saved_derivative_identity import probe
from .saved_derivative_mapping_reference import exact
from .saved_derivative_dto_reference import saved_dto as typed
from .saved_derivative_work_bounds_reference import counters
from ._deposit_portfolio_calendar import _normalize
MAX_WORK=8_000_000;MAX_RETAINED=128*1024*1024;MAX_STREAM=32*1024*1024

def arrays(value):
    if type(value)is list:return len(value)+sum(arrays(x)for x in value)
    if type(value)is dict:return sum(arrays(x)for x in value.values())
    return 0

def escaped(value):
    if type(value)is str:return c.native_json_length(value)
    if type(value)is list:return max([0]+[escaped(x)for x in value])
    if type(value)is dict:return max([0]+[max(c.native_json_length(k),escaped(v))for k,v in value.items()])
    return 0

def admission(raw,check=lambda:False):
    c.keys(raw,'schema portfolio derivative_owners policy');c.exact(raw['schema'],'saved-derivative-deposit-portfolio-mixed-1')
    c.keys(raw['policy'],'identity ownership phase');c.keys(raw['policy']['identity'],'id revision')
    require(all(type(x)is str and x for x in raw['policy']['identity'].values()),'explicit reviewed joint policy identity')
    c.exact(raw['policy']['ownership'],'exclusive_native_deposit_portfolio_and_derivative-1')
    c.exact(raw['policy']['phase'],'dated_cash_then_deposit_atom_then_derivative_native_events-1')
    rb=c.native_json_length(raw);require(rb<=8*1024*1024,'joint raw arena preparse refusal')
    require(type(raw['derivative_owners'])is list and 0<len(raw['derivative_owners'])<=64,'bounded derivative owners')
    mixed_days=_normalize(raw['portfolio']['specification'])['horizon_days']+1
    work=capacity=count=0
    for owner in raw['derivative_owners']:
        c.keys(owner,'account_id runs');require(type(owner['runs'])is list and 0<len(owner['runs'])<=64,'bounded derivative runs')
        for run in owner['runs']:
            probe(check);c.keys(run,'ledger_scenario saved');validate_mapping(run['saved']);value=typed(run['saved']);admit_accounting(value['physical'])
            node_records=0
            for derivative in [value['physical']['derivative']]+([]if value['physical']['pricing_xva_reference']is None else[value['physical']['pricing_xva_reference']]):
                c.exact(derivative['operation']['kind'],'netting');net=derivative['operation']['request']
                node_records+=sum(len(p['nodes'])*(64+16*len(net['mapping']))for p in net['paths'])
            cell=1024+8*escaped(value)+4*c.native_json_length(value['book']['identity'])
            records=4*(node_records+8*arrays(value));source_bytes=c.native_json_length(value)
            capacity+=records*cell+16*source_bytes;work+=250_000+8*source_bytes+counters(value['physical'],mixed_days,check)['replay_work'];count+=1
            require(work<=MAX_WORK and capacity<=MAX_RETAINED,'joined derivative structural upper exceeds admission')
    deposit=portfolio_admission(raw['portfolio']);staging=2*MAX_STREAM;arena=16*rb
    spec=_normalize(raw['portfolio']['specification']);accounts={a['id']:a for a in spec['accounts']}
    scenarios={r['scenario_id']for r in raw['portfolio']['runs']}
    asof=raw['portfolio']['positions'][0]['paths'][0]['source']['continuation']['original']['as_of_ordinal'];end=asof+spec['horizon_days']
    books=set();buckets=set()
    for owner in raw['derivative_owners']:
        require(owner['account_id']in accounts,'joint derivative account absent')
        a=accounts[owner['account_id']];first=typed(owner['runs'][0]['saved']);key=tuple(first['book']['identity'][k]for k in ('id','revision'))
        require(key not in books,'joint duplicate saved derivative book');books.add(key)
        require(len(owner['runs'])==len(scenarios)and {r['ledger_scenario']for r in owner['runs']}==scenarios,'complete derivative scenario coverage before projection')
        for run in owner['runs']:
            value=typed(run['saved']);net=value['physical']['derivative']['operation']['request']
            require(net['asof_ordinal']==asof and net['policy']['tail_return_ordinal']<=end and
                a['entity']==value['physical']['entity']and a['currency']==value['physical']['currency'],'joint derivative context mismatch')
            exact(float(a['cash']),float(value['physical']['opening']['cash']))
            exact(value['book'],first['book']);exact(value['physical']['opening'],first['physical']['opening'])
        for m in first['physical']['derivative']['operation']['request']['mapping']:
            if m['enforceable_netting']:
                bucket=(owner['account_id'],m['counterparty'],m['netting_set_id'])
                # Multiple trades inside one owner may share the same legal bucket;
                # a different owner cannot divide that immutable source ownership.
                if bucket in buckets:raise ArithmeticError('joint legal netting split across owners')
        buckets.update((owner['account_id'],m['counterparty'],m['netting_set_id'])for m in first['physical']['derivative']['operation']['request']['mapping']if m['enforceable_netting'])
    joined=8*rb+deposit['work_units']+work;retained=arena+deposit['reserved_bytes']+capacity+staging
    require(joined<=MAX_WORK and retained<=MAX_RETAINED,'joined source2/derivative upper preflight refusal')
    return dict(deposit=deposit,derivative_work_upper=work,derivative_retained_upper=capacity,joined_work_upper=joined,joined_retained_upper=retained,stream_and_header_staging=staging,derivative_sources=count,raw_joint_arena_upper=arena)
