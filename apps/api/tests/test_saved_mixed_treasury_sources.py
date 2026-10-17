"""Closing capital from verified saved-derivative and formal-hedge shared journals.

The source jobs run the actual native model-ledger worker; the treasury bridge replays the
persisted journal against the trial balance, checks the closing statements and runs the
native thirteen-ratio bridge under supplied policies. No FTP or report-date capital.
"""
from copy import deepcopy
from datetime import date
from pathlib import Path
import pytest
import polars as pl
from app import worker, store, persistence as db, saved_derivative_jobs as candidate, treasury_store
from app.artifacts import Codec, Objects
from app.repository import Repository
from app.storage_config import StorageConfig
from portfolio_risk.analytics.saved_derivative_identity import loads
from test_treasury_sources import fill_ledger

ROOT = Path(__file__).resolve().parents[3]
SOURCES = {'hedge': ROOT / 'packages/portfolio-risk/tests/fixtures/saved_hedge_mixed/base.json',
           'derivative': ROOT / 'packages/portfolio-risk/tests/fixtures/saved_derivative_mixed/base.json'}


@pytest.fixture
def durable(tmp_path, monkeypatch):
    config = StorageConfig('sqlite:///' + str(tmp_path / 'worker.db'), str(tmp_path / 'objects'), 'isolated', 'capital')
    repo = Repository(config.database_url, config.tenant, config.workspace)
    repo.migrate()
    codec = Codec(Objects(config.artifact_url, config.tenant, config.workspace))
    identity = db.current_identity()
    monkeypatch.setattr(db, '_IDENTITY', deepcopy(identity))
    monkeypatch.setattr(db, 'REPO', repo)
    monkeypatch.setattr(db, 'CODEC', codec)
    monkeypatch.setattr(db, 'CONFIG', config)
    worker.STOP.clear()
    store.SETTINGS = store.SETTINGS.model_copy(update={'n_threads': 1, 'compute_backend': 'rust'})
    state = store.snapshot_local()
    state['revision'] = 0
    store.STATE_META['revision'] = 0
    repo.initialize(codec.dump(state))

    def source(name):
        raw = loads(SOURCES[name].read_bytes())
        q = candidate.queue_input(raw, timeout=300.)
        payload = dict(operation=candidate.OPERATION, args=[q], state=store.snapshot_local(), identity=deepcopy(identity))
        jid = repo.enqueue('saved_derivative_mixed', 0, codec.dump(payload), {})
        assert repo.acquire('capital-worker', 300.)
        job = repo.claim('capital-worker')
        worker.execute(job)
        row = repo.job(jid)
        assert row['status'] == 'done', row['detail']
        repo.release('capital-worker')
        return jid, raw
    yield source
    worker.STOP.clear()
    repo.engine.dispose()


def policy(jid):
    template = treasury_store.template(jid, 'ledger')['specification']
    report_date = template['treasury']['as_of']
    spec = fill_ledger(template)
    spec['treasury']['as_of'] = report_date
    return spec


@pytest.mark.parametrize('name', ['hedge', 'derivative'])
def test_closing_capital_replays_the_shared_journal_at_the_complete_tail(durable, name):
    jid, raw = durable(name)
    spec = policy(jid)
    d = raw['dated']
    closing = d['dated']['as_of_ordinal'] + d['specification']['horizon_days']
    assert spec['treasury']['as_of'] == date.fromordinal(closing).isoformat()
    out = treasury_store.run(jid, spec)
    assert out['source_verification']['journal_replayed'] is True
    assert out['source_receipt']['model_schema'] == raw['schema']
    assert out['source_receipt']['basis'] == 'complete_tail_shared_journal_closing-1'
    assert out['source_receipt']['closing_ordinal'] == closing
    scopes = {(p['scenario'], p['account']) for p in spec['policies']}
    assert out['capital_bridge'].height == len(scopes) and len(out['capital_metrics']) > 0
    # Closing capital equity equals the verified closing statements; CFH OCI has closed by the tail.
    for r in out['capital_bridge'].iter_rows(named=True):
        assert abs(r['book_aoci']) < 1e-9
    # Independent capital: the verified closing trial balance through the reference bridge rules.
    from portfolio_risk.analytics.hedge_treasury_reference import capital, _typed_policy
    from portfolio_risk.analytics.hedge_accounting_reference import _compare
    tables = db.CODEC.tables(db.result_ref(jid))
    trial = treasury_store._table(tables, '/trial_balance', 250000).to_dicts()
    policies = [_typed_policy(p, require_aoci=False) for p in spec['policies']]
    for p in policies:
        p.setdefault('cash_flow_hedge_aoci_eligible', False)
    audits, contributions, capitals, metrics = capital(trial, policies, spec['amount_multiplier'])
    _compare(audits, out['capital_bridge'].to_dicts(), 'capital_bridge')
    _compare(metrics, out['capital_metrics'].to_dicts() if hasattr(out['capital_metrics'], 'to_dicts') else out['capital_metrics'], 'capital_metrics')


@pytest.mark.parametrize('failure', ['date', 'receipt', 'journal', 'ftp', 'credit'])
def test_closing_capital_refusals(durable, monkeypatch, failure):
    jid, _ = durable('hedge')
    spec = policy(jid)
    if failure == 'date':
        spec['treasury']['as_of'] = '2000-01-01'
    elif failure == 'receipt':
        original = treasury_store._metadata

        def changed(ref, keys):
            out = deepcopy(original(ref, keys))
            if 'execution' in out:
                out['execution']['validation']['complete_hedge_basis_oci_tail_verified'] = False
            return out
        monkeypatch.setattr(treasury_store, '_metadata', changed)
    elif failure == 'journal':
        original = treasury_store._frames

        def tampered(ref):
            for frame in original(ref):
                if 'transaction_id' in frame.columns:
                    frame = frame.with_columns(pl.when(pl.int_range(pl.len()) == 0).then(pl.col('debit') + 1.)
                                               .otherwise(pl.col('debit')).alias('debit'))
                yield frame
        monkeypatch.setattr(treasury_store, '_frames', tampered)
    if failure in ('ftp', 'credit'):
        with pytest.raises(ValueError, match='compatible'):
            treasury_store.template(jid, 'cashflows' if failure == 'ftp' else 'credit_ledger')
        return
    with pytest.raises((ValueError, ArithmeticError)):
        treasury_store.run(jid, spec)
