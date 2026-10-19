"""Synthetic fixtures for the remaining ASU 2025-09 interest-rate items in hedge-accounting-1.

Built from the reviewed example generator so curves, observations and par rates stay consistent:

- `hedge-cyr-tenor.json`: existing choose-your-rate debt elects, on 1 March, a documented option whose
  tenor splits March into two payments (15 March and 1 April) (815-30-35-37I).
- `hedge-cyr-issuance.json`: a forward-starting swap designated 1 January hedges the forecasted
  issuance of choose-your-rate debt on 1 March; the best estimate changes on 1 February and the debt is
  issued on 1 March at that rate (815-30-35-37C through 35-37H).
- `hedge-cyr-replacement.json`: existing choose-your-rate debt is replaced on 1 March by debt accruing at
  another documented option (815-30-35-37K).
- `hedge-qualitative-similarity.json`: a similar-risk group designated 1 January is assessed
  quantitatively on 1 February and qualitatively on 1 March (815-20-55-23C).

    uv run --project packages/portfolio-risk python scripts/make_hedge_asu_followthrough_examples.py
"""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'packages/portfolio-risk-native/tests/fixtures'


def module():
    spec = importlib.util.spec_from_file_location('hedge_example', ROOT / 'scripts/make_hedge_accounting_example.py')
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def ident(s):
    return dict(id=s, revision='r1')


def adopt(v, election, start):
    v['policy']['adoption']['asu_2025_09'] = dict(adopted=True, adoption_ordinal=start, elections=dict(
        similar_risk_grouping=election == 'similar_risk_grouping', selectable_rate_debt=election == 'selectable_rate_debt',
        swap_written_option_exception=False, dual_hedge_basis=False))


def reindex(request, index, feb1, march=None):
    """Rename the floating index and, optionally, replace period 3's projection from 1 February and its fixing."""
    for p in request['floating_periods']:
        p['coupon']['index']['index_id'] = index
        if march is not None and p['period_id'] == 'l3':
            for o in p['coupon']['observations']:
                if o['source']['kind'] == 'observed' or request['asof_ordinal'] >= feb1:
                    o['rate'] = march
    return request


def hypothetical(m, position, periods, fixed, start, notional=100., index='term-1m', march=None, last=None):
    nodes = []
    for t in range(start, m.LAST + 1):
        q = m.swap(t, position, periods, fixed, m.LAST if last is None else last)['request']
        q['notional'] = notional
        nodes.append(dict(ordinal=t, request=reindex(q, index, m.FEB1, march)))
    return dict(evidence=ident(position + '-evidence'), nodes=nodes)


def par(m, periods, rates, at):
    """Par fixed rate at `at` for (pid, start, end) periods with supplied rates, on the node curve."""
    rate = .06 if at < m.FEB1 else .04
    w = [((e - s) / 360.) * m.df(rate, e - at) for _, s, e in periods if e > at]
    r = [rates[pid] for pid, s, e in periods if e > at]
    return sum(a * b for a, b in zip(r, w)) / sum(w)


def assess(rid, ordinal, sequence, **extra):
    return dict(event_id=f'assess-{rid}-{ordinal}', relationship_id=rid, ordinal=ordinal, sequence=sequence, kind='assess',
                review=ident('assessment-' + rid), **extra)


def tenor():
    m = module()
    MAR15 = m.MAR1 + 14
    original = m.curve

    def curve(t, last=m.APR1):
        c = original(t, last)
        rate = .06 if t < m.FEB1 else .04
        if t < MAR15 <= last:
            c['points'].append(dict(ordinal=MAR15, discount_factor=m.df(rate, MAR15 - t)))
            c['points'].sort(key=lambda p: p['ordinal'])
        return c
    m.curve = curve
    m.OBSERVED.update({'3a': .045, '3b': .045})
    v = m.request()
    adopt(v, 'selectable_rate_debt', m.JAN1)
    two_week = [('1', m.JAN1, m.FEB1), ('2', m.FEB1, m.MAR1), ('3a', m.MAR1, MAR15), ('3b', MAR15, m.APR1)]
    k = m.FIXED['swap-cfh']
    k_w = par(m, [p for p in two_week if p[1] >= m.FEB1], {'2': .04, '3a': .045, '3b': .045}, m.FEB1)
    x = v['forecast_exposures'][0]
    x['choose_your_rate'] = dict(evidence=ident('cyr-terms'), documented_index_ids=['term-1m', 'term-2w'], debt_classification='liability',
                                 option_schedules=[dict(index_id='term-2w', periods=m.periods('e', two_week))])
    rel = v['relationships'][1]
    rel.update(matched_critical_terms_evidence=None, hypothetical=hypothetical(m, 'hyp-base', m.PERIODS, k, m.FEB1),
               rate_hypotheticals=[dict(index_id='term-2w', hypothetical=hypothetical(m, 'hyp-2w', two_week, k_w, m.FEB1, index='term-2w'))])
    v['events'].append(dict(event_id='elect-2w', relationship_id='cfh-1', ordinal=m.MAR1, sequence=2, kind='rate_election',
                            review=ident('election-review'), index_id='term-2w'))
    return v, dict(k=k, k_w=k_w)


