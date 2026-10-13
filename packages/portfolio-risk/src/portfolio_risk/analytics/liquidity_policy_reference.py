"""Independent policy checks for publication replay of native dated reports.

Amounts and ratios are supplied by the independent journal replay, never by the
producer. These helpers cannot generate cashflows, post a journal or price a book.
"""
import math


def basis(account, metric):
    floor = account[metric + '_floor']
    selection = account.get(metric + '_applicability', '')
    if type(selection) is not str:
        raise ArithmeticError('liquidity applicability must be a string')
    selected = selection or ('internal_limit' if floor > 0 else 'unconfigured')
    if selected not in ('internal_limit', 'not_applicable', 'unconfigured') or ((selected == 'internal_limit') != (floor > 0)):
        raise ArithmeticError('liquidity applicability/floor differs')
    return selected


def validate(account):
    for metric in ('lcr', 'nsfr'):
        basis(account, metric)
    policy = account.get('htm_accounting', '')
    materiality = account.get('htm_taint_materiality', 0.)
    if (type(policy) is not str or policy not in ('', 'asc320-htm-v1')
            or type(materiality) not in (int, float) or not math.isfinite(materiality)
            or not 0 <= materiality <= 1 or (materiality > 0 and not policy)):
        raise ArithmeticError('HTM policy domain differs')


def apply_path(row, account):
    for metric in ('lcr', 'nsfr'):
        selected = basis(account, metric)
        margin = row[metric + '_headroom']
        if selected != 'internal_limit':
            status = selected
            row[metric + '_headroom'] = None
        elif margin < -1e-9:
            status = 'breach'
        elif row[metric + '_proxy'] is None:
            status = 'unavailable'
        else:
            status = 'within_limit'
        row[metric + '_status'] = status


def apply_summary(summary, account, rows):
    # These replay routes reject legacy sale policies, so no reclassification
    # event can occur. A future sale owner needs its own independent replay.
    summary['htm_reclassification_day'] = None
    ranks = {'within_limit': 0, 'unavailable': 1, 'breach': 2}
    for metric in ('lcr', 'nsfr'):
        selected = basis(account, metric)
        summary[metric + '_applicability'] = selected
        summary[metric + '_status'] = (max((row[metric + '_status'] for row in rows), key=ranks.__getitem__)
                                         if selected == 'internal_limit' else selected)


def accumulate(worst, row):
    ranks = {'within_limit': 0, 'unavailable': 1, 'breach': 2,
             'unconfigured': 0, 'not_applicable': 0}
    for metric in ('lcr', 'nsfr'):
        key = metric + '_status'
        if key not in worst or ranks[row[key]] > ranks[worst[key]]:
            worst[key] = row[key]
