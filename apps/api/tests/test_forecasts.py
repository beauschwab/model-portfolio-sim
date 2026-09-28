"""Published source contracts and immutable forecast execution boundary."""
from datetime import date
import io
import json
import httpx
import openpyxl
import pytest
from fastapi.testclient import TestClient
from app import main, market_data as md, store
from app.forecast_data import metadata


@pytest.fixture(autouse=True)
def storage(tmp_path, monkeypatch):
    monkeypatch.setenv("MARKET_DATA_DIR", str(tmp_path))


def request(name, day=date(2026,9,28)):
    return md.FetchRequest(dataset=name, as_of=day)


def transport(fn):
    def call(req):
        v=fn(req)
        return httpx.Response(200, content=v if isinstance(v,bytes) else v.encode())
    return httpx.MockTransport(call)


def xlsx(sheets):
    w=openpyxl.Workbook();w.remove(w.active)
    for name, rows in sheets.items():
        s=w.create_sheet(name)
        for row in rows:s.append(row)
    b=io.BytesIO();w.save(b);return b.getvalue()


def sep_page():
    rows = [["Variable","Median1","Central Tendency2","Range3"],
            ["2026","2027","2028","Longer run"]*3]
    for field in ["Change in real GDP","Unemployment rate","PCE inflation","Core PCE inflation4","Federal funds rate"]:
        rows.append([field,"4.1","3.6","3.1","3.0"]+["3.1–4.1"]*8)
        rows.append(["June projection"]+["9.9"]*12)
    return '<table>'+''.join('<tr>'+''.join('<td>'+x+'</td>' for x in r)+'</tr>' for r in rows)+'</table>'


def test_sep_selects_published_release_ignores_previous_comparison_and_preserves_future():
    urls = '<a href="/monetarypolicy/fomcprojtabl20260916.htm">SEP</a><a href="/monetarypolicy/fomcprojtabl20261209.htm">Future</a>'
    s=md.fetch_snapshot(request("fed_sep"),transport(lambda r:urls if r.url.path.endswith('fomccalendars.htm') else sep_page()))
    assert all(r['value'] != 9.9 for r in s['observations'])
    assert any(r['date']=='2028-12-31' for r in s['observations'])
    assert {r['release_date'] for r in s['observations']} == {'2026-09-16'}
    assert metadata(s)['runnable'] == ['median']
    assert all('20261209' not in x['url'] for x in s['sources'])


def test_spf_skips_lagged_actual_and_fails_for_unavailable_vintage():
    page='last update: August 14, 2026 <a href="/medianLevel.xlsx">Data</a>'
    sheets={k:[['YEAR','QUARTER']+[f'{k}{i}' for i in range(1,7)], [2026,3,99,4,3,2,1,.5]]
            for k in ['TBILL','TBOND','BOND','BAABOND','UNEMP']}
    t=transport(lambda r:xlsx(sheets) if r.url.path.endswith('xlsx') else page)
    with pytest.raises(md.DataError,match='after as_of'):
        md.fetch_snapshot(request('philly_spf',date(2026,8,1)),t)
    s=md.fetch_snapshot(request('philly_spf'),t)
    first=s['observations'][0]
    assert first['date']=='2026-07-01' and first['value']==4
    assert len(s['observations'])==25
    assert not any(r['value']==99 for r in s['observations'])


def test_sme_decimal_units_panel_statistic_and_embargo():
    page='<a href="/2026/jul-2026-data.xlsx">July</a><a href="/2026/sep-2026-data.xlsx">September</a>'
    h=['panel_type','question_tag','horizon_date','aggregation','aggregation_value','survey_release_date','horizon']
    data=xlsx({'Sheet1':[h,['Combined','fftr_pathofmodes','2027-12-31','pctl50',.035,'2026-07-15','2027'],
                          ['Combined','fftr_pathofmodes','2027-12-31','count',55,'2026-07-15','2027'],
                          ['Dealers','fftr_pathofmodes','2027-12-31','pctl50',.04,'2026-07-15','2027']]})
    t=transport(lambda r:data if r.url.path.endswith('xlsx') else page)
    s=md.fetch_snapshot(request('nyfed_sme'),t)
    assert len(s['observations'])==1 and s['observations'][0]['value']==.035
    assert s['observations'][0]['unit']=='decimal_rate'
    assert s['observations'][0]['available_after']=='2026-09-01'
    assert not any('sep-2026' in x['url'] for x in s['sources'])
    with pytest.raises(md.DataError,match='cutoff'):
        md.fetch_snapshot(request('nyfed_sme',date(2026,8,1)),t)


def fed_snapshot():
    page='<a href="/2026_Final_Supervisory_Baseline_Domestic.csv">Final</a>'
    csv='Scenario Name,Date,3-month Treasury rate,10-year Treasury yield\nBaseline,2026 Q1,4,4.5\nBaseline,2026 Q2,3,4\n'
    return md.fetch_snapshot(request('fed_stress'),transport(lambda r:page if r.url.path.endswith('htm') else csv))


def test_preview_uses_all_rows_before_pagination_and_does_not_mutate_inputs():
    s=fed_snapshot()
    with TestClient(main.app) as c:
        before=store.snapshot();rev=before['revision']
        visible=c.get(f'/market-data/snapshots/{s["id"]}?limit=1').json()
        assert len(visible['observations'])==1
        assert visible['forecast']['periods']==['2026-01-01','2026-04-01']
        body=dict(snapshot_id=s['id'],scenario='baseline',start_period='2026-01-01',horizon_months=9,expected_revision=rev)
        p=c.post('/forecasts/preview',json=body)
        assert p.status_code==200,p.text
        assert p.json()['drivers'][3]['short_rate']==.03
        assert p.json()['coverage']['short_rate']['tail_months_in_report']==3
        assert c.post('/forecasts/run',json={**body,'expected_revision':rev+1}).status_code==409
        assert c.post('/forecasts/preview',json={**body,'start_period':'2026-02-01'}).status_code==422
        assert c.post('/forecasts/preview',json={**body,'scenario':'p25'}).status_code==422
        assert store.snapshot()['revision']==rev
        assert store.BOOKS['mbs'].equals(before['books']['mbs'])


def test_run_queues_frozen_plan_and_provenance(monkeypatch):
    s=fed_snapshot();calls=[]
    def submit(kind,fn,*args,**kwargs):
        calls.append((kind,fn,args,kwargs));return 'test-job'
    with TestClient(main.app) as c:
        monkeypatch.setattr(store,'submit',submit)
        monkeypatch.setattr(store,'job_status',lambda _:dict(id='test-job',kind='forecast_nii',status='queued',revision=store.snapshot()['revision']))
        body=dict(snapshot_id=s['id'],scenario='baseline',start_period='2026-01-01',horizon_months=9,expected_revision=store.snapshot()['revision'])
        r=c.post('/forecasts/run',json=body)
        assert r.status_code==200,r.text
        assert calls[0][0]=='forecast_nii'
        assert calls[0][2][1]['snapshot_id']==s['id']
        assert calls[0][3]['state']['revision']==body['expected_revision']
