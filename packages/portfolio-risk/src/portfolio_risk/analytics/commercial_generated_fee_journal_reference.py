"""Bounded independent protocol-seven replay for generated acquired fee paths."""
from collections import defaultdict
import json
from .commercial_ledger_reference import journal_lines
from .commercial_fee_reference import GL
from .commercial_source_reference import compare


def verify(prepared, fee_sources, raw, frames, storage, check=lambda: False):
    positions = {p.id: p for p in prepared['spec']['positions']}
    allocations = {a['position_id']: a['allocated_face'] for a in raw['allocations']}
    weights = {(p['scenario'], p['position_id'], p['path_key']): p['source_weight'] for p in raw['participations']}
    events = storage.sequence(events=True)
    daily = storage.sequence(events=True)
    fee_daily = storage.sequence(events=True)
    binding = fee_binding = 0
    # A unique packed binding key preserves native kind/binding/group ordering.
    # Source count is structurally bounded; kind precedes the full binding index.
    stride = len(prepared['sources']) + 1
    for source, path_fees in zip(prepared['sources'], fee_sources, strict=True):
        check()
        current = source['raw']['source']['current']
        scenario, position = current['scenario_id'], current['position_id']
        origin = source.get('origin_day', 0)
        source_class = source.get('source_class', 'commercial_loan-1')
        allocation = allocations[position] / current['opening_principal']
        for path, fee in zip(source['raw']['credit']['paths'], path_fees, strict=True):
            key = (scenario, position, path['path_key'])
            weight = weights[key]
            factor = allocation * weight
            tracking = ({}, {})
            by_day = storage.snapshots()
            for index, group in enumerate(source['groups']):
                check()
                if group['path_key'] != path['path_key']:
                    continue
                year = group['year']
                day = origin + elapsed(year)
                lines = journal_lines(group, tracking, factor)
                meta = dict(scenario=scenario, day=day, native_credit_year=year,
                    source_origin_day=origin, source_class=source_class,
                    global_native_credit_year=origin / 365. + year,
                    date_policy='actual_credit_clock_order_day_end_journal_no_retiming_source-1',
                    position_id=position, path_key=path['path_key'], source_weight=float(weight),
                    allocation_factor=float(allocation), native_group_index=index, native_atomic_group=group)
                events.append((scenario, day, origin / 365. + year, binding, index, (0, meta), lines))
                by_day[day] = group['next']
            latest = None
            for day in range(prepared['horizon'] + 1):
                check()
                if day in by_day:
                    latest = by_day[day]
                if latest is not None:
                    row = dict(scenario=scenario, day=day, position_id=position,
                        path_key=path['path_key'], source_weight=float(weight), allocation_factor=float(allocation),
                        source_origin_day=origin, source_class=source_class, unweighted_book=latest, capital_ftp_eligible=False)
                    daily.append((scenario, day, 0., binding, 0, row))
            if fee is None:
                raise ArithmeticError('generated paired Physical fee source absent')
            snapshots = storage.snapshots()
            states = {}
            for index, (year, group) in enumerate(zip(fee['native_years'], fee['source']['groups'], strict=True)):
                check()
                day = origin + elapsed(year)
                lines = [(GL[p['account']], p['amount'] * factor) for p in group['postings'] if p['amount'] * factor != 0.]
                meta = dict(scenario=scenario, day=day, position_id=position, path_key=path['path_key'],
                    native_year=year, source_weight=float(weight), allocation_factor=float(allocation),
                    native_group_index=index, native_fee_group=group)
                events.append((scenario, day, origin / 365. + year, stride + fee_binding, index, (1, meta), lines))
                states[group['event']['fee_id']] = group['next']
                snapshots[day] = dict(states)
            states = {}
            for day in range(prepared['horizon'] + 1):
                check()
                if day in snapshots:
                    states = snapshots[day]
                for index, fee_id in enumerate(sorted(states)):
                    row = dict(scenario=scenario, day=day, position_id=position, path_key=path['path_key'],
                        fee_id=fee_id, source_weight=float(weight), allocation_factor=float(allocation), native_fee_snapshot=states[fee_id])
                    fee_daily.append((scenario, day, 0., binding, index, row))
            binding += 1
            fee_binding += 1
    loan_lineage, fee_lineage, converted = storage.sequence(), storage.sequence(), storage.sequence(events=True)
    sequences = defaultdict(lambda: sum(a.cash != 0. or a.equity != 0. for a in prepared['spec']['accounts']))
    for scenario, day, year, sort_binding, index, kind_meta, lines in events:
        check()
        kind, meta = kind_meta
        if lines:
            sequences[scenario] += 1
        meta['transaction_number'] = sequences[scenario] if lines else None
        (fee_lineage if kind else loan_lineage).append(meta)
        financial = dict(position_id=meta['position_id'], native_atomic_group=dict(
            phase='commercial_nonintegral_fee' if kind else meta['native_atomic_group']['phase']))
        converted.append((scenario, day, year, sort_binding, index, financial, lines))
    for name, wanted in [('commercial_source_lineage', loan_lineage), ('commercial_fee_lineage', fee_lineage)]:
        if name=='commercial_fee_lineage':
            compare_fee_lineage(wanted,restored(frames,name,check),check)
        else:
            storage.compare_rows(wanted,restored(frames,name,check),check)
    for name, wanted in [('commercial_daily', daily), ('commercial_fee_daily', fee_daily)]:
        storage.compare_rows((row[5] for row in wanted), restored(frames, name, check), check)
        storage.compare_rows((row[5] for row in wanted if row[1] == prepared['horizon']),
            restored(frames, name.replace('_daily', '_closing'), check), check)
    # This emits every independent journal line, trial balance, gross statement,
    # cash/equity flow and closing report into strict bounded partition iterators.
    storage.verify_reports(prepared, converted, frames, check)
    return dict(verified=True, journal_replayed=True, persisted_partitions_replayed=True,
        raw_loan_and_fee_recomputed=True, native_credit_event_binding=True,
        single_sim_ordered_journal_replayed=True, gross_fee_rights_reconciled=True,
        complete_tail_reports_replayed=True, loan_carrying_value_changed=False,
        pricing_unit_ftp_adoption=False, empirical_validation=False, accounting_approval=False)


def compare_fee_lineage(expected,received,check=lambda:False):
    """Native fee clocks retain the inherited clock tolerance and calendar.

    A numerically tiny change can cross an exact ACT365 boundary. Monetary
    closeness never permits moving a fee event to a different journal day.
    """
    import itertools
    import math
    missing=object()
    for wanted,actual in itertools.zip_longest(expected,received,fillvalue=missing):
        check()
        if wanted is missing or actual is missing:
            raise ArithmeticError('generated complete fee lineage coverage differs')
        year=actual.get('native_year')
        if type(year)is not float or not math.isfinite(year) or year<0.:
            raise ArithmeticError('generated native fee clock domain differs')
        compare(float(wanted['native_year']),year,path='$.native_credit_year',check=check)
        if elapsed(wanted['native_year'])!=elapsed(year):
            raise ArithmeticError('generated native fee clock cannot change its journal day')
        compare(wanted,actual,check=check)


def elapsed(year):
    import math
    x = year * 365.
    nearest = round(x)
    day = nearest if year == nearest / 365. else math.ceil(x)
    return day + (year > day / 365.)


def restored(frames, name, check):
    for frame in frames.frames(name):
        for row in frame.iter_rows(named=True):
            check()
            for key in list(row):
                if key.endswith('_json'):
                    row[key[:-5]] = json.loads(row.pop(key))
            yield row
