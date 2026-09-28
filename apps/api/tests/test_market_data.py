"""Offline source contracts, immutable snapshots and atomic market application."""
from datetime import date, timedelta
import hashlib
import json
import math
import time

from fastapi.testclient import TestClient
import httpx
import numpy as np
import pytest

from app import main, market_data as md, store

DAY = date(2026, 9, 25)


@pytest.fixture(autouse=True)
def storage(tmp_path, monkeypatch):
    monkeypatch.setenv("MARKET_DATA_DIR", str(tmp_path))


def request(dataset, **kwargs):
    return md.FetchRequest(dataset=dataset, as_of=DAY, start=date(2026, 1, 1), **kwargs)


def transport(routes):
    def handle(req):
        val = routes(req) if callable(routes) else routes[req.url.path]
        return httpx.Response(200, text=val) if isinstance(val, str) else val
    return httpx.MockTransport(handle)


def eris_transport(bad=False):
    filename = "Eris_20260925_EOD_DiscountFactors_SOFR.csv"
    lines = ["Date,DiscountFactor"]
    for year in range(51):
        d = DAY + timedelta(days=365 * year)
        lines.append(f"{d:%m/%d/%Y},{math.exp(-.04 * year) if not bad else -1}")
    return transport({"/ftp/": f'<a href="{filename}">curve</a>', "/ftp/" + filename: "\n".join(lines)})


def test_curve_snapshot_provenance_units_and_integrity(tmp_path):
    s = md.fetch_snapshot(request("eris_sofr"), eris_transport())
    assert s["curve"]["swap_rates"] == pytest.approx([math.expm1(.04)] * 10)
    assert s["curve"]["classification"] == "derived"
    assert md.get_snapshot(s["id"]) == s
    for source in s["sources"]:
        raw = (tmp_path / "raw" / (source["sha256"] + ".bin")).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == source["sha256"]
    path = tmp_path / "snapshots" / (s["id"] + ".json")
    path.write_text('{}')
    with pytest.raises(md.DataError, match="integrity"):
        md.get_snapshot(s["id"])


def test_no_future_or_stale_eris_fallback():
    t = transport(lambda req: '<a href="Eris_20260928_EOD_DiscountFactors_SOFR.csv">future</a>')
    with pytest.raises(md.DataError, match="seven days"):
        md.fetch_snapshot(request("eris_sofr"), t)


def test_malformed_discount_curve_never_saved():
    with pytest.raises(md.DataError):
        md.fetch_snapshot(request("eris_sofr"), eris_transport(True))
    assert md.list_snapshots() == []


def test_nyfed_percent_conversion_and_window():
    t = transport(lambda req: json.dumps({"refRates": [
        {"effectiveDate": "2026-09-25", "percentRate": 4.5},
        {"effectiveDate": "2026-09-28", "percentRate": 5}]}))
    s = md.fetch_snapshot(request("nyfed", series=["SOFR"]), t)
    assert [r["value"] for r in s["observations"]] == [.045]
    assert s["observations"][0]["unit"] == "decimal_rate"


def test_pmms_missing_values_are_not_zero():
    t = transport(lambda r: "date,pmms30,pmms15\n9/25/2026,6.5, \n")
    s = md.fetch_snapshot(request("pmms"), t)
    assert len(s["observations"]) == 1
    assert s["observations"][0]["value"] == .065
    assert any("vintage" in w for w in s["warnings"])


def test_fhfa_filters_geography_flavor_and_frequency():
    t = transport(lambda r: "hpi_type,hpi_flavor,frequency,place_id,yr,period,index_nsa,index_sa\n"
        "traditional,purchase-only,monthly,USA,2026,7,300,301\n"
        "traditional,purchase-only,monthly,CA,2026,7,400,401\n"
        "traditional,all-transactions,monthly,USA,2026,7,500,501\n")
    s = md.fetch_snapshot(request("fhfa"), t)
    assert [r["value"] for r in s["observations"]] == [301, 300]


