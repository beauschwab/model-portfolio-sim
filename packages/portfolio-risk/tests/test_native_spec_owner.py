"""Raw daily-spec normalization and rejection parity, without Python validation."""
from copy import deepcopy
from dataclasses import asdict,is_dataclass
import pytest
from portfolio_risk.analytics import balance_stress as model
from portfolio_risk.analytics.ledger_native import validate_spec,library_path
from portfolio_risk.core.runtime import RunConfig,run_context

pytestmark=pytest.mark.skipif(not library_path().is_file(),reason='build native ledger')


def plain(value):
    if is_dataclass(value):return asdict(value)
    if isinstance(value,dict):return {k:plain(v) for k,v in value.items()}
    if isinstance(value,list):return [plain(v) for v in value]
    return value


def test_native_raw_defaults_reconcile_without_python_validation(monkeypatch):
    from test_balance_stress import small
    for raw in [small(),model.example_specification(),dict(version='balance-stress-2',horizon_days=30,
        accounts=[dict(id='a',entity='e',currency='USD',cash=100.,equity=100.)],scenarios=[dict(name='shock')])]:
        expected=plain(model.validate(raw))
        assert plain(validate_spec(raw))==expected
    def forbidden(*args,**kwargs):raise AssertionError('Python financial specification validation executed')
    monkeypatch.setattr(model,'_decode',forbidden)
    monkeypatch.setattr(model,'_range',forbidden)
    with run_context(RunConfig(compute_backend='rust')):
        assert plain(model.validate(raw))==expected


@pytest.mark.parametrize('mutate',[
    lambda r:r.update(horizon_days=True),lambda r:r.update(unrecognized=0),
    lambda r:r['accounts'][0].update(cash=True),lambda r:r['accounts'][0].update(cash=1e16),
    lambda r:r['accounts'][0].update(include_aoci=1),lambda r:r['accounts'][0].update(equity=1000000000.),
    lambda r:r['accounts'][0].update(htm_asset_limit=2.),lambda r:r['positions'][0].update(payment_interval_days=1.5),
    lambda r:r['positions'][0].update(account='missing'),lambda r:r['positions'][0].update(balance=-1.),
    lambda r:r['positions'][0].update(opening_market_price=0.),lambda r:r['positions'][0].update(hqla_level='fake'),
    lambda r:r['scenarios'][0].update(name='baseline'),lambda r:r.update(reverse_severities=[1.,1.]),
    lambda r:r.update(cashflows=[dict(position=r['positions'][0]['id'],day=0)]),
    lambda r:r.update(cashflows=[dict(position=r['positions'][0]['id'],day=1,principal=1e14)]),
])
def test_native_rejection_matches_reference(mutate):
    from test_balance_stress import small
    raw=deepcopy(small());mutate(raw)
    with pytest.raises(ValueError):model.validate(raw)
    with pytest.raises(ValueError):validate_spec(raw)


def test_native_admission_and_detached_inputs():
    from test_balance_stress import small
    raw=small();saved=deepcopy(raw)
    with pytest.raises(ValueError):validate_spec(raw,max_positions=0)
    with pytest.raises(ValueError):validate_spec(raw,max_work=1)
    result=validate_spec(raw);result['positions'][0].balance=0
    assert raw==saved
