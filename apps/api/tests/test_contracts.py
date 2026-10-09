"""HTTP, serialization, job isolation and cache-publication regression gates."""
import importlib
import json
import struct
import threading
import time

import numba
import pytest
from fastapi.testclient import TestClient

from app import main, store


@pytest.fixture
def client():
    with store._LOCK:
        store.BOOKS.clear()
        store.SCENARIOS.clear()
        store.JOBS.clear()
        store.CACHE.clear()
        store.SETTINGS = main.RiskSettings(n_threads=2)
        store.STATE_META["revision"] = 0
    with TestClient(main.app) as client:
        yield client


def finished(jid):
    until = time.monotonic() + 15
    while time.monotonic() < until:
        job = store.job_status(jid)
        if job["status"] in ("done", "error"):
            return job
        time.sleep(.005)
    raise AssertionError("job did not finish")


def skeleton(blob):
    n = struct.unpack_from("<I", blob, 4)[0]
    return json.loads(blob[8:8 + n])


def test_interactive_evaluation_preserves_library_backend(client, monkeypatch):
    from portfolio_risk import unitlib
    from portfolio_risk.core.runtime import assumption
    observed=[]
    def evaluate(lib, allocations, base_kpis):
        observed.append(assumption('compute_backend','python'))
        return {'nii_total_$':0.}
    monkeypatch.setattr(unitlib,'evaluate_strategy',evaluate)
    store.CACHE.update(revision=0,library={},base={},compute_backend='rust')
    assert store.eval_strategy_sync([])['revision']==0
    assert observed==['rust']
    assert assumption('compute_backend','python')=='python'


def test_production_defaults_and_deprecated_options(client):
    assert client.get('/settings').json()['compute_backend']=='rust'
    assert main.RiskSettings.model_validate({}).compute_backend=='rust'
    response=client.put('/settings',json={'compute_backend':'python'})
    assert response.status_code==422 and 'PYTHON_BACKEND_DEPRECATED' in response.text
    assert client.get('/settings').json()['compute_backend']=='rust'
    for backend in ['numpy','numba','python']:
        assert client.post('/run',json={'kind':'pricing','backend':backend}).status_code==422
    store.SETTINGS=main.RiskSettings(compute_backend='python')
    response=client.post('/run',json={'kind':'pricing','books':['loans']})
    assert response.status_code==409 and 'PYTHON_BACKEND_DEPRECATED' in response.text
    # Historical settings remain inspectable; selecting Rust is an explicit edit.
    assert client.get('/settings').json()['compute_backend']=='python'
    assert client.put('/settings',json={'compute_backend':'rust'}).status_code==200


def test_production_pricing_and_whatif_use_native_coordinators(client,monkeypatch):
    from portfolio_risk.analytics import incremental,whatif
    from portfolio_risk.core import graph_native
    store.SETTINGS=main.RiskSettings(n_paths=32,n_paths_base=32,horizon_months=3,n_threads=2)
    def forbidden(*a,**kw):raise AssertionError('Python financial coordination invoked')
    monkeypatch.setattr(incremental,'Evaluation',forbidden)
    monkeypatch.setattr(whatif,'apply_overrides',forbidden)
    seen=[]
    for name in ['price_books','compare_books']:
        original=getattr(graph_native,name)
        def capture(*a,_original=original,_name=name,**kw):
            seen.append(_name);return _original(*a,**kw)
        monkeypatch.setattr(graph_native,name,capture)
    sr,vp=store.MARKET['swap_rates'],store.MARKET['vol_pts']
    store.run_pricing(['loans'],sr,vp)
    store.run_pricing(['loans'],sr,vp,options={'compare':True})
    assert seen==['price_books','compare_books']


def test_legacy_python_library_requires_rebuild(client):
    store.CACHE.update(revision=0,library={},base={},compute_backend='python')
    with pytest.raises(RuntimeError,match='PYTHON_BACKEND_DEPRECATED'):store.eval_strategy_sync([])