def test_fred_vintage_units_and_secret_redaction(monkeypatch):
    monkeypatch.setenv("FRED_API_KEY", "test-key-never-save")
    seen = []
    def route(req):
        seen.append(req)
        if req.url.path.endswith("observations"):
            assert req.url.params["realtime_end"] == str(DAY)
            return json.dumps({"count": 2, "observations": [
                {"date": "2026-09-25", "value": "3.2"}, {"date": "2026-09-24", "value": "."}]})
        return json.dumps({"seriess": [{"id": "DPRIME", "units": "Percent", "notes": "Provider copyright"}]})
    s = md.fetch_snapshot(request("fred", series=["DPRIME"]), transport(route))
    assert s["observations"][0]["value"] == 3.2
    assert s["observations"][0]["unit"] == "Percent"
    assert "test-key-never-save" not in json.dumps(s)
    assert s["sources"][-1]["series_metadata"]["notes"] == "Provider copyright"
    def fail(req):
        raise httpx.ConnectError("secret " + str(req.url), request=req)
    with pytest.raises(md.DataError) as err:
        md.fetch_snapshot(request("fred"), httpx.MockTransport(fail))
    assert "test-key" not in str(err.value)


def test_sec_excludes_filings_after_asof_and_retains_context(monkeypatch):
    monkeypatch.setenv("SEC_USER_AGENT", "Research research@example.test")
    row = {"end": "2026-06-30", "val": 100, "accn": "one", "form": "10-Q", "filed": "2026-08-01"}
    t = transport(lambda r: json.dumps({"facts": {"us-gaap": {"Assets": {"units": {"USD": [
        row, row | {"val": 200, "accn": "two", "filed": "2026-10-01"}]}}}}}))
    s = md.fetch_snapshot(request("sec", identifier="72971", series=["Assets"]), t)
    assert len(s["observations"]) == 1
    assert s["observations"][0]["accession"] == "one"


def test_fed_stress_retains_future_scenario_periods():
    t = transport(lambda r: '<a href="/supervisionreg/files/2026_Final_Supervisory_Baseline_Domestic.csv">Final</a>'
        if r.url.path.endswith("htm") else 'Scenario Name,Date,Unemployment rate,House Price Index (Level)\nSupervisory Baseline,2027 Q1,6,300\n')
    s = md.fetch_snapshot(request("fed_stress"), t)
    assert s["observations"][0]["date"] == "2027-01-01"
    assert s["observations"][0]["classification"] == "assumed"
    assert s["observations"][1]["unit"] == "index"


def test_fdic_rates_reads_rate_not_cap():
    t = transport(lambda r: '<p>PRESS RELEASE / September 25, 2026</p><p>Revised Rule September 21, 2026</p><table><tr><th>Product</th><th>National Deposit Rates2</th><th>National Rate Cap</th></tr>'
        '<tr><td>Savings</td><td>0.37</td><td>4.38</td></tr></table>')
    s = md.fetch_snapshot(request("fdic_rates"), t)
    assert s["observations"][0]["value"] == pytest.approx(.0037)
    assert s["observations"][0]["date"] == "2026-09-21"


@pytest.mark.parametrize("dataset,text", [("pmms", "<html>upstream error</html>"),
    ("fed_zero", "invalid file"), ("treasury", "<!DOCTYPE x><x/>")])
def test_unexpected_upstream_payload_fails_closed(dataset, text):
    with pytest.raises(md.DataError):
        md.fetch_snapshot(request(dataset), transport(lambda r: text))


def test_http_failure_and_missing_config(monkeypatch):
    monkeypatch.delenv("FRED_API_KEY", raising=False)
    with pytest.raises(md.DataError, match="FRED_API_KEY"):
        md.fetch_snapshot(request("fred"))
    with pytest.raises(md.DataError, match="HTTP 429"):
        md.fetch_snapshot(request("nyfed", series=["SOFR"]), transport(lambda r: httpx.Response(429)))


