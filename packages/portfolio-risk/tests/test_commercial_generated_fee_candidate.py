"""Actual generated joint-fee Sim, bounded persisted replay and tamper controls."""
from copy import deepcopy
import json
import os
from pathlib import Path

import polars as pl
import pytest
from portfolio_risk.analytics.balance_stream import _rust
from portfolio_risk.analytics.bounded_native_records import loads
from portfolio_risk.analytics.commercial_generated_transport import GeneratedWriter, VerificationFrames, selected
from portfolio_risk.analytics.commercial_generated_candidate_reference import verify_preparation, verify_stream
from portfolio_risk.analytics.model_ledger import binary_path

ROOT = Path(__file__).resolve().parents[3]


def fixture():
    return json.loads((ROOT / 'examples/models/commercial-generated-candidate-fee-journal-1.json').read_text())


def run(raw, directory):
    workflow, protocol, _, _, operation = selected(raw['schema'])
    writer = GeneratedWriter(directory, 65536, check=lambda: False, model=raw['schema'])
    header, completion, trailer = {}, {}, {}
    executable = Path(os.environ.get('PORTFOLIO_GENERATED_TEST_BINARY', binary_path()))
    count=[0]
    def decode(line):
        count[0]+=len(line)
        return loads(line)
    _rust(dict(op=operation, schema=raw['schema'], request=raw, execution_timeout_ms=120000),
        writer, None, lambda: False, 120., executable=executable, workflow=header,
        completion_protocol=protocol, completion=completion, completion_trailer=trailer,
        decode_record=decode, record_bytes=32 << 20)
    writer.native_stream_bytes=count[0]
    assert header['schema'] == workflow
    writer.finish()
    writer.admit_persisted()
    source = verify_preparation(raw, header['prepared_request'], header['source_receipt'])
    return raw, source, VerificationFrames(writer, lambda: completion, lambda: trailer)


@pytest.fixture(scope='module')
def generated(tmp_path_factory):
    return run(fixture(), tmp_path_factory.mktemp('generated-fee-candidate'))


def test_real_minimum128_joint_policy_both_physical_scenarios_and_separate_fee_journal(generated):
    raw, source, frames = generated
    validation = verify_stream(raw, source, frames)
    assert validation['verified'] and validation['gross_fee_rights_reconciled']
    assert validation['persisted_partitions_replayed'] and validation['compact_absolute_endpoints_replayed']
    assert source['receipt']['physical_paths'] == 256
    assert source['receipt']['maximum_live_full_physical_owners'] == 1
    assert source['receipt']['retained_bytes_upper'] <= 128 << 20
    assert source['receipt']['admitted_work_upper'] <= 4_000_000_000
    assert frames.completion == dict(complete=True, protocol=7)
    assert frames.trailer['record'] == 'commercial_generated_fee_complete'
    phases = set()
    for frame in frames.frames('commercial_fee_lineage'):
        phases.update(loads(row['native_fee_group_json'].encode())['event']['action'] for row in frame.iter_rows(named=True))
    assert {'acquire', 'release_acquired_cost', 'earn', 'cash', 'cancel_unearned'} <= phases
    assert not validation['empirical_validation'] and not validation['opening_identity_verified']


@pytest.mark.parametrize('target', ['protocol', 'trailer', 'inventory', 'fee_cash', 'fee_endpoint', 'loan_weight', 'fee_order', 'fee_missing', 'fee_duplicate', 'fee_clock', 'fee_clock_boundary'])
def test_rehashed_or_corrupted_generated_fee_records_cannot_verify(generated, target):
    raw, source, frames = generated
    base = frames
    class Changed:
        schemas = dict(base.schemas)
        completion = dict(base.completion)
        trailer = dict(base.trailer)
        def frames(self, name):
            changed = False
            for frame in base.frames(name):
                if not changed and ((target in {'fee_cash','fee_order','fee_missing','fee_duplicate','fee_clock','fee_clock_boundary'} and name == 'commercial_fee_lineage')
                        or (target == 'fee_endpoint' and name == 'commercial_fee_daily')
                        or (target == 'loan_weight' and name == 'commercial_candidate_native_sources')):
                    rows = frame.to_dicts()
                    key = 'native_fee_group_json' if target in {'fee_cash','fee_order','fee_missing','fee_duplicate','fee_clock','fee_clock_boundary'} else 'native_fee_snapshot_json' if target == 'fee_endpoint' else 'native_json'
                    value = loads(rows[0][key].encode())
                    if target=='fee_order':rows[:2]=reversed(rows[:2])
                    elif target=='fee_missing':rows.pop(0)
                    elif target=='fee_duplicate':rows.insert(0,dict(rows[0]))
                    elif target=='fee_clock':rows[0]['native_year']=5e-10
                    elif target=='fee_clock_boundary':rows[0]['native_year']=float.fromhex('0x0.0000000000001p-1022')
                    elif target == 'fee_cash': value['postings'][0]['amount'] += .01
                    elif target == 'fee_endpoint': value['fee_receivable'] += .01
                    else: value['weight'] *= .5
                    if target not in {'fee_order','fee_missing','fee_duplicate'}:rows[0][key] = json.dumps(value)
                    frame = pl.DataFrame(rows, schema=frame.schema)
                    changed = True
                yield frame
    changed = Changed()
    if target == 'protocol': changed.completion['protocol'] = 6
    elif target == 'trailer': changed.trailer['record'] = 'commercial_generated_complete'
    elif target == 'inventory': changed.schemas.pop('commercial_fee_closing')
    with pytest.raises((ArithmeticError, ValueError)):
        verify_stream(raw, source, changed)


