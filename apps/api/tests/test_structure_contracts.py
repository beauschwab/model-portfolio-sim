"""Composition, sole materialization and unchanged persisted contract layouts."""
import json
import pytest
from fastapi.testclient import TestClient
from app import store, persistence, operations, main
from app.artifacts import Codec, Objects
from app.contracts import WorkspaceSnapshot, PreparedLibraryBundle, QueueFull


def test_durable_operation_allowlist_preserves_names_and_identity():
    handlers=operations.registered()
    assert set(handlers)=={
        'store.run_pricing', 'store.run_risk_all', 'store.run_stress_all',
        'store.run_kpis_scenario', 'store.run_nii', 'store.build_unitlib_job',
        'store.build_aged_unitlib_job', 'store.run_strategy_job', 'store.run_optimize_job',
        'store.run_scenario_grid', 'store.run_forecast', 'store.run_balance_stress',
        'store.run_streamed_balance_stress', 'store.run_saved_balance_stress',
        'store.run_dated_balance_stress', 'store.run_credit_balance_stress',
        'store.run_credit_path_balance_stress', 'store.run_model_analysis',
        'store.run_modeled_balance_stress', 'store.fetch_research_data', 'store.run_treasury',
        'decision.build', 'decision.update', 'treasury.run', 'mortgage_capital.run', 'modeled_product_ftp.run', 'saved_derivative.run', 'saved_derivative_deposit_portfolio.run',
        'cohort.import_tape', 'cohort.build', 'cohort.analytics', 'cohort.attribution', 'cohort.audit',
    }
    from app import mortgage_capital, saved_derivative_jobs, joint_deposit_derivative_jobs
    assert handlers['saved_derivative_deposit_portfolio.run'] is joint_deposit_derivative_jobs.run
    assert handlers['saved_derivative.run'] is saved_derivative_jobs.run
    assert handlers['mortgage_capital.run'] is mortgage_capital.run
    assert handlers['store.run_credit_path_balance_stress'] is store.run_credit_path_balance_stress
    for name,fn in handlers.items():
        assert operations.resolve(name) is fn
        assert operations.operation(fn)==name
    for name in ['store.__dict__','store.snapshot','decision.close','os.system','','store.run_pricing.extra']:
        with pytest.raises(ValueError,match='unregistered'):operations.resolve(name)
    with pytest.raises(ValueError):operations.operation(lambda:None)
    assert store.QueueFull is QueueFull


def test_historical_snapshot_roundtrip_preserves_dictionary_identity(tmp_path):
    with TestClient(main.app):
        original_books=store.BOOKS;original_market=store.MARKET
        state: WorkspaceSnapshot=store.snapshot()
        state.pop('tapes');state.pop('cohort_publications')
        state['settings']=state['settings'].model_copy(update={'compute_backend':'python'})
        codec=Codec(Objects(str(tmp_path),'isolated','structure'))
        loaded=codec.load(codec.dump(state))
        store.CACHE.update(revision=state['revision'],library={},base={},compute_backend='rust')
        persistence.restore(loaded)
        assert store.BOOKS is original_books and store.MARKET is original_market
        assert not store.CACHE and not store.TAPES and not store.COHORT_PUBLICATIONS
        assert store.SETTINGS.compute_backend=='python'
        with pytest.raises(ValueError,match='PYTHON_BACKEND_DEPRECATED'):store.SETTINGS.require_production_backend()


def test_json_response_schemas_and_transport_exceptions():
    document=main.app.openapi()
    for path in ['/state','/settings','/market','/jobs/{jid}','/balance-stress/capabilities']:
        assert '$ref' in document['paths'][path]['get']['responses']['200']['content']['application/json']['schema']
    assert 'application/json' not in document['paths']['/jobs/{jid}/result']['get']['responses']['200'].get('content',{}) or document['paths']['/jobs/{jid}/result']['get']['responses']['200']['content']['application/json']['schema']=={}
    with TestClient(main.app) as client:
        assert client.get('/state').json()['revision']==store.STATE_META['revision']
        assert client.get('/market').json()==store.market_view()