def test_apply_is_atomic_invalidates_cache_and_preserves_book_and_vol():
    s = md.fetch_snapshot(request("eris_sofr"), eris_transport())
    with TestClient(main.app) as c:
        before = store.snapshot()
        revision = before["revision"]
        store.CACHE["sentinel"] = True
        conflict = c.put("/market-data/active-curve", json={"snapshot_id": s["id"], "expected_revision": revision + 1})
        assert conflict.status_code == 409
        assert store.STATE_META["revision"] == revision
        response = c.put("/market-data/active-curve", json={"snapshot_id": s["id"], "expected_revision": revision})
        assert response.status_code == 200, response.text
        assert not store.CACHE
        after = store.snapshot()
        assert after["revision"] == revision + 1
        assert after["asof"] == before["asof"]
        np.testing.assert_array_equal(after["market"]["vol_pts"], before["market"]["vol_pts"])
        assert not np.array_equal(before["market"]["swap_rates"], after["market"]["swap_rates"])
        for name, book in before["books"].items():
            # Polars Object columns (call schedules) do not support frame.equals.
            assert store.to_arrow_envelope(book) == store.to_arrow_envelope(after["books"][name])
        assert before["market"].get("provenance", {}).get("snapshot_id") is None
        assert after["market"]["provenance"]["snapshot_id"] == s["id"]
        assert c.get(f"/market-data/snapshots/{s['id']}?limit=2").json()["observation_count"] == 51
        assert len(c.get(f"/market-data/snapshots/{s['id']}?limit=2").json()["observations"]) == 2
        assert c.get(f"/market-data/snapshots/{s['id']}/export").json()["id"] == s["id"]
        c.put("/market", json={"swap_rates": response.json()["swap_rates"], "vol_pts": response.json()["vol_pts"]})
        assert c.get("/market").json()["provenance"].get("snapshot_id") is None


def test_reference_snapshot_cannot_replace_curve():
    s = md.fetch_snapshot(request("pmms"), transport(lambda r: "date,pmms30,pmms15\n9/25/2026,6,5\n"))
    with TestClient(main.app) as c:
        response = c.put("/market-data/active-curve", json={"snapshot_id": s["id"], "expected_revision": store.STATE_META["revision"]})
        assert response.status_code == 422


def test_registered_import_and_bad_requests():
    with TestClient(main.app) as c:
        assert c.post("/market-data/fetch", json={"dataset": "bogus"}).status_code == 422
        assert c.post("/market-data/fetch", json={"dataset": "fdic", "identifier": "not-a-cert"}).status_code == 422
        assert c.post("/market-data/fetch", json={"dataset": "pmms", "url": "https://example.com"}).status_code == 422
        assert c.get("/market-data/snapshots/not-a-hash").status_code == 422
        result = c.post("/market-data/import", json={"dataset": "pooltalk", "as_of": str(DAY),
            "source_description": "Authorized aggregate pool extract", "observations": [
                dict(date=str(DAY), series="pool-factor", value=.95, unit="factor", classification="observed")]})
        assert result.status_code == 200
        assert result.json()["curve"] is None


def test_fetch_job_saves_without_changing_active_market(monkeypatch):
    actual = md.fetch_snapshot
    monkeypatch.setattr(md, "fetch_snapshot", lambda req: actual(req, eris_transport()))
    with TestClient(main.app) as c:
        before = c.get("/market").json()
        job = c.post("/market-data/fetch", json=request("eris_sofr").model_dump(mode="json")).json()
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            status = c.get(f"/jobs/{job['id']}").json()
            if status["status"] in ("done", "error"):
                break
            time.sleep(.01)
        assert status["status"] == "done", status
        assert c.get("/market").json() == before
        assert len(c.get("/market-data/snapshots").json()) == 1
