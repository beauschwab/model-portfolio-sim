"""Native pilot gates: all public frames, reference replay, invalid input, isolation."""
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy

import numpy as np
import polars as pl
from polars.testing import assert_frame_equal
import pytest

from portfolio_risk.analytics.balance_stress import run_balance_stress, example_specification
from portfolio_risk.analytics.journal import replay
from portfolio_risk.analytics.ledger_native import BatchJournal, RustLedgerKernel, library_path, python_apply
from test_balance_stress import small
from test_balance_ledger import funded
from test_security_basis import premium


@pytest.fixture(scope='module')
def kernel():
    if not library_path().is_file():
        pytest.skip('optional Rust ledger DLL is not built')
    return RustLedgerKernel()


def fixture(case):
    if case == 'mixed':
        spec = example_specification()
        spec['horizon_days'] = 30
        spec['reverse_severities'] = [0., 1.5]
        return spec
    if case == 'collateral':
        return funded()
    if case in {'afs', 'trading'}:
        spec = premium(case)
        spec['accounts'][0]['tax_rate'] = .2
        spec['cashflows'] = [dict(position='s', day=3, cash_interest=5., accrual_interest=5., book_amortization=-1.),
                             dict(position='s', day=30, principal=100., book_amortization=-9.)]
        spec['policies'] = [dict(id='sale', account='a', kind='sell', position='s', trigger_cash=60., limit=50.)]
        return spec
    spec = small()
    if case == 'zero':
        spec['positions'] = []
        spec['accounts'][0].update(cash=0., equity=0.)
    elif case == 'forward':
        spec = premium()
        spec['positions'][0]['start_day'] = 2
        spec['accounts'][0].update(cash=130., equity=50.)
    elif case == 'credit':
        spec['positions'][0] = dict(id='s', account='a', kind='loan', balance=100., annual_pd=.2,
                                    rate=-.01, opening_accrued=2., recovery_days=3)
        spec['accounts'][0]['equity'] += 2.
        spec['cashflows'] = [dict(position='s', day=30, principal=100., accrual_interest=1., cash_interest=1.)]
    return spec


@pytest.mark.parametrize('case', ['mixed', 'collateral', 'afs', 'trading', 'zero', 'forward', 'credit'])
@pytest.mark.parametrize('backend', ['columnar', 'rust'])
def test_complete_simulation_parity(kernel, case, backend):
    spec = fixture(case)
    original = deepcopy(spec)
    reference = run_balance_stress(spec)
    result = run_balance_stress(spec, journal_backend=backend)
    assert spec == original
    assert result.keys() == reference.keys()
    for key in result:
        if isinstance(result[key], pl.DataFrame):
            assert_frame_equal(result[key], reference[key], check_exact=True)
        elif key != 'execution':
            assert result[key] == reference[key]
    # Reference implementation independently reconstructs persisted native lines.
    rebuilt = replay(result['journal'].to_dicts())
    for row in result['trial_balance'].to_dicts():
        key = tuple(row[k] for k in ('scenario', 'account', 'gl_account', 'instrument_id'))
        assert rebuilt[key] == pytest.approx(row['balance'], rel=1e-12, abs=1e-9)


@pytest.mark.parametrize('seed', range(6))
def test_randomized_balanced_postings_and_daily_checkpoints(kernel, seed):
    rng = np.random.default_rng(seed)
    native, control = BatchJournal('s', kernel), BatchJournal('s', python_apply)
    expected = {('a', 'cash', ''): 0.}
    for day in range(10):
        for _ in range(25):
            instrument = str(rng.integers(0, 30))
            amount = float(rng.normal()*1e5)
            entries = {('asset_principal', instrument): amount, ('cash', ''): -amount}
            expected['a', 'asset_principal', instrument] = expected.get(('a', 'asset_principal', instrument), 0.)+amount
            expected['a', 'cash', ''] -= amount
            native.post(day, 'a', 'move', entries)
            control.post(day, 'a', 'move', entries)
        native.verify(expected)
        control.verify(expected)
        assert np.array_equal(native.state, control.state)
    n, nt = native.close()
    p, pt = control.close()
    assert_frame_equal(n, p, check_exact=True)
    assert nt == pt
    assert replay(n.to_dicts()) == replay(p.to_dicts())


