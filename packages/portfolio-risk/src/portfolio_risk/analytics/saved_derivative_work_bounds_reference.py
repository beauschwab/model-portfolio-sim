"""Independent raw journal-branch cardinality upper, without projection.

The raw Physical source owns financial postings. Optional Pricing XVA diagnostics
retain the separate raw/source reservation and never create financial GL lines.
"""
from .saved_derivative_identity import probe
MAX_USIZE=(1<<64)-1

def checked(value):
    if type(value)is not int or not 0<=value<=MAX_USIZE:raise ArithmeticError('checked structural work overflow')
    return value

def swap_cash(request):return checked(len(request['fixed_periods'])+len(request['floating_periods']))

def option_cash(request):
    return checked(1+len(request['states'])+sum(swap_cash(state['underlying'])for state in request['states']))

def cash_slots(trade):
    kind=trade['kind']
    if kind=='swap':return swap_cash(trade['request'])
    if kind=='european_swaption':return option_cash(trade['request'])
    if kind=='expired_european':
        return checked(option_cash(trade['terminal'])+(0 if trade.get('delivered_underlying')is None else swap_cash(trade['delivered_underlying'])))
    raise ArithmeticError('unknown declared raw trade domain')

def counters(physical,mixed_days,check=lambda:False):
    checked(mixed_days)
    if mixed_days==0:raise ArithmeticError('complete mixed days include asof')
    net=physical['derivative']['operation']['request'];path=net['paths'][0]
    legal={(m['enforceable_netting'],m['netting_set_id']if m['enforceable_netting']else m['position_id'])for m in net['mapping']}
    nodes=len(path['nodes']);trades=rights=0
    for node in path['nodes']:
        probe(check);trades=checked(trades+len(node['trades']))
        for trade in node['trades']:probe(check);rights=checked(rights+cash_slots(trade))
    opening=checked(7+2*len(physical['opening']['trades']))
    # Two three-line marks plus four-line delivery per node/trade. Due3,
    # eventual MPOR settlement2 and default unpaid retirement1 per origin.
    # GLOBAL collateral: at most N*B calls, each settles once despite bursts;
    # interest<=7*N*B and tail<=6*B, together <=20*N*B. Closeout8*B+2*T.
    lines=checked(opening+10*trades+20*nodes*len(legal)+8*len(legal)+2*len(physical['opening']['trades'])+6*rights)
    scans=checked(lines*checked(1+checked(6*mixed_days))+16*nodes)
    return dict(opening_lines=opening,physical_nodes=nodes,node_trade_slots=trades,
        legal_buckets=len(legal),cash_slots=rights,line_upper=lines,replay_work=scans)


def standalone_upper(owners,mixed_days,dated_work_units,check=lambda:False,max_work=8000000):
    from .saved_derivative_mapping_reference import validate_mapping,require
    from .saved_derivative_financial_reference import _admit
    from .saved_derivative_dto_reference import saved_dto
    work=checked(dated_work_units)
    for owner in owners:
        for run in owner['runs']:
            probe(check);validate_mapping(run['saved']);physical=saved_dto(run['saved'])['physical'];_admit(physical)
            work=checked(work+250000+counters(physical,mixed_days,check)['replay_work'])
    require(work<=max_work,'joined standalone structural work preflight')
    return work