def test_whatif_calibration_validation_and_analytics(client):
    store.SETTINGS = main.RiskSettings(n_paths=32, n_paths_base=32, horizon_months=6, n_threads=2)
    ident = store.BOOKS['loans']['id'][0]
    coupon = float(store.BOOKS['loans']['coupon_or_spread'][0])
    baseline = store.BOOKS['loans'].clone()
    rev = store.STATE_META['revision']
    payload = dict(kind='whatif', books=['loans'], include_analytics=True, expected_revision=rev,
                   assumption_overrides={'loans': {ident: {'coupon_or_spread': coupon+.01}}})
    assert client.post('/run', json=payload | {'expected_revision': rev+1}).status_code == 409
    assert client.post('/run', json=payload | {'assumption_overrides': {'loans': {ident: {'price': 90}}}}).status_code == 422
    assert client.post('/run', json=payload | {'assumption_overrides': {'loans': {ident: {'coupon_or_spread': 10}}}}).status_code == 422
    assert client.get('/pricing/assumptions').status_code == 200
    job = client.post('/run', json=payload)
    assert job.status_code == 200
    jid = job.json()['id']
    assert finished(jid)['status'] == 'done', store.job_status(jid)
    result = skeleton(store.job_result(jid))
    assert result['calibration_mode'] == 'hold' and result['net_value_change'] > 0
    assert result['revised']['nii']['total'] > 0
    assert store.BOOKS['loans'].equals(baseline) and store.STATE_META['revision'] == rev
    rebased = client.post('/run', json=payload | {'calibration_mode': 'recalibrate'}).json()['id']
    assert finished(rebased)['status'] == 'done'
    assert abs(skeleton(store.job_result(rebased))['net_value_change']) < 10


def test_incremental_pricing_http_and_scope_validation(client):
    settings = client.get('/settings').json() | {'n_paths': 33, 'n_paths_base': 35, 'n_threads': 2}
    assert client.put('/settings', json=settings).status_code == 200
    for payload in [dict(kind='pricing', books=['mm']),
                    dict(kind='pricing', books=['loans'], spread_overrides_bp={'cds': {'missing': 25}}),
                    dict(kind='pricing', books=['loans'], spread_overrides_bp={'loans': {'missing': 25}}),
                    dict(kind='risk', spread_overrides_bp={'loans': {'missing': 25}}),
                    dict(kind='pricing', books=['loans', 'loans'])]:
        assert client.post('/run', json=payload).status_code == 422
    ident = store.BOOKS['loans']['id'][0]

    def pricing(**extra):
        response = client.post('/run', json=dict(kind='pricing', books=['loans']) | extra)
        assert response.status_code == 200, response.text
        jid = response.json()['id']
        assert finished(jid)['status'] == 'done', store.job_status(jid)
        return skeleton(client.get(f'/jobs/{jid}/result').content)

    base = pricing()
    warm = pricing()
    assert base['totals'] == warm['totals']
    assert sum(s['computed'] for s in warm['graph'].values()) == 0
    spread = pricing(spread_overrides_bp={'loans': {ident: 25}})
    assert spread['graph']['marks:loans']['computed'] == 1
    assert sum(s['computed'] for s in spread['graph'].values()) == 1
    assert spread['totals']['loans'] < base['totals']['loans']
    assert client.put('/scenarios/up', json={'name': 'up', 'ust10y_bp': [100]}).status_code == 200
    scenario = pricing(scenario='up')
    assert scenario['graph']['calibration:loans']['computed'] == 0
    assert scenario['revision'] == store.STATE_META['revision']


def test_incremental_job_keeps_queued_snapshot_after_book_edit(client, monkeypatch):
    store.SETTINGS = main.RiskSettings(n_paths=33, n_paths_base=35, n_threads=2)
    entered, resume = threading.Event(), threading.Event()
    original = store.run_pricing

    def blocked(*args):
        entered.set()
        assert resume.wait(10)
        return original(*args)

    monkeypatch.setattr(store, 'run_pricing', blocked)
    revision = store.STATE_META['revision']
    old = client.post('/run', json={'kind': 'pricing', 'books': ['loans']}).json()['id']
    try:
        assert entered.wait(10)
        rows = store.BOOKS['loans'].to_dicts()
        for row in rows:
            row['face'] *= 2
        assert client.put('/books/loans', json=json.loads(json.dumps(rows, default=str))).status_code == 200
    finally:
        resume.set()
    assert finished(old)['status'] == 'done'
    old_result = skeleton(store.job_result(old))
    assert old_result['revision'] == revision
    new = client.post('/run', json={'kind': 'pricing', 'books': ['loans']}).json()['id']
    assert finished(new)['status'] == 'done'
    new_result = skeleton(store.job_result(new))
    assert new_result['revision'] == revision + 1
    assert new_result['totals']['loans'] == pytest.approx(2 * old_result['totals']['loans'])
    assert sum(s['computed'] for s in new_result['graph'].values()) == 0


