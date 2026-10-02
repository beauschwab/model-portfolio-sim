import pytest
from app import store, persistence as db, treasury_store, cohort_store
from portfolio_risk.analytics.balance_stress import example_specification
from portfolio_risk.analytics.treasury import example
from test_persistence import durable, BACKENDS
from test_cohorts import execute, setup_tape

ASSET_GLS=['cash','restricted_cash','asset_principal','book_adjustment','allowance','accrued_interest',
           'fair_value_adjustment','recovery_receivable','derivative_value','posted_margin','intercompany_receivable']


def fill_ledger(spec):
    spec['amount_multiplier']=1.
    spec['treasury'].update(as_of='2026-10-01',policy_id='test-policy')
    for p in spec['policies']:
        for k,v in p.items():
            if v is None:p[k]=True if k=='include_aoci' else 0.
        for k,v in p['capital'].items():
            if v is None and k!='advanced_rwa':p['capital'][k]=0.
        p['capital']['period']='closing'
        p['gl_risk_weights']={g:0. if g in ['cash','restricted_cash'] else 1. for g in ASSET_GLS}
    return spec


@pytest.mark.parametrize('streamed',[False,True])
def test_completed_native_ledger_to_capital_and_restart(durable,monkeypatch,streamed):
    store.SETTINGS=store.SETTINGS.model_copy(update={'compute_backend':'rust','n_threads':2})
    raw=example_specification();raw.update(horizon_days=30,reverse_severities=[])
    if streamed:
        from portfolio_risk.analytics import balance_stream
        original=balance_stream.run_streamed_balance_stress
        monkeypatch.setattr(balance_stream,'run_streamed_balance_stress',lambda *a,**kw:original(*a,**kw,partition_rows=37))
        source,_=execute('streamed_balance_stress',store.run_streamed_balance_stress,
            dict(specification=raw,backend='rust',large_book=False,binary_sha256=db.identity()['balance_native_sha256']))
    else:source,_=execute('balance_stress',store.run_balance_stress,raw)
    tables=db.CODEC.tables(db.result_ref(source))
    if streamed:assert len(tables['/journal']['parts'])>1
    closing=treasury_store._table(tables,'/closing_statements',10000)
    spec=fill_ledger(treasury_store.template(source,'ledger')['specification'])
    jid,out=execute('treasury',treasury_store.run,source,spec)
    assert out['source_verification']['journal_replayed']
    assert out['source_receipt']['result_sha256']==db.result_ref(source)['sha256']
    expected={(r['scenario'],r['account']):r for r in closing.to_dicts()}
    for r in out['capital_bridge'].to_dicts():
        assert r['book_equity']==pytest.approx(expected[r['scenario'],r['account']]['equity'])
    db.stop();db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['source_receipt']==out['source_receipt']


@pytest.mark.parametrize('individual',[False,True])
def test_real_cohort_cashflows_prepare_ftp_with_lineage(durable,individual):
    store.SETTINGS=store.SETTINGS.model_copy(update={'compute_backend':'rust','n_paths':32,'n_paths_base':32,'n_threads':2,'horizon_months':3})
    build,_=setup_tape()
    source,values=execute('cohort_analytics',cohort_store.analytics,build,['mortgage','deposit'],
        ['mortgage-000','deposit-000'] if individual else None)
    spec=treasury_store.template(source,'cashflows')['specification']
    curve=example()['curves'][0]|{'as_of':spec['treasury']['as_of']}
    spec['treasury'].update(policy_id='test-ftp',curves=[curve])
    for p in spec['mappings']:
        p.update(entity=curve['entity'],curve_id=curve['id'],residual_tail_years=3.,
                 repricing_years=.25 if p['book']=='deposits' else None)
    _,out=execute('treasury',treasury_store.run,source,spec)
    assert out['ftp_positions'].height==values['cashflows'].height
    if individual:assert set(out['ftp_positions']['loan_id'])=={'mortgage-000','deposit-000'}
    else:assert out['ftp_positions']['cohort_id'].null_count()==0
    assert out['ftp_positions']['external_profit'].sum()==pytest.approx(values['nii']['nii'].sum(),abs=1e-8)
    assert out['ftp_reconciliation']['elimination_error'].abs().max()<1e-8
    assert out['source_receipt']['revision']==values['revision']
    spec['treasury']['as_of']='2000-01-01'
    with pytest.raises(ValueError,match='date'):treasury_store.run(source,spec)


def test_unsupported_or_unpublished_source_fails(durable):
    source,_=execute('treasury',store.run_treasury,example())
    with pytest.raises(ValueError,match='compatible'):treasury_store.template(source,'cashflows')


@pytest.mark.parametrize('failure',['corruption','cancel'])
def test_source_failure_publishes_no_report(durable,monkeypatch,failure):
    from app import worker
    import polars as pl
    raw=example_specification();raw.update(horizon_days=30,reverse_severities=[])
    source,_=execute('balance_stress',store.run_balance_stress,raw)
    spec=fill_ledger(treasury_store.template(source,'ledger')['specification'])
    jid=store.submit('treasury',treasury_store.run,source,spec)
    original=treasury_store._frames
    def interrupted(ref):
        for frame in original(ref):
            if 'transaction_id' in frame.columns:
                if failure=='cancel':db.REPO.cancel(jid)
                else:frame=frame.with_columns((pl.col('debit')+1.).alias('debit'))
            yield frame
    monkeypatch.setattr(treasury_store,'_frames',interrupted)
    worker.execute(db.REPO.claim(db.WORKER))
    row=db.REPO.job(jid)
    assert row['status']!='done' and row['result'] is None