@pytest.mark.parametrize('case', ['unbalanced', 'mismatch', 'nan', 'offset', 'key', 'overflow'])
def test_native_validation_and_atomic_output(kernel, case):
    previous, expected = np.zeros(2), np.array([1., -1.])
    offsets, keys, values = np.array([0, 2], dtype=np.int64), np.array([0, 1], dtype=np.int64), np.array([1., -1.])
    if case == 'unbalanced': values[1] = -2.
    if case == 'mismatch': expected[0] = 2.
    if case == 'nan': values[0] = np.nan
    if case == 'offset': offsets[0] = 1
    if case == 'key': keys[1] = 2
    if case == 'overflow': values[:] = [np.finfo(float).max, -np.finfo(float).max]
    output = np.full(2, 123.)
    status = kernel._apply(2, 1, 2, previous, offsets, keys, values, expected, output)
    assert status != 0
    assert output.tolist() == [123., 123.]
    with pytest.raises((ValueError, ArithmeticError)):
        kernel(previous, offsets, keys, values, expected)


def test_missing_and_unknown_backend_fail_without_fallback(monkeypatch):
    monkeypatch.setenv('PORTFOLIO_LEDGER_RUST_LIB', '/missing/ledger.dll')
    with pytest.raises(RuntimeError, match='no fallback'):
        run_balance_stress(small(), journal_backend='rust')
    with pytest.raises(ValueError, match='unknown journal backend'):
        run_balance_stress(small(), journal_backend='typo')


def test_parallel_calls_are_isolated_and_inputs_unchanged(kernel):
    previous = np.zeros(2)
    offsets, keys = np.array([0, 2], dtype=np.int64), np.array([0, 1], dtype=np.int64)
    amounts = [np.array([float(i), -float(i)]) for i in range(1, 17)]
    with ThreadPoolExecutor(max_workers=4) as pool:
        outputs = list(pool.map(lambda v: kernel(previous, offsets, keys, v, v), amounts))
    assert np.array_equal(previous, [0., 0.])
    for i, (out, amount) in enumerate(zip(outputs, amounts), 1):
        assert np.array_equal(out, amount)
        assert amount.tolist() == [float(i), -float(i)]


def test_checkpoint_failure_does_not_publish_partial_state(kernel):
    journal = BatchJournal('s', kernel)
    journal.post(0, 'a', 'opening', {('cash', ''): 10., ('opening_equity', ''): -10.})
    journal.verify({('a', 'cash', ''): 10., ('a', 'opening_equity', ''): -10.})
    before = journal.state.copy()
    journal.post(1, 'a', 'bad', {('cash', ''): 1.})
    with pytest.raises(ArithmeticError, match='unbalanced'):
        journal.verify({('a', 'cash', ''): 11., ('a', 'opening_equity', ''): -10.})
    assert np.array_equal(journal.state, before)
    with pytest.raises(RuntimeError, match='checkpoint'):
        journal.close()


def test_accurate_transaction_sum_preserves_sequential_gl_order(kernel):
    offsets, keys = np.array([0, 4], dtype=np.int64), np.array([0, 0, 0, 1], dtype=np.int64)
    values = np.array([1e16, 1., -1e16, -1.])
    # fsum is zero, while sequential GL accumulation deliberately loses the unit.
    expected = np.array([0., -1.])
    assert np.array_equal(kernel(np.zeros(2), offsets, keys, values, expected), expected)


@pytest.mark.parametrize('bad_keys', [[0., 1.5], [False, True], np.array([0, 2**64-1], dtype=np.uint64)])
def test_integer_indices_are_not_silently_coerced(kernel, bad_keys):
    with pytest.raises(ValueError, match='integers'):
        kernel(np.zeros(2), [0, 2], bad_keys, [1., -1.], [1., -1.])