def test_expired_job_status_is_404(client):
    store.JOBS["expired"] = dict(id="expired", kind="nii", status="done", finished_at=0, result=b"old")
    assert client.get("/jobs/expired").status_code == 404


def test_queue_capacity_is_reported_as_retryable(client, monkeypatch):
    monkeypatch.setattr(store, "MAX_QUEUE", 0)
    response = client.post("/run", json={"kind": "nii"})
    assert response.status_code == 429
    assert response.headers["retry-after"] == "5"


def test_invalid_program_rejected_without_mutation(client):
    before = store.PROGRAMS.copy()
    assert client.put("/programs/bad", json={"name": "bad", "term_m": 0}).status_code == 422
    assert store.PROGRAMS == before


def test_optimizer_computes_earnings_for_each_market(client, monkeypatch):
    unitlib = importlib.import_module("portfolio_risk.strategy.unitlib")
    kpis = importlib.import_module("portfolio_risk.analytics.kpis")
    optimizer = importlib.import_module("portfolio_risk.strategy.optimizer")
    nii_calls, capital_inputs = [], []
    monkeypatch.setattr(store, "base_calibration", lambda state: {})
    monkeypatch.setattr(unitlib, "build_unit_library", lambda *a, **kw: {"units": []})
    def nii(sr, vp, **kw):
        nii_calls.append(float(sr[0]))
        return {"monthly": float(sr[0])}
    monkeypatch.setattr(store, "run_nii", nii)
    def compute(*args, **kwargs):
        capital_inputs.append(kwargs["nii_monthly"])
        return {"nii_total_$": kwargs["nii_monthly"]}
    monkeypatch.setattr(kpis, "compute_kpis", compute)
    monkeypatch.setattr(optimizer, "optimize_balance_sheet", lambda *a, **kw: {"feasible": True})
    store.SCENARIOS["up"] = main.MarketScenario(name="up", ust10y_bp=[100])
    state = store.snapshot()
    token = store._RUN_STATE.set(state)
    try:
        store.run_optimize_job(state["market"]["swap_rates"], state["market"]["vol_pts"], {"scenarios": ["up"]})
    finally:
        store._RUN_STATE.reset(token)
    assert len(nii_calls) == 2
    assert capital_inputs == nii_calls
    assert capital_inputs[1] == pytest.approx(capital_inputs[0] + .01)


def test_real_engine_jobs_and_interactive_cache_roundtrip(client):
    settings = client.get("/settings").json() | {"n_paths": 33, "n_paths_base": 33, "horizon_months": 6, "n_threads": 2}
    assert client.put("/settings", json=settings).status_code == 200
    assert client.put("/scenarios/up", json={"name": "up", "ust10y_bp": [100]}).status_code == 200
    requests = [dict(kind="risk"), dict(kind="risk", scenario="up"),
                dict(kind="stress", scenario="up", books=["mbs", "deposits"]),
                dict(kind="nii"), dict(kind="nii", scenario="up"),
                dict(kind="kpis", scenario="up"), dict(kind="unitlib")]
    for payload in requests:
        response = client.post("/run", json=payload)
        assert response.status_code == 200, response.text
        job = finished(response.json()["id"])
        assert job["status"] == "done", (payload, job["detail"])
        blob = client.get(f"/jobs/{job['id']}/result").content
        assert blob.startswith(b"ARW1")
        if payload["kind"] == "risk":
            assert "hedges" in skeleton(blob)
        if payload["kind"] == "unitlib":
            assert store.CACHE["library"]["horizon"] == 6
    result = client.post("/strategy/eval", json=[dict(template="agency_mbs", purchase_m=2, notional=1e6)])
    assert result.status_code == 200
    assert len(skeleton(result.content)["nii_incremental"]) == 6
    response = client.post("/optimize", json={"scenarios": ["up"], "lcr_min": .1, "nsfr_min": .1,
                                              "cet1_min": .01, "eve_limit": 1, "max_total_assets": 1e7})
    assert response.status_code == 200, response.text
    job = finished(response.json()["id"])
    assert job["status"] == "done", job["detail"]
    result = skeleton(client.get(f"/jobs/{job['id']}/result").content)
    assert result["feasible"] and result["validated"]