def forward_variant(m, v):
    """Add a 6% forward-starting payer swap on period 3 designated from 1 January (the issuance hedge)."""
    csa = v['derivative']['derivative']['operation']['request']
    for n in csa['paths'][0]['nodes']:
        fwd = deepcopy(n['trades'][1]); r = fwd['request']
        r.update(position_id='swap-fwd', contract=ident('swap-fwd-contract'), fixed_rate=.06,
                 fixed_periods=r['fixed_periods'][2:], floating_periods=r['floating_periods'][2:])
        n['trades'].append(fwd)
    mapping = deepcopy(csa['mapping'][1]); mapping.update(position_id='swap-fwd', contract=ident('swap-fwd-contract')); csa['mapping'].append(mapping)
    review = deepcopy(v['derivative']['policy']['trades'][1]); review['position_id'] = 'swap-fwd'; v['derivative']['policy']['trades'].append(review)
    v['derivative']['opening']['trades'].append(dict(position_id='swap-fwd', clean_value=0.))
    for p in [p for p in v['derivative']['physical_index_paths'] if p['position_id'] == 'swap-cfh' and p['period_id'] == 'l3']:
        q = deepcopy(p); q['position_id'] = 'swap-fwd'; v['derivative']['physical_index_paths'].append(q)
    return csa


def issuance():
    m = module()
    v = m.request()
    adopt(v, 'selectable_rate_debt', m.JAN1)
    forward_variant(m, v)
    exposure = deepcopy(v['forecast_exposures'][0])
    exposure.update(exposure_id='issue-1', periods=exposure['periods'][2:],
                    choose_your_rate=dict(evidence=ident('cyr-offerings'), documented_index_ids=['term-1m', 'term-1m-alt'],
                                          debt_classification='liability', forecasted_issuance=ident('issuance-probable')))
    v['forecast_exposures'].append(exposure)
    period3 = [m.PERIODS[2]]
    base = hypothetical(m, 'hyp-fwd', period3, .06, m.JAN1)
    alt = hypothetical(m, 'hyp-fwd-alt', period3, .06, m.JAN1, index='term-1m-alt', march=.046)
    v['relationships'].append(dict(deepcopy(v['relationships'][1]), relationship_id='cfh-fwd', hedging_instrument_position_id='swap-fwd',
                                   forecast_exposure_id='issue-1', inception_ordinal=m.JAN1, matched_critical_terms_evidence=None,
                                   hypothetical=base, rate_hypotheticals=[dict(index_id='term-1m-alt', hypothetical=alt)],
                                   risk_management_objective=ident('cfh-fwd-objective'), documentation=ident('cfh-fwd-documentation')))
    v['events'] += [dict(event_id='best-estimate', relationship_id='cfh-fwd', ordinal=m.FEB1, sequence=1, kind='rate_election',
                         review=ident('best-estimate-review'), index_id='term-1m-alt'),
                    dict(event_id='issued', relationship_id='cfh-fwd', ordinal=m.MAR1, sequence=2, kind='debt_issued',
                         review=ident('issuance-review'), index_id='term-1m-alt')]
    v['events'].sort(key=lambda e: (e['ordinal'], e['sequence']))
    return v, {}