def test_zero_face_keeps_complete_unweighted_loan_and_fee_owners(tmp_path):
    raw = fixture()
    raw['allocation'][0]['face'] = 0.
    raw, source, frames = run(raw, tmp_path)
    assert verify_stream(raw, source, frames)['complete_tail_replayed']
    assert sum(frame.height for frame in frames.frames('commercial_fee_daily')) > 0
    assert sum(frame.height for frame in frames.frames('commercial_daily')) == 256 * 31
    assert all(frame.filter(pl.col('event') != 'opening').height == 0 for frame in frames.frames('journal'))


def test_fee_journal_later_calendar_uses_partitions_beyond_finite_result_limit(tmp_path,record_property):
    raw=fixture()
    # A single future service fee permits a longer truthful outer calendar under
    # the same joined work cap. Complete two-fee ownership is tested above.
    g=raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']
    for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:
        policy=law['nonintegral_fees']['policy']
        policy['fees']=[f for f in policy['fees'] if f['fee_id']=='service-fee']
        policy['events']=[e for e in policy['events'] if e['fee_id']=='service-fee']
    a=g['fee_acquisition'];a['original_signed_fee_quote']=0.;a['total_purchase_cost']=100.01
    a['allocations']=[f for f in a['allocations'] if f['fee_id']=='service-fee']
    a['original_physical_releases']=[]
    for role in a['physical_releases_by_scenario']:role['acquisition_releases']=[]
    raw['specification']['horizon_days']=74
    raw,source,frames=run(raw,tmp_path)
    assert verify_stream(raw,source,frames)['complete_tail_replayed']
    assert source['receipt']['full_end_ordinal']-raw['as_of_ordinal']==74
    assert sum(frame.height for frame in frames.frames('commercial_fee_daily'))==256*74
    assert frames._frames.object_count>1
    record_property('native_stream_bytes',frames._frames.native_stream_bytes)
    assert frames._frames.native_stream_bytes>32<<20


@pytest.mark.parametrize('failure', [ArithmeticError, InterruptedError])
def test_spool_closes_suspended_cursors_before_preserving_refusal_or_cancellation(failure):
    from portfolio_risk.analytics._commercial_generated_spool import replay_store
    iterator = None
    directory = None
    with pytest.raises(failure, match='original replay failure'):
        with replay_store(lambda:False) as store:
            directory = Path(store.db.execute('PRAGMA database_list').fetchone()[2]).parent
            sequence = store.sequence()
            sequence.append(dict(amount=1.))
            sequence.append(dict(amount=2.))
            iterator = iter(sequence)
            assert next(iterator) == dict(amount=1.)
            assert store.cursors
            # Keep the generator and traceback alive through Store.close.
            raise failure('original replay failure')
    assert directory is not None and not directory.exists()
    iterator.close()


def test_late_outer_calendar_retains_existing_joined_numeric_refusal(tmp_path):
    raw=fixture()
    raw['specification']['horizon_days']=60
    with pytest.raises(RuntimeError,match='joined commercial/fee ledger exceeds8M/32MiB'):
        run(raw,tmp_path)