@pytest.mark.parametrize("name", ["mbs", "loans", "debt", "deposits", "cds", "mm"])
def test_book_json_roundtrip(client, name):
    original = store.BOOKS[name]
    rows = json.loads(store._arrow_safe(original).write_json())
    response = client.put(f"/books/{name}", json=rows)
    assert response.status_code == 200, response.text
    assert len(store.BOOKS[name]) == len(original)
    if "maturity" in original.columns:
        assert store.BOOKS[name].schema["maturity"] == original.schema["maturity"]
    # Validate the saved frame a second time to catch schedule corruption.
    assert client.put(f"/books/{name}", json=rows).status_code == 200


def test_bad_edit_preserves_previous_book(client):
    original = store.BOOKS["loans"]
    response = client.put("/books/loans", json=[{"id": "invalid"}])
    assert response.status_code == 422
    assert store.BOOKS["loans"] is original
    assert store.STATE_META["revision"] == 0


@pytest.mark.parametrize("value", ["NaN", "Infinity", None])
def test_numeric_string_nonfinite_edits_are_rejected(client, value):
    rows = json.loads(store._arrow_safe(store.BOOKS["loans"]).write_json())
    rows[0]["face"] = value
    assert client.put("/books/loans", json=rows).status_code == 422
    assert store.STATE_META["revision"] == 0


def test_empty_book_retains_schema(client):
    schema = store.BOOKS["loans"].schema
    assert client.put("/books/loans", json=[]).status_code == 200
    assert store.BOOKS["loans"].schema == schema
    assert client.get("/books").status_code == 200


def test_edits_invalidate_strategy_bundle(client):
    store.CACHE.update(revision=0, library={}, base={})
    response = client.put("/settings", json={"n_paths": 33, "n_threads": 2})
    assert response.status_code == 200
    assert not store.CACHE
    assert client.post("/strategy/eval", json=[]).status_code == 409


def test_failed_rebuild_never_publishes_half_cache(client, monkeypatch):
    unitlib = importlib.import_module("portfolio_risk.strategy.unitlib")
    monkeypatch.setattr(unitlib, "build_unit_library", lambda *a, **k: {"units": [], "templates": {}, "grid_m": [], "horizon": 27})
    def fail(*args):
        raise RuntimeError("controlled KPI failure")
    monkeypatch.setattr(store, "run_kpis", fail)
    previous = {"revision": 0, "library": {"old": True}, "base": {"old": True}}
    store.CACHE.update(previous)
    with pytest.raises(RuntimeError, match="controlled"):
        store.build_unitlib_job(None, None)
    assert store.CACHE == previous


def test_changed_inputs_discard_completed_rebuild(client, monkeypatch):
    unitlib = importlib.import_module("portfolio_risk.strategy.unitlib")
    monkeypatch.setattr(unitlib, "build_unit_library", lambda *a, **k: {"units": [], "templates": {}, "grid_m": [], "horizon": 27})
    def change(*args):
        with store._LOCK:
            store.changed()
        return {"new": True}
    monkeypatch.setattr(store, "run_kpis", change)
    with pytest.raises(RuntimeError, match="inputs changed"):
        store.build_unitlib_job(None, None)
    assert not store.CACHE


