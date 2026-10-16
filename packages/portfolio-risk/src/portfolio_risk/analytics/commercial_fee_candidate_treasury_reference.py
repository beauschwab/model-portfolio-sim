"""Independent acquired loan/fee source, one-Sim journal and treasury gate."""
from copy import deepcopy
from datetime import date
import math
import polars as pl

from .commercial_candidate_reference import verify as verify_candidate
from .commercial_graph_reference import verify_graph
from .commercial_source_reference import compare
from .commercial_treasury_reference import _state, _day, _ftp, _capital
from .modeled_product_funding_reference import prepare, FEE_LANES
from .modeled_json_identity import exact_json_identity as exact

SCHEMA = 'commercial-candidate-fee-treasury-1'


class CapitalFrames:
    def __init__(self, tables): self.tables = tables
    def frames(self, table):
        rows = self.tables.get(table, [])
        if rows: yield pl.DataFrame(rows)


def _funding(raw, unit, owner, native, review, end, check):
    source = native['source_request']; current = source['source']['current']
    start = unit['graph']['as_of_ordinal']; key = native['path_key']
    groups = [(i, g) for i, g in enumerate(native['groups']) if g['path_key'] == key]
    points = [p for p in native['reference']['carry_points'] if p['path_key'] == key]
    bound = native['fee_projection']['bound_path']; fee_groups = bound['source']['groups']
    names = FEE_LANES[18:]; states = []; undrawns = []; at = pi = fi = 0
    snapshot = None; fee_states = {}; undrawn = source['credit'].get('opening_undrawn_commitment', 0.)
    for elapsed in range(end-start+1):
        check()
        while at < len(groups) and _day(groups[at][1]['year']) <= elapsed:
            snapshot = groups[at][1]['next']; at += 1
        while pi < len(points) and _day(points[pi]['year']) <= elapsed:
            undrawn = points[pi]['undrawn_commitment']; pi += 1
        while fi < len(fee_groups) and _day(bound['native_years'][fi]) <= elapsed:
            g = fee_groups[fi]; fee_states[g['event']['fee_id']] = g['next']; fi += 1
        state = _state(snapshot, start+elapsed, undrawn)
        state['channels'].extend(dict(channel=name, amount=0.) for name in names)
        for fee in bound['source']['raw_request']['fees']:
            snapshot_fee = fee_states.get(fee['fee_id'], {})
            for index, name in enumerate(names, 18):
                signed = snapshot_fee.get(name, 0.)
                amount = -signed if name in ('fee_payable','deferred_fee_income','acquired_fee_liability') else signed
                if not math.isfinite(amount) or amount < 0.: raise ArithmeticError('independent gross fee stock sign')
                state['channels'][index]['amount'] += amount
                state['origins'].append(dict(origin_id=f"fee:{fee['fee_id']}:{name}", source_event_id=f"native-fee:{fee['fee_id']}", channel=name, amount=amount))
        states.append(state); undrawns.append(undrawn)
    cash = []; earned = []; opening_cash = 0.
    def accept(identity, ordinal, bank_cash, earnings):
        nonlocal opening_cash
        if ordinal == start:
            opening_cash += bank_cash
            if any(amount != 0. for _, amount in earnings): raise ArithmeticError('opening candidate fee earnings require policy')
        else:
            cash.append(dict(event_id=identity, ordinal=ordinal, kind='source_atomic_cash', bank_cash=bank_cash, funding_change_sequence=None, exclusion_review=deepcopy(review['cash_exclusion_review'])))
            for channel, amount in earnings:
                if amount != 0.: earned.append(dict(event_id=f'{identity}:{channel}', ordinal=ordinal, channel=channel, bank_earnings=amount, review=deepcopy(review['income_review'])))
    for index, group in groups:
        earnings = [(channel, -sum((p['posting']['amount'] for p in group['postings'] if p['posting']['account'] == account), 0.)) for channel, account in [('original_eir','interest_income'),('reviewed_basis','basis_recognition_expense'),('incurred_credit','credit_loss_expense'),('recovery_income','recovery_income')]]
        accept(f'loan:{index}', start+_day(group['year']), group['cash'], earnings)
    for index, group in enumerate(fee_groups):
        earnings = [(channel, -sum((p['amount'] for p in group['postings'] if p['account'] == account), 0.)) for channel, account in [('service_fee','fee_income'),('fee_cost_release','fee_acquisition_expense'),('fee_writeoff','fee_writeoff_expense'),('fee_recovery','fee_recovery_income')]]
        accept(f'fee:{index}', start+_day(bound['native_years'][index]), sum((p['amount'] for p in group['postings'] if p['account'] == 'cash'), 0.), earnings)
    def balance(state): return sum(a['amount'] * p['coefficient'] for a, p in zip(state['channels'], review['policy']['channels'], strict=True))
    changes = [dict(sequence=i+1, ordinal=after['ordinal'], source_event_id=after['source_checkpoint_id'], kind='reviewed_other_reallocation', amount=balance(after)-balance(before), cash_event_id=None, review=deepcopy(review['channel_change_review'])) for i, (before, after) in enumerate(zip(states, states[1:]))]
    request = dict(schema='modeled-product-ftp-source-channels-3', measure='physical', source=deepcopy(current['contract']), original_yield_anchor=deepcopy(source['source']['acquisition']['original_yield_anchor']), path_id=native['path_id'], entity=review['entity'], currency_unit=review['currency'], scenario_id=review['scenario'], side='asset', start_ordinal=start, full_end_ordinal=end, source_weight=float(native['weight']), unit_face=float(unit['unit_face']), allocated_face=float(next(a['face'] for a in raw['allocation'] if a['unit_id'] == owner['unit_id'])), opening_funding_balance=balance(states[0]), policy=deepcopy(review['policy']), states=states, cash=cash, changes=changes, earned=earned, resets=deepcopy(review['resets']))
    for reset in request['resets']:
        for name in ('reference_years','liquidity_years'): reset[name] = float(reset[name])
    return dict(scenario=review['scenario'], position_id=review['position_id'], path_key=key, request=request, preparation=prepare(request, check), opening_cash=opening_cash), undrawns


