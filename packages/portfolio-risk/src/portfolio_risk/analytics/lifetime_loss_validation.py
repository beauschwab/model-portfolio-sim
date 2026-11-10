"""Independent admission and supplied hazard laws for lifetime-loss replay."""
from copy import deepcopy
import math
import sys
import unicodedata

from ._deposit_continuation_income_reference import native_json_length as size
from .modeled_json_identity import exact_json_identity

INTEGERS = {'training_end', 'scaling_training_end', 'availability_lag_days',
    'forecast_as_of_ordinal', 'ordinal', 'seconds_of_day', 'max_jumps_per_path', 'path_key', 'seed'}


def ordered_sum(values):
    total = 0.
    for value in values:
        total += value
    return total


def require(ok, message):
    if not ok:
        raise ValueError('lifetime loss ' + message)


def fields(obj, names):
    require(type(obj) is dict and obj.keys() == set(names.split()), 'fields')


def number(value, low, high):
    require(type(value) in (int, float) and math.isfinite(value) and low <= value <= high, 'numeric domain')


def integer(value, low, high):
    require(type(value) is int and low <= value <= high, 'integer domain')


def label(value):
    require(type(value) is str and value.strip() and len(value.encode()) <= 256
            and not any(unicodedata.category(c) == 'Cc' for c in value), 'identity')


def vector(values, count, low, high):
    require(type(values) is list and len(values) == count, 'vector dimension')
    for v in values:
        number(v, low, high)


def typed(value, key=''):
    if type(value) is dict:
        return {k: typed(v, k) for k, v in value.items()}
    if type(value) is list:
        return [typed(v, key) for v in value]
    if type(value) is int and key not in INTEGERS:
        return float(value)
    return value


def hazards(rows):
    require(type(rows) is list and len(rows) == 3, 'origin dimension')
    for i, row in enumerate(rows):
        vector(row, 5, 0., 100.)
        require(row[i] == 0. and ordered_sum(row) <= 100., 'hazard row')


def path_label(value):
    # Preserve the selected core path contract's control-free byte identity;
    # outer report labels additionally require non-whitespace content.
    require(type(value) is str and 0 < len(value.encode()) <= 256
            and not any(unicodedata.category(c) == 'Cc' for c in value), 'path identity')


def path_validate(p, check):
    fields(p, 'schema measure monetary_unit source_id source_revision model_policy_id origin_time max_jumps_per_path intervals boundaries draw_stream paths')
    require(p['schema'] == 'credit-path-1' and p['measure'] == 'physical', 'Physical path schema')
    for key in ('monetary_unit', 'source_id', 'source_revision', 'model_policy_id'):
        path_label(p[key])
    fields(p['origin_time'], 'ordinal seconds_of_day')
    integer(p['origin_time']['ordinal'], 1, 3652059)
    integer(p['origin_time']['seconds_of_day'], 0, 86399)
    integer(p['max_jumps_per_path'], 1, 128)
    require(1 <= len(p['paths']) <= 10000 and 1 <= len(p['intervals']) <= 128
            and len(p['boundaries']) <= 128 and len(p['paths']) * p['max_jumps_per_path'] <= 250000, 'path count')
    previous = 0.
    for interval in p['intervals']:
        check()
        fields(interval, 'end_year hazards_per_year accrual_rates loss_given_default credit_conversion_factor interest_recovery_fraction complete_recovery_tails')
        number(interval['end_year'], 0., 100.)
        require(interval['end_year'] > previous, 'interval order')
        previous = interval['end_year']
        hazards(interval['hazards_per_year'])
        vector(interval['accrual_rates'], 3, -100., 100.)
        for key in ('loss_given_default', 'credit_conversion_factor', 'interest_recovery_fraction'):
            vector(interval[key], 3, 0., 1.)
        require(len(interval['complete_recovery_tails']) == 3, 'recovery origins')
        for tail in interval['complete_recovery_tails']:
            require(1 <= len(tail) <= 32, 'complete recovery count')
            lag, weight = -1., 0.
            for point in tail:
                fields(point, 'lag_years weight')
                number(point['lag_years'], 0., 100.); number(point['weight'], 0., 1.)
                require(point['lag_years'] > lag, 'recovery order')
                lag = point['lag_years']; weight += point['weight']
            require(weight <= 1. and abs(weight-1.) <= 1e-12, 'complete recovery weights')
    previous, ids = -1., set()
    for boundary in p['boundaries']:
        check(); fields(boundary, 'event_id policy_id year action')
        path_label(boundary['event_id']); path_label(boundary['policy_id'])
        number(boundary['year'], 0., p['intervals'][-1]['end_year'])
        require(boundary['year'] >= previous and boundary['event_id'] not in ids, 'boundary order/identity')
        previous = boundary['year']; ids.add(boundary['event_id'])
        action = boundary['action']; kind = action['kind']
        names = {'maturity': '', 'draw': 'undrawn_fraction',
                 'repayment': 'principal_fraction interest_fraction cancel_undrawn_fraction'}
        require(kind in names, 'boundary kind'); fields(action, 'kind ' + names[kind])
        for key in names[kind].split():
            vector(action[key], 3, 0., 1.)
    ids, keys = set(), set()
    for opening in p['paths']:
        check(); fields(opening, 'path_id path_key initial_live_state principal undrawn_commitment accrued_interest book_adjustment')
        path_label(opening['path_id']); integer(opening['path_key'], 0, 2**64-1)
        require(opening['initial_live_state'] in ('performing', 'watch', 'delinquent'), 'opening live state')
        require(opening['path_id'] not in ids and opening['path_key'] not in keys, 'path uniqueness')
        ids.add(opening['path_id']); keys.add(opening['path_key'])
        for key in ('principal', 'undrawn_commitment', 'accrued_interest', 'book_adjustment'):
            number(opening[key], 0. if key in ('principal', 'undrawn_commitment') else -1e15, 1e15)
        require(sum(abs(opening[k]) for k in ('principal', 'undrawn_commitment', 'accrued_interest', 'book_adjustment')) <= 1e15, 'gross opening amount')
    stream = p['draw_stream']; kind = stream['kind']
    require(kind in ('supplied', 'counter_splitmix64_v1'), 'draw stream')
    fields(stream, 'kind stream_id stream_revision ' + ('paths' if kind == 'supplied' else 'seed'))
    path_label(stream['stream_id']); path_label(stream['stream_revision'])
    if kind == 'counter_splitmix64_v1':
        integer(stream['seed'], 0, 2**64-1)
    else:
        require(len(stream['paths']) == len(keys), 'draw coverage')
        seen, count = set(), 0
        for row in stream['paths']:
            fields(row, 'path_key pairs'); integer(row['path_key'], 0, 2**64-1)
            require(row['path_key'] in keys and row['path_key'] not in seen, 'draw key')
            seen.add(row['path_key']); count += len(row['pairs'])
            require(count <= 260000, 'draw admission')
            for pair in row['pairs']:
                fields(pair, 'waiting_uniform destination_uniform')
                for value in pair.values():
                    number(value, 0., 1.); require(0. < value < 1., 'strict uniform')