def test_job_snapshot_and_actual_runtime_configuration(client):
    from portfolio_risk.core.runtime import path_count
    started, release = threading.Event(), threading.Event()
    blocker = store.submit("blocker", lambda: (started.set(), release.wait(5)))
    assert started.wait(5)
    response = client.put("/settings", json={"seed": 7, "n_paths": 33, "n_paths_base": 65, "n_threads": 1})
    assert response.status_code == 200
    jid = store.submit("probe", lambda: {"seed": store.current_state()["settings"].seed,
        "paths": path_count(128), "base_paths": path_count(512, base=True), "threads": numba.get_num_threads()})
    client.put("/settings", json={"seed": 999, "n_paths": 2048, "n_threads": 0})
    release.set()
    assert finished(blocker)["status"] == "done"
    assert finished(jid)["status"] == "done"
    result = skeleton(store.job_result(jid))
    assert result == {"seed": 7, "paths": 33, "base_paths": 65, "threads": 1}
    next_id = store.submit("probe", lambda: {"threads": numba.get_num_threads()})
    assert finished(next_id)["status"] == "done"
    assert skeleton(store.job_result(next_id))["threads"] == numba.config.NUMBA_NUM_THREADS


def test_results_serialized_once_and_retained_with_bounds(client, monkeypatch):
    monkeypatch.setattr(store, "MAX_JOBS", 2)
    ids = []
    for i in range(3):
        jid = store.submit("probe", lambda: {"value": 1})
        assert finished(jid)["status"] == "done"
        ids.append(jid)
    assert ids[0] not in store.JOBS
    blob = store.job_result(ids[-1])
    assert store.job_result(ids[-1]) is blob
    assert client.get(f"/jobs/{ids[-1]}/result").content == blob


@pytest.mark.parametrize("path,body", [
    ("/strategy/eval", [{"template": "unknown", "purchase_m": 0, "notional": 1}]),
    ("/strategy/eval", [{"template": "agency_mbs", "purchase_m": -1, "notional": 1}]),
    ("/optimize", {"scenarios": ["missing"]}),
    ("/optimize", {"lcr_min": -1}),
])
def test_invalid_requests_are_rejected(client, path, body):
    assert client.post(path, json=body).status_code == 422


def test_invalid_assumption_patch_is_atomic(client):
    before = store.snapshot()["assumptions"]
    response = client.put("/assumptions", json={"deposit_segments": {"DDA": {"base": .2}}, "cd_ew_params": [1]})
    assert response.status_code == 422
    assert store.ASSUMPTIONS == before


def test_deposit_flight_amplitude_is_a_multiplier_not_a_rate(client):
    # the segment defaults run from 1.5 to 4.0, and the native deck only requires amp >= 0
    assert client.put("/assumptions", json={"deposit_segments": {"MMDA": {"amp": 4.5}}}).status_code == 200
    assert store.ASSUMPTIONS["deposit_segments"]["MMDA"]["amp"] == 4.5
    before = store.snapshot()["assumptions"]
    assert client.put("/assumptions", json={"deposit_segments": {"MMDA": {"amp": -0.1}}}).status_code == 422
    assert client.put("/assumptions", json={"deposit_segments": {"MMDA": {"base": 1.5}}}).status_code == 422
    assert store.ASSUMPTIONS == before


def test_nonfinite_skeleton_is_valid_json():
    assert skeleton(store.to_arrow_envelope({"x": float("nan")})) == {"x": None}


def test_state_fingerprints_change_only_for_the_edited_input(client):
    """The client's recalculation graph diffs these per input node, so an edit must
    move exactly the node it touched and leave the others alone."""
    def nodes():
        body = client.get("/state").json()
        assert body["inputs"]["revision"] == body["revision"]
        return body["inputs"]["nodes"]
    before = nodes()
    assert {"market", "settings", "assumptions:deposits", "assumptions:cds", "scenarios", "context"} <= before.keys()

    assert client.put("/assumptions", json={"deposit_segments": {"DDA": {"base": 0.02}}}).status_code == 200
    after = nodes()
    assert {k for k in before if before[k] != after.get(k)} == {"assumptions:deposits"}

    settings = client.get("/settings").json()
    assert client.put("/settings", json={**settings, "seed": settings["seed"] + 1}).status_code == 200
    final = nodes()
    assert {k for k in after if after[k] != final.get(k)} == {"settings"}
    # an unchanged snapshot hashes the same on every call
    assert nodes() == final