def test_signed_acquired_fee_liability_posts_native_cash_and_cost_release_once(tmp_path):
    raw=fixture();g=raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']
    for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:
        p=law['nonintegral_fees']['policy'];p['fees'][0]['entitlement']=-2.
        for event in p['events']:
            if event['fee_id']=='origin-service':event['amount']=-2.
    a=g['fee_acquisition'];a['original_signed_fee_quote']=-2.;a['total_purchase_cost']=98.01
    a['allocations'][0]['allocated_cost']=-2.;a['original_physical_releases'][0]['amount']=-2.
    for role in a['physical_releases_by_scenario']:role['acquisition_releases'][0]['amount']=-2.
    raw,source,frames=run(raw,tmp_path)
    assert verify_stream(raw,source,frames)['gross_fee_rights_reconciled']
    acquired=[]
    for frame in frames.frames('commercial_fee_lineage'):
        for row in frame.iter_rows(named=True):
            group=loads(row['native_fee_group_json'].encode())
            if group['event']['action']=='acquire':acquired.append(group)
    assert len(acquired)==256
    assert all(g['event']['amount']==-2. and g['next']['acquired_fee_liability']==-2. for g in acquired)


def test_generated_fee_recoveries_survive_loan_principal_settlement(tmp_path):
    import math
    raw=fixture();g=raw['commercial']['units'][0]['graph']['positions'][0]['generated_policy']
    quote=1.5*math.exp(-.2*6./365.)
    for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:
        p=law['nonintegral_fees']['policy']
        p['fees']=[f for f in p['fees'] if f['fee_id']=='service-fee']
        p['events']=[e for e in p['events'] if e['fee_id']=='service-fee']
        next(e for e in p['events'] if e['action']=='earn')['ordinal']=raw['as_of_ordinal']
        p['exercise']['waive_unpaid_earned_fraction']=1.
        p['exercise']['recoveries']=[dict(lag_days=5,fraction=.5)]
    a=g['fee_acquisition'];a['original_signed_fee_quote']=quote;a['total_purchase_cost']=100.01+quote
    a['allocations']=[f for f in a['allocations'] if f['fee_id']=='service-fee']
    a['allocations'][0]['allocated_cost']=quote
    release=dict(fee_id='service-fee',ordinal=raw['as_of_ordinal']+6,sequence=0,amount=quote,review=dict(a['identity']))
    a['original_physical_releases']=[release]
    for role in a['physical_releases_by_scenario']:role['acquisition_releases']=[dict(release)]
    raw,source,frames=run(raw,tmp_path)
    assert verify_stream(raw,source,frames)['complete_tail_replayed']
    recoveries=[]
    for frame in frames.frames('commercial_fee_lineage'):
        for row in frame.iter_rows(named=True):
            event=loads(row['native_fee_group_json'].encode())['event']
            if event['action']=='recover_waived':recoveries.append((row['day'],event['amount']))
    assert len(recoveries)==256 and all(day==6 and amount==1.5 for day,amount in recoveries)


@pytest.mark.parametrize('failure',['cancel','write','deadline'])
def test_generated_fee_partial_native_output_preserves_failure_and_cleans_owned_files(tmp_path,failure):
    from portfolio_risk.analytics.commercial_generated_ledger import run_generated_candidate_ledger
    from portfolio_risk.analytics.model_ledger import _publish_directory
    stopped=[False];events=[]
    def progress(table):
        events.append(table)
        if failure=='write':raise OSError('explicit output sink failure')
        stopped[0]=True
    def cancelled():
        if failure=='deadline':
            import time
            time.sleep(.01)  # Real elapsed time exceeds the original1ms budget.
        return stopped[0]
    expected=TimeoutError if failure=='deadline' else OSError if failure=='write' else InterruptedError
    match='deadline exceeded' if failure=='deadline' else 'output sink failure' if failure=='write' else 'candidate cancelled'
    with pytest.raises(expected,match=match):
        run_generated_candidate_ledger(fixture(),tmp_path,progress=progress,cancelled=cancelled,
            timeout=.001 if failure=='deadline' else 120.,
            executable=Path(os.environ.get('PORTFOLIO_GENERATED_TEST_BINARY',binary_path())),publish_directory=_publish_directory)
    assert failure=='deadline' or events
    assert not list(tmp_path.iterdir())


@pytest.mark.parametrize('year',[5e-10,float.fromhex('0x0.0000000000001p-1022'),-1e-20])
def test_fee_clock_comparison_refuses_sub_money_tolerance_or_calendar_retiming(year):
    from portfolio_risk.analytics.commercial_generated_fee_journal_reference import compare_fee_lineage
    with pytest.raises(ArithmeticError):
        compare_fee_lineage([dict(native_year=0.)],[dict(native_year=year)])


def test_fractional_fee_clocks_keep_inherited_clock_tolerance_within_same_day():
    from portfolio_risk.analytics.commercial_generated_fee_journal_reference import compare_fee_lineage
    compare_fee_lineage([dict(native_year=.1)],[dict(native_year=.1+5e-15)])