def source_for(cohort, model, scenario, drivers, check):
    source = deepcopy(cohort['source'])
    for index, interval in enumerate(source['intervals']):
        check()
        if model['kind'] == 'constant_row':
            interval['hazards_per_year'] = deepcopy(model['hazards_per_year'])
        elif model['kind'] == 'fixed_row_assumption':
            interval['hazards_per_year'] = deepcopy(model['hazards_per_year'][index])
        else:
            for i, row in enumerate(interval['hazards_per_year']):
                for j, baseline in enumerate(row):
                    beta = model['beta'][i*5+j]
                    if baseline == 0.:
                        require(not any(beta), 'zero baseline coefficient')
                    elif any(beta):
                        exponent = ordered_sum(b*((v-d['offset'])/d['scale']) for b,v,d in
                                       zip(beta, scenario['lag_aligned_values'][index], drivers))
                        try:
                            row[j] *= math.exp(exponent)
                        except OverflowError as error:
                            raise ValueError('lifetime loss hazard overflow') from error
                        number(row[j], sys.float_info.min, 100.)
        hazards(interval['hazards_per_year'])
    return source


def validate(r, check):
    fields(r, 'schema model_version source_id source_revision data_kind training_split_id holdout_split_id training_end assumptions drivers base_scenario_id scenarios cohorts outcomes')
    require(r['schema'] == r['model_version'] == 'lifetime-credit-loss-1', 'schema/version')
    require(r['data_kind'] in ('synthetic', 'observed'), 'data kind')
    for key in ('source_id', 'source_revision', 'training_split_id', 'holdout_split_id', 'base_scenario_id'):
        label(r[key])
    integer(r['training_end'], 1, 3652059)
    require(r['training_split_id'] != r['holdout_split_id'] and 1 <= len(r['assumptions']) <= 64, 'split/assumptions')
    for assumption in r['assumptions']:
        label(assumption)
    require(1 <= len(r['drivers']) <= 16 and 1 <= len(r['scenarios']) <= 8 and 1 <= len(r['cohorts']) <= 16, 'counts')
    seen = set()
    for d in r['drivers']:
        fields(d, 'driver_id unit scaling_source_id scaling_training_end availability_lag_days offset scale minimum maximum')
        for key in ('driver_id', 'unit', 'scaling_source_id'):
            label(d[key])
        require(d['driver_id'] not in seen, 'driver uniqueness'); seen.add(d['driver_id'])
        integer(d['scaling_training_end'], 1, r['training_end']); integer(d['availability_lag_days'], 0, 36500)
        number(d['offset'], -1e12, 1e12); number(d['scale'], sys.float_info.min, 1e12)
        number(d['minimum'], -1e12, 1e12); number(d['maximum'], d['minimum'], 1e12)
    first, seen = r['cohorts'][0]['source'], set()
    for s in r['scenarios']:
        fields(s, 'scenario_id source_id source_revision forecast_as_of_ordinal lag_aligned_values')
        for key in ('scenario_id', 'source_id', 'source_revision'):
            label(s[key])
        require(s['scenario_id'] not in seen, 'scenario uniqueness'); seen.add(s['scenario_id'])
        integer(s['forecast_as_of_ordinal'], 1, first['origin_time']['ordinal'])
        require(len(s['lag_aligned_values']) == len(first['intervals']), 'scenario grid')
        for values in s['lag_aligned_values']:
            require(len(values) == len(r['drivers']), 'scenario drivers')
            for value, d in zip(values, r['drivers']):
                number(value, d['minimum'], d['maximum'])
    require(r['base_scenario_id'] in seen, 'base scenario')
    raw_size = size(r); require(raw_size <= 2 << 20, 'raw byte admission')
    work, output, seen = raw_size, 4*raw_size, set()
    for c in r['cohorts']:
        check(); fields(c, 'cohort_id product vintage monetary_multiplier probability_weights source models')
        label(c['cohort_id']); label(c['vintage'])
        require(c['cohort_id'] not in seen, 'cohort uniqueness'); seen.add(c['cohort_id'])
        require(c['product'] in ('auto', 'personal', 'credit_card') and len(c['models']) == 3, 'product/model count')
        p = c['source']; path_validate(p, check)
        require(p['origin_time']['ordinal'] > r['training_end'] and p['origin_time'] == first['origin_time']
                and p['monetary_unit'] == first['monetary_unit'] and exact_json_identity(
                    [i['end_year'] for i in p['intervals']], [i['end_year'] for i in first['intervals']]), 'paired Physical grid')
        number(c['monetary_multiplier'], 0., 1e12); vector(c['probability_weights'], len(p['paths']), 0., 1.)
        total = ordered_sum(c['probability_weights'])
        require(total <= 1. and abs(total-1.) <= 1e-12, 'complete probability weights')
        kinds, ids = set(), set()
        for m in c['models']:
            kind = m['kind']; names = {'constant_row':'hazards_per_year',
                'supplied_covariate':'coefficient_source_id coefficient_source_revision training_end beta',
                'fixed_row_assumption':'assumption_id hazards_per_year'}
            require(kind in names and kind not in kinds and m['model_id'] not in ids, 'model choices')
            kinds.add(kind); ids.add(m['model_id']); fields(m, 'kind model_id model_version '+names[kind])
            label(m['model_id']); label(m['model_version'])
            if kind == 'supplied_covariate':
                label(m['coefficient_source_id']); label(m['coefficient_source_revision'])
                integer(m['training_end'], 1, r['training_end']); require(len(m['beta']) == 15, 'coefficient origins')
                for row in m['beta']:
                    vector(row, len(r['drivers']), -100., 100.)
            elif kind == 'fixed_row_assumption':
                label(m['assumption_id']); require(m['assumption_id'] in r['assumptions']
                    and len(m['hazards_per_year']) == len(p['intervals']), 'fixed assumptions/grid')
        recoveries = max(len(t) for i in p['intervals'] for t in i['complete_recovery_tails'])
        path_bytes = 4096 + (p['max_jumps_per_path'] + len(p['boundaries']) + recoveries + 1)*2048
        calls = len(p['paths'])*len(c['models'])*len(r['scenarios']); source_bytes = size(p)
        require(source_bytes <= 1 << 20, 'path source byte admission')
        output += calls*path_bytes
        work += calls*(3*source_bytes+path_bytes) + len(c['models'])*len(r['scenarios'])*len(p['intervals'])*len(r['drivers'])*64
    require(work <= 32000000 and output <= 32 << 20, 'aggregate work/output admission')
    require(len(r['outcomes']) <= len(r['cohorts']), 'outcome count')
    seen = set()
    for o in r['outcomes']:
        fields(o, 'cohort_id vintage source_id source_revision monetary_unit observed_through_year reported_principal_loss reported_signed_interest_loss')
        label(o['source_id']); label(o['source_revision'])
        cohorts = [c for c in r['cohorts'] if c['cohort_id'] == o['cohort_id']]
        require(len(cohorts) == 1 and o['cohort_id'] not in seen, 'outcome cohort'); seen.add(o['cohort_id'])
        c = cohorts[0]; p = c['source']
        require(o['vintage'] == c['vintage'] and o['monetary_unit'] == p['monetary_unit'], 'outcome vintage/unit')
        end = p['intervals'][-1]['end_year'] + max(v['lag_years'] for i in p['intervals'] for t in i['complete_recovery_tails'] for v in t)
        number(o['observed_through_year'], end, 200.)
        number(o['reported_principal_loss'], 0., 1e15); number(o['reported_signed_interest_loss'], -1e15, 1e15)
    return work, output