def verify(raw, actual, check=lambda: False):
    check()
    names='schema raw_request source_candidate capital ftp funding_paths full_end_ordinal bridge_work_bound bridge_work_limit native_ordered_journal_replayed separate_fee_source_adoption original_loan_eir_unchanged economic_loss_deducted_again independent_persisted_verification opening_identity_verified mixed_ledger_adoption empirical_validation regulatory_credit_rwa_approval accounting_standard_approval production_adoption'
    if set(actual)!=set(names.split()) or raw.get('schema') != SCHEMA or actual.get('schema') != SCHEMA or not exact(raw, actual['raw_request']): raise ArithmeticError('candidate fee treasury immutable raw identity differs')
    source = actual['source_candidate']; candidate = raw['raw_candidate']
    source_validation = verify_candidate(candidate, source, check)
    end = source['full_end_ordinal']
    if actual['full_end_ordinal'] != end or end-candidate['as_of_ordinal'] > 64: raise ArithmeticError('candidate fee complete bounded source calendar differs')
    for name, value in dict(native_ordered_journal_replayed=True, separate_fee_source_adoption=True, original_loan_eir_unchanged=True, economic_loss_deducted_again=False, independent_persisted_verification=False, opening_identity_verified=False, mixed_ledger_adoption=False, empirical_validation=False, regulatory_credit_rwa_approval=False, accounting_standard_approval=False, production_adoption=False).items():
        if not exact(actual[name], value): raise ArithmeticError('candidate fee treasury authority differs/'+name)
    if type(actual['bridge_work_bound']) is not int or not 0 < actual['bridge_work_bound'] <= 2_000_000 or not exact(actual['bridge_work_limit'], 2_000_000): raise ArithmeticError('candidate fee bridge admission differs')
    if raw.get('capital') is not None:
        if raw['capital']['treasury']['as_of'] != date.fromordinal(end).isoformat() or any(p['capital']['period'] != date.fromordinal(end).isoformat() for p in raw['capital']['policies']): raise ArithmeticError('candidate fee exact closing capital date')
        _capital(raw['capital'], actual['capital'], CapitalFrames(source['tables']), check, fees=True)
    elif actual['capital'] is not None: raise ArithmeticError('unselected candidate fee capital')
    paths = []; positions = []; used = set(); ftp = raw.get('ftp')
    if ftp is not None:
        if ftp['as_of'] != date.fromordinal(candidate['as_of_ordinal']).isoformat(): raise ArithmeticError('candidate fee outer asof identity')
        accounts = {a['id']: a for a in source['preparation']['resolved_specification']['accounts']}
        for ui, (unit, owner) in enumerate(zip(candidate['commercial']['units'], candidate['owners'], strict=True)):
            graph = verify_graph(unit['graph'], source['preparation']['native_preparation']['source']['native_graphs'][ui], check)
            for native in graph['sources']:
                if native['role'] != 'physical': continue
                scenario = next(m['ledger_scenario_id'] for m in candidate['run_mapping'] if m['source_scenario_id'] == native['scenario_id'])
                review = next(p for p in ftp['reviews'] if (p['scenario'],p['position_id'],p['path_key']) == (scenario,owner['position_id'],native['path_key']))
                key = (scenario, owner['position_id'], native['path_key'])
                if key in used: raise ArithmeticError('duplicate candidate fee funding review')
                used.add(key); account = accounts[owner['account_id']]
                if (review['entity'],review['currency']) != (account['entity'],account['currency']): raise ArithmeticError('candidate fee entity/currency lineage differs')
                path, undrawn = _funding(candidate, unit, owner, native, review, end, check); paths.append(path); prepared = path['preparation']
                if not prepared['treasury_nonnegative_positions_compatible']: raise ArithmeticError('research signed funding cannot enter nonnegative Treasury positions')
                for i, day in enumerate(prepared['days']):
                    avg = undrawn[i] if review['policy']['averaging'] == 'opening_daily' else (undrawn[i]+undrawn[i+1])*.5
                    positions.append(dict(id=f"{scenario}:{owner['position_id']}:{native['path_key']}:{day['ordinal']}", loan_id=owner['position_id'], cohort_id=None, entity=review['entity'], currency=review['currency'], scenario=scenario, period=date.fromordinal(day['ordinal']).isoformat(), curve_id=review['curve_id'], side='asset', average_balance=day['scaled_average_funding_balance'], accrual_fraction=day['accrual_fraction'], repricing_years=day['reference_years'], funding_years=day['liquidity_years'], external_interest=day['original_eir'], fees=sum(a['amount'] for a in day['earnings_by_channel'] if a['channel'] != 'original_eir'), operating_cost=review['daily_operating_cost']*prepared['scale'], expected_loss=0., allocated_capital=review['allocated_capital']*prepared['scale'], cost_of_capital=float(review['annual_cost_of_capital']), option_spread=float(review['option_spread']), contingent_liquidity_charge=avg*prepared['scale']*review['contingent_annual_rate']*day['accrual_fraction']))
        if len(used) != len(ftp['reviews']): raise ArithmeticError('unused candidate fee funding review')
        specification = dict(as_of=ftp['as_of'], policy_id=ftp['policy_review']['id'], capital=[], curves=deepcopy(ftp['curves']), positions=positions, strategy_units=[], capital_deltas=[])
        for curve in specification['curves']:
            for name in ('tenors','reference_rates','liquidity_spreads'): curve[name] = [float(v) for v in curve[name]]
        compare(specification, actual['ftp']['prepared_specification'], check=check)
        rows, reconciliation = _ftp(specification)
        compare(rows, actual['ftp']['ftp_positions'], check=check); compare(reconciliation, actual['ftp']['ftp_reconciliation'], check=check)
        if not exact(actual['ftp']['policy_review'], ftp['policy_review']): raise ArithmeticError('candidate fee treasury policy identity')
        if actual['ftp']['capital_metrics'] or actual['ftp']['capital_limits']: raise ArithmeticError('candidate fee FTP cannot infer capital')
    elif actual['ftp'] is not None: raise ArithmeticError('unselected candidate fee FTP')
    compare(paths, actual['funding_paths'], check=check)
    return dict(verified=True, raw_source_recomputed=True, ordered_journal_replayed=True, persisted_partitions_replayed=True, complete_tail_verified=True, separate_fee_rights_verified=True, original_loan_eir_unchanged=True, closing_capital_verified=raw.get('capital') is not None, daily_ftp_verified=ftp is not None, opening_identity_verified=False, mixed_ledger_adoption=False, empirical_validation=False, regulatory_credit_rwa_approval=False)