def replacement():
    m = module()
    v = m.request()
    adopt(v, 'selectable_rate_debt', m.JAN1)
    k = m.FIXED['swap-cfh']
    k_alt = par(m, m.PERIODS[1:], {'2': .04, '3': .046}, m.FEB1)
    v['forecast_exposures'][0]['choose_your_rate'] = dict(evidence=ident('cyr-terms'), documented_index_ids=['term-1m', 'term-1m-alt'],
                                                          debt_classification='liability')
    rel = v['relationships'][1]
    rel.update(matched_critical_terms_evidence=None, hypothetical=hypothetical(m, 'hyp-base', m.PERIODS, k, m.FEB1),
               rate_hypotheticals=[dict(index_id='term-1m-alt', hypothetical=hypothetical(m, 'hyp-alt', m.PERIODS, k_alt, m.FEB1,
                                                                                         index='term-1m-alt', march=.046))])
    v['events'].append(dict(event_id='replaced', relationship_id='cfh-1', ordinal=m.MAR1, sequence=2, kind='debt_replacement',
                            review=ident('replacement-debt-review'), index_id='term-1m-alt'))
    return v, dict(k=k, k_alt=k_alt)


def par_swap(m, v, position, fixed):
    """Add a payer swap on the example periods at `fixed` from 1 January (at market there)."""
    csa = v['derivative']['derivative']['operation']['request']
    for n in csa['paths'][0]['nodes']:
        trade = deepcopy(n['trades'][1]); r = trade['request']
        r.update(position_id=position, contract=ident(position + '-contract'), fixed_rate=fixed)
        n['trades'].append(trade)
    mapping = deepcopy(csa['mapping'][1]); mapping.update(position_id=position, contract=ident(position + '-contract')); csa['mapping'].append(mapping)
    review = deepcopy(v['derivative']['policy']['trades'][1]); review['position_id'] = position; v['derivative']['policy']['trades'].append(review)
    v['derivative']['opening']['trades'].append(dict(position_id=position, clean_value=0.))
    for p in [p for p in v['derivative']['physical_index_paths'] if p['position_id'] == 'swap-cfh']:
        q = deepcopy(p); q['position_id'] = position; v['derivative']['physical_index_paths'].append(q)


def qualitative():
    m = module()
    v = m.request()
    adopt(v, 'similar_risk_grouping', m.JAN1)
    rates = {'1': .065, '2': .06, '3': .06}
    fixed = par(m, m.PERIODS, rates, m.JAN1)
    par_swap(m, v, 'swap-grp', fixed)
    v['forecast_exposures'][0]['notional'] = 60.
    second = dict(deepcopy(v['forecast_exposures'][0]), exposure_id='float-2', notional=40., index_id='term-1m-alt')
    v['forecast_exposures'].append(second)
    rel = v['relationships'][1]
    rel.update(forecast_exposure_id=None, matched_critical_terms_evidence=None, inception_ordinal=m.JAN1, hedging_instrument_position_id='swap-grp', group=dict(
        member_exposure_ids=['float-1', 'float-2'], similar_risk_method='instrument_highly_effective_each_risk',
        qualitative_similarity_election=ident('qualitative-similarity-election'),
        hypotheticals=[dict(exposure_id='float-1', hypothetical=hypothetical(m, 'hyp-1', m.PERIODS, fixed, m.JAN1, notional=60.)),
                       dict(exposure_id='float-2', hypothetical=hypothetical(m, 'hyp-2', m.PERIODS, fixed, m.JAN1, notional=40.,
                                                                             index='term-1m-alt', march=.046))]))
    v['events'] = [e for e in v['events'] if e['relationship_id'] != 'cfh-1']
    v['events'] += [assess('cfh-1', m.FEB1, 1), assess('cfh-1', m.MAR1, 1, qualitative_similarity_support=ident('similarity-support'))]
    v['events'].sort(key=lambda e: (e['ordinal'], e['sequence']))
    return v, dict(fixed=fixed)


if __name__ == '__main__':
    OUT.mkdir(parents=True, exist_ok=True)
    for name, build in (('hedge-cyr-tenor', tenor), ('hedge-cyr-issuance', issuance), ('hedge-cyr-replacement', replacement),
                        ('hedge-qualitative-similarity', qualitative)):
        v, facts = build()
        path = OUT / f'{name}.json'
        path.write_text(json.dumps(v, separators=(',', ':')) + '\n')
        print(path.name, path.stat().st_size, 'bytes', facts)
