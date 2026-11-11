"""Independent raw prefix and journal window controls shared by cut readers."""
from collections import defaultdict
from copy import deepcopy
import math
from .commercial_candidate_holding_reference import raw_prefix as survivor_prefix
from .commercial_dto import graph_typed
from .modeled_json_identity import exact_json_identity


def same(a, b):
    if not exact_json_identity(graph_typed(a), graph_typed(b)):
        raise ArithmeticError('cut immutable owner or observed raw prefix differs')


def raw_prefix(raw, check=lambda: False):
    """Validate surviving complete origins, then separately validate new dates."""
    old = raw['completed_candidate']; new = deepcopy(raw['revised_candidate'])
    n = len(old['commercial']['units']); cut = raw['cut_ordinal']
    if min(len(new['commercial']['units']), len(new['owners']), len(new['allocation']),
           len(new['specification']['positions'])) < n:
        raise ArithmeticError('cut cannot remove surviving commercial owners')
    same(old['as_of_ordinal'], new['as_of_ordinal'])
    for a, b in [(old['commercial'], new['commercial'])] + [
            (a['graph'], b['graph']) for a, b in zip(
                old['commercial']['units'], new['commercial']['units'][:n], strict=True)]:
        check()
        same([d for d in a['reporting_ordinals'] if d <= cut],
             [d for d in b['reporting_ordinals'] if d <= cut])
        b['reporting_ordinals'] = deepcopy(a['reporting_ordinals'])
    appended = new['commercial']['units'][n:]
    if any(u['graph']['as_of_ordinal'] <= cut for u in appended):
        raise ArithmeticError('cut appended actual acquisition must follow cut')
    new['commercial']['units'] = new['commercial']['units'][:n]
    for key in ('owners', 'allocation'):
        new[key] = new[key][:n]
    new['specification']['positions'] = new['specification']['positions'][:n]
    new['reporting_end_ordinal'] = old['reporting_end_ordinal']
    if 'horizon_days' in old['specification']:
        new['specification']['horizon_days'] = old['specification']['horizon_days']
    survivor_prefix(dict(completed_candidate=old, revised_candidate=new, cut_ordinal=cut), check)
    return len(appended)


def _observed_prefix(old, new, cut, survivor_ids, check):
    """Compare unweighted native endpoints, even when allocation is zero."""
    for name in set(old) | set(new):
        if name in ('summary', 'trial_balance', 'closing_statements', 'consolidated', 'attribution'):
            continue
        def rows(tables):
            result = []
            for row in tables.get(name, []):
                if type(row.get('day')) is not int or row['day'] > cut:
                    continue
                position = row.get('position_id', row.get('position', row.get('instrument_id', '')))
                if position and position not in survivor_ids:
                    # Future candidates' generic zero exposure placeholders are
                    # not surviving financial owners or money at the cut.
                    if name != 'exposures' or any(row.get(k, 0.) != 0. for k in (
                            'balance', 'allowance', 'commitment', 'accrued_interest')):
                        raise ArithmeticError('future candidate has observed money/source groups')
                    continue
                result.append(row)
            return result
        check()
        if not exact_json_identity(rows(old), rows(new)):
            raise ArithmeticError('cut exact ordered unweighted observed monetary prefix differs/' + name)


def _window(full, specification, cut, check):
    """Retain actual full-origin counters, create signed opening, filter future.

    P&L stock remains in its existing GL, and original transaction numbers
    survive. The resumed Sim's accounting history is not a new acquisition.
    """
    balances = defaultdict(float)
    for row in full['journal']:
        check()
        if type(row.get('value')) is not float or not math.isfinite(row['value']):
            raise ArithmeticError('cut raw journal scalar differs')
        if row['day'] <= cut:
            balances[tuple(row[k] for k in ('scenario', 'account', 'gl_account', 'instrument_id'))] += row['value']
    result = {name: [] for name in full}
    scenarios = ['baseline'] + [s['name'] for s in specification['scenarios']]
    for scenario in scenarios:
        opening = [(key, value) for key, value in sorted(balances.items()) if key[0] == scenario]
        for (run, account, gl, instrument), value in opening:
            result['journal'].append(dict(transaction_number=0, scenario=run, day=cut,
                account=account, event='native_verified_cut_opening', gl_account=gl,
                instrument_id=instrument, value=value))
        result['journal'].extend(deepcopy(r) for r in full['journal']
                                 if r['scenario'] == scenario and r['day'] > cut)
    snapshots = {'path', 'exposures', 'commercial_daily', 'commercial_closing',
                 'commercial_fee_daily', 'commercial_fee_closing'}
    terminal = {'summary', 'trial_balance', 'closing_statements', 'consolidated', 'reverse_grid'}
    for name, rows in full.items():
        if name in terminal:
            result[name] = deepcopy(rows)
        elif name in snapshots:
            result[name] = [deepcopy(r) for r in rows if r['day'] >= cut]
        elif name not in ('journal', 'attribution'):
            result[name] = [deepcopy(r) for r in rows if type(r.get('day')) is int and r['day'] > cut]
    totals = defaultdict(lambda: [0., 0., 0.])
    for row in result['ledger']:
        check()
        for i, k in enumerate(('cash', 'earnings', 'aoci')):
            totals[row['scenario'], row['account'], row['event']][i] += row[k]
    keys = sorted({(a, e) for _, a, e in totals})
    for scenario in scenarios[1:]:
        for account, event in keys:
            base = totals['baseline', account, event]; other = totals[scenario, account, event]
            result['attribution'].append(dict(scenario=scenario, account=account, event=event,
                cash_delta=other[0]-base[0], earnings_delta=other[1]-base[1], aoci_delta=other[2]-base[2]))
    return result


