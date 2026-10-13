"""Float64 reporting values must not inherit caller JSON integer spelling."""
from copy import deepcopy
from pathlib import Path
import json
import pytest
import polars as pl
from portfolio_risk.analytics import model_ledger
from portfolio_risk.analytics import mortgage_mixed_reference as reference
from portfolio_risk.analytics import _mortgage_mixed_financial_reference as financial
from portfolio_risk.analytics.mortgage_mixed_transport import SCHEMAS

ROOT = Path(__file__).resolve().parents[3]


def browser_numbers(value):
    # JavaScript JSON.stringify writes finite exact integral Numbers without .0.
    if type(value) is float and value.is_integer():
        return int(value)
    if isinstance(value, dict):
        return {k: browser_numbers(v) for k, v in value.items()}
    if isinstance(value, list):
        return [browser_numbers(v) for v in value]
    return value


@pytest.fixture(scope='module')
def integer_run(tmp_path_factory):
    raw = json.loads((ROOT / 'examples/models/mortgage-mixed-accounting-1.json').read_text())
    raw = browser_numbers(raw)
    raw['dated']['specification']['accounts'][0]['cash_floor'] = 0
    assert type(raw['dated']['specification']['positions'][0]['balance']) is int
    assert type(raw['dated']['specification']['accounts'][0]['cash_floor']) is int
    path = model_ledger.run_model_ledger(raw, tmp_path_factory.mktemp('mortgage-integer-json'),
        partition_rows=7, timeout=60.)
    manifest = json.loads(path.read_text())
    header = json.loads((path.parent / manifest['modeled_source']['path']).read_text())
    control = reference.verify_preparation(raw, header['prepared_request'], header['source_receipt'])
    return raw, path, manifest, control


def test_browser_integer_finances_verify_every_default_native_partition(integer_run):
    raw, path, manifest, control = integer_run

    class Stored:
        schemas = SCHEMAS

        @property
        def completion(self):
            return json.loads((path.parent / manifest['producer_completion']['path']).read_text())

        def frames(self, name):
            for part in manifest['tables'][name]['parts']:
                yield pl.read_parquet(path.parent / part['path'])

    verified = reference.verify_stream(raw, control, Stored())
    assert verified == manifest['validation']
    assert verified['single_daily_gl_verified'] is True
    # Raw immutable Value remains integer JSON; only expected financial outputs cast.
    assert type(control.raw['dated']['specification']['positions'][0]['balance']) is int
    for table, row in financial.replay_rows(control.raw, control.specification, control.events,
            control.bindings, control.sources, verified_posting_lines=control.topology):
        for field, dtype in SCHEMAS[table].items():
            if dtype == pl.Float64 and field in row and row[field] is not None:
                assert type(row[field]) is float, (table, field, row[field])


@pytest.mark.parametrize('actual,expected', [(0., 0), (1, True), (3., 3), (-0., 0.)])
def test_integer_day_boolean_and_signed_zero_identities_remain_exact(actual, expected):
    with pytest.raises(ArithmeticError):
        financial.exact(actual, expected)


def test_empty_account_and_pre_acquisition_reporting_finances_are_binary64():
    account = dict(id='a', entity='e', currency='USD', cash=0, equity=0, cash_floor=0,
        capital_deductions=0, cet1_floor=0, leverage_floor=0, lcr_floor=0,
        nsfr_floor=0, htm_asset_limit=1)
    row = financial.observe({}, [], [account], {}, [], [], 'baseline', 0)[0]
    for key, dtype in SCHEMAS['path'].items():
        if dtype == pl.Float64 and row[key] is not None:
            assert type(row[key]) is float, key
    assert type(row['day']) is int


def test_financial_tolerance_and_raw_integer_type_stay_unchanged():
    financial.equal(50., 50.)
    with pytest.raises(ArithmeticError):
        financial.equal(50.0001, 50.)
    with pytest.raises(ArithmeticError):
        financial.equal(50., 50)


@pytest.mark.parametrize('field', ['balance', 'allowance', 'watch_fraction', 'commitment'])
def test_bool_financial_input_is_rejected_before_reporting_cast(field):
    raw = json.loads((ROOT / 'examples/models/mortgage-mixed-accounting-1.json').read_text())
    raw['dated']['specification']['positions'][0][field] = False
    with pytest.raises((ArithmeticError, ValueError)):
        financial.admit(raw)


@pytest.mark.parametrize('value', [float('nan'), float('inf'), -float('inf')])
def test_nonfinite_financial_input_is_rejected_before_reporting_cast(value):
    raw = json.loads((ROOT / 'examples/models/mortgage-mixed-accounting-1.json').read_text())
    raw['dated']['specification']['positions'][0]['balance'] = value
    with pytest.raises((ArithmeticError, ValueError)):
        financial.admit(raw)
