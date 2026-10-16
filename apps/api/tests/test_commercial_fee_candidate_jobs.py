"""Joint finite fee exercise survives actual durable candidate publication."""
from copy import deepcopy
import json
from pathlib import Path

import pytest

from app import model_routes, persistence as db, worker
from app.model_tables import format_sources, restore_sources
from test_persistence import durable

ROOT = Path(__file__).resolve().parents[3]


def raw_request():
    raw = json.loads((ROOT/'examples/models/commercial-candidate-journal-1.json').read_text())
    units = json.loads((ROOT/'examples/models/commercial-fee-units-1.json').read_text())
    unit = units['units'][0]; unit['accounting_category'] = 'AC'
    graph = unit['graph']; position = graph['positions'][0]; origin = graph['as_of_ordinal']
    quote = 99. + 100.*.1*10./365.
    position['original_signed_cash_quote'] = quote
    position['exercise'] = dict(schema='commercial-native-exercise-1', identity=dict(id='durable-joint-call', revision='1'),
        information_policy='native_full_observed_prefix-1', simultaneous_priority='issuer',
        exercise_dates=[dict(ordinal=origin+10, timing='after_scheduled_cash', call_clean_price_factor=.99,
                            put_clean_price_factor=None, call_fee=0., put_fee=0.)])
    loan_histories = position['original_pricing'] + [h for hs in position['histories']
        for role in ('physical', 'pricing', 'pricing_down', 'pricing_up') for h in hs[role]]
    for h in loan_histories: h['source']['source']['acquisition']['cost'] = quote
    position['original_physical'] = deepcopy(position['histories'][0]['physical'])
    for h in position['original_physical']: h['source']['source']['current'] = deepcopy(h['source']['source']['original'])
    selection = position['fees']; selection['total_purchase_cost'] = quote + 5.
    fee_histories = selection['original_pricing'] + [h for hs in selection['histories']
        for role in ('physical', 'pricing', 'pricing_down', 'pricing_up') for h in hs[role]]
    for h in fee_histories:
        policy = h['policy']; policy['coincident_order'] = 'native_exit_follows_native_exercise_timing-1'
        policy['exercise'].update(cancel_future_service=True, cancel_future_cash=True, cancel_unearned_entitlement=True)
        for event in policy['events']: event['ordinal'] = origin+10
    selection['original_physical'] = deepcopy(selection['histories'][0]['physical'])
    scenario = deepcopy(graph['scenarios'][0]); scenario.update(scenario_id='stress', run_id='stress-run')
    graph['scenarios'].append(scenario)
    for histories in (position['histories'], selection['histories']):
        stress = deepcopy(histories[0]); stress['scenario_id'] = 'stress'; histories.append(stress)
    raw['commercial'] = units
    return raw


def queued():
    raw = raw_request()
    jid = model_routes.submit(model_routes.ModelRunRequest(schema_name=raw['schema'], request=raw, expected_revision=0)).id
    db.WORKER = 'joint-fee-candidate-test'; worker.STOP.clear(); assert db.REPO.acquire(db.WORKER)
    return jid, db.REPO.claim(db.WORKER), raw


def test_joint_fee_candidate_publishes_reopens_and_replays_complete_actual_owners(durable):
    jid, job, raw = queued()
    deadline = db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job); row = db.REPO.job(jid)
    assert row['status'] == 'done', row['detail']
    saved = db.CODEC.load(db.result_ref(jid))
    assert saved['execution']['publication_deadline_unix'] == deadline
    assert saved['execution']['source_verification']['purchased_journal_replayed']
    result = restore_sources(saved['model_result'], saved['execution']['table_layouts'])
    assert result['preparation']['raw_request'] == raw
    assert result['producer_completion'] == dict(complete=True, protocol=7)
    graph = result['preparation']['native_preparation']['source']['native_graphs'][0]
    assert graph['exercise_policies'][0]['policy']['nodes'][0]['decision'] == 'call'
    assert all(s['fee_projection']['bound_path']['exit']['exercise_timing'] == 'after_scheduled_cash'
               for s in graph['native_sources'])
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    assert verify(raw, result)['persisted_partitions_replayed']


@pytest.mark.parametrize('target', ['basis', 'fee_cash', 'decision', 'exercise_clock'])
def test_rehashed_joint_fee_candidate_source_and_posting_tamper_refuses(durable, monkeypatch, target):
    jid, job, _ = queued(); dump = db.CODEC.dump; load = db.CODEC.load; changed = []
    def mutate(value):
        ref = dump(value)
        if type(value) is not dict or value.get('execution', {}).get('schema') != 'commercial-candidate-journal-1': return ref
        saved = load(ref); result = restore_sources(saved['model_result'], saved['execution']['table_layouts']); changed.append(True)
        graph = result['preparation']['native_preparation']['source']['native_graphs'][0]
        if target == 'basis':
            graph['native_sources'][0]['fee_projection']['allocations'][0]['allocated_cost'] += 1.
        elif target == 'fee_cash':
            row = next(r for r in result['tables']['journal'] if r['event'] == 'commercial_nonintegral_fee' and r['gl_account'] == 'cash' and r['value'] > 0.)
            row['value'] += 1.
        elif target == 'decision': graph['exercise_policies'][0]['policy']['nodes'][0]['decision'] = 'continue'
        else: graph['native_sources'][0]['fee_projection']['bound_path']['exit']['exercise_timing'] = 'before_scheduled_cash'
        saved['model_result'], saved['execution']['table_layouts'] = format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC, 'dump', mutate); worker.execute(job)
    row = db.REPO.job(jid)
    assert changed and row['status'] != 'done' and row['result'] is None
