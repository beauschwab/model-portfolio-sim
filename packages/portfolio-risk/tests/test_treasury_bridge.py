from copy import deepcopy
import polars as pl
import pytest
from portfolio_risk.analytics.treasury import bridge, example
from portfolio_risk.analytics.journal import Journal


def ledger_case():
    j=Journal('baseline')
    j.post(0,'bank','opening',{('cash',''):100.,('asset_principal','loan'):900.,
        ('funding_principal','deposit'):-800.,('opening_equity',''):-200.})
    j.post(1,'bank','interest',{('cash',''):10.,('pnl:interest','loan'):-10.})
    j.post(1,'bank','tax',{('cash',''):-2.,('pnl:tax',''):2.})
    j.post(2,'bank','mark',{('fair_value_adjustment','loan'):-30.,('oci',''):30.})
    j.post(3,'bank','distribution',{('cash',''):-5.,('equity_distributions',''):5.})
    trial=pl.DataFrame([dict(scenario='baseline',account=a,gl_account=g,instrument_id=i,balance=v)
                       for (a,g,i),v in j.balances.items()])
    t=example();c=deepcopy(t['capital'][0])
    c.update(entity='bank',cet1_deductions=3.,additional_tier1=20.,tier2=10.,average_assets=1000.)
    t.update(capital=[],curves=[],positions=[])
    policy=dict(account='bank',scenario='baseline',capital=c,include_aoci=True,equity_exclusions=20.,intangible_assets=4.,
        leverage_addon=50.,leverage_deductions=3.,gl_risk_weights={'cash':0.},instrument_risk_weights={'loan':.5})
    return dict(kind='ledger',treasury=t,policies=[policy],amount_multiplier=1.),trial,j.to_frame() if hasattr(j,'to_frame') else pl.DataFrame(j.rows)


def flow_case():
    t=example();t.update(capital=[],positions=[])
    o=pl.DataFrame([dict(book='loans',id='a',balance=100.,book_adjustment=5.,side='asset',market_price=1.)])
    f=pl.DataFrame([dict(book='loans',id='a',month=m,principal=p,cash_interest=1.,accrual_interest=1.,book_amortization=-.1)
                    for m,p in [(1,20.),(2,30.)]])
    mapping=dict(book='loans',id='a',entity='demo-bank',currency='USD',scenario='base',curve_id=t['curves'][0]['id'],
        loan_id='original-loan',cohort_id='cohort-a',repricing_years=None,residual_tail_years=1.,
        operating_cost_rate=0.,expected_loss_rate=0.,capital_ratio=.1,cost_of_capital=.12,
        option_spread=0.,annual_fee_rate=0.,contingent_charge_rate=0.)
    return dict(kind='cashflows',treasury=t,mappings=[mapping],horizon=2),o,f


def test_journal_capital_components_include_tax_oci_and_distribution_once():
    spec,trial,journal=ledger_case()
    out=bridge(spec,trial_balance=trial,journal_frames=[journal.head(3),journal.slice(3)])
    audit=out['capital_bridge'].row(0,named=True)
    assert audit['assets']==973. and audit['liabilities']==800. and audit['book_equity']==173.
    assert audit['common_equity']==175. and audit['retained_earnings']==8.
    assert audit['eligible_aoci']==-30. and audit['credit_rwa']==435.
    ratios={r['metric']:r for r in out['capital_metrics'].to_dicts()}
    assert ratios['cet1']['numerator']==150.
    assert ratios['slr']['numerator']==170. and ratios['slr']['denominator']==1020.
    assert ratios['tce_ta']['numerator']==149. and ratios['tce_ta']['denominator']==969.
    assert out['source_verification']['journal_replayed']
    spec['policies'][0]['include_aoci']=False
    other=bridge(spec,trial_balance=trial,journal_frames=[journal])
    assert other['capital_metrics'].filter(pl.col('metric')=='cet1')['numerator'][0]==180.


def test_ledger_amount_conversion_and_corruption_rejection():
    spec,trial,journal=ledger_case();spec['amount_multiplier']=1000.
    out=bridge(spec,trial_balance=trial,journal_frames=[journal])
    assert out['capital_bridge']['assets'][0]==973000.
    bad=trial.with_columns((pl.col('balance')+1).alias('balance'))
    with pytest.raises(ValueError,match='journal/trial'):bridge(spec,trial_balance=bad,journal_frames=[journal])
    spec['policies'][0]['instrument_risk_weights']={}
    with pytest.raises(ValueError,match='risk-weight'):bridge(spec,trial_balance=trial,journal_frames=[journal])


def test_cashflow_balances_remaining_wal_and_income_match_hand_calculation():
    spec,openings,flows=flow_case()
    out=bridge(spec,openings=openings,cashflows=flows)
    a,b=out['ftp_preparation'].to_dicts()
    assert a['average_balance']==90. and b['average_balance']==65.
    assert a['funding_years']==pytest.approx(.65)
    assert b['funding_years']==pytest.approx(17/24)
    assert a['tail_share']==.5 and b['tail_share']==.625
    assert out['ftp_positions']['external_profit'].to_list()==pytest.approx([.9,.9])
    assert out['ftp_positions']['loan_id'].to_list()==['original-loan']*2
    assert out['ftp_positions']['cohort_id'].to_list()==['cohort-a']*2
    assert out['ftp_positions']['capital_cost'].to_list()==pytest.approx([.09,.065])
    assert out['ftp_reconciliation']['elimination_error'].abs().max()<1e-12


@pytest.mark.parametrize('case',['tail','missing','duplicate','overpay','negative','mapping','opening'])
def test_bad_captured_flows_fail_closed(case):
    spec,o,f=flow_case()
    if case=='tail':spec['mappings'][0]['residual_tail_years']=None
    if case=='missing':f=f.head(1)
    if case=='duplicate':f=pl.concat([f,f.head(1)])
    if case=='overpay':f=f.with_columns(pl.lit(80.).alias('principal'))
    if case=='negative':f=f.with_columns(pl.lit(-1.).alias('principal'))
    if case=='mapping':spec['mappings'][0]['id']='wrong'
    if case=='opening':o=pl.concat([o,o])
    with pytest.raises(ValueError):bridge(spec,openings=o,cashflows=f)


def test_complete_runoff_needs_no_tail_and_floating_reset_is_independent():
    spec,o,f=flow_case();spec['mappings'][0].update(residual_tail_years=None,repricing_years=.25)
    f=f.with_columns(pl.Series('principal',[20.,80.]))
    out=bridge(spec,openings=o,cashflows=f)
    assert out['ftp_preparation']['funding_years'].to_list()==pytest.approx([.15,1/12])
    assert out['ftp_positions']['reference_rate'].to_list()==pytest.approx([.03975]*2)
    assert out['ftp_preparation']['tail_principal'].to_list()==[0.,0.]


def test_deposit_funding_life_cannot_silently_become_repricing_tenor():
    spec,o,f=flow_case();spec['mappings'][0]['book']='deposits'
    o=o.with_columns(pl.lit('deposits').alias('book'),pl.lit('liability').alias('side'))
    f=f.with_columns(pl.lit('deposits').alias('book'))
    with pytest.raises(ValueError,match='FTP_RESET_REQUIRED'):bridge(spec,openings=o,cashflows=f)
    spec['mappings'][0]['repricing_years']=.25
    out=bridge(spec,openings=o,cashflows=f)
    assert out['ftp_positions']['external_profit'].to_list()==pytest.approx([-.9,-.9])


def test_source_template_preserves_original_deposit_id_prefix():
    from portfolio_risk.analytics.treasury import source_template
    out=source_template('cashflows',[dict(book='deposits',id='HL-original-deposit'),
        dict(book='mbs',id='HL-HL-original-mortgage')],asof='2026-10-01',horizon=2,basis='individual_model_reprice')
    assert [r['loan_id'] for r in out['mappings']]==['HL-original-deposit','HL-original-mortgage']


@pytest.mark.parametrize('case',['afs','trading','credit','collateral','mixed'])
def test_actual_native_ledger_closing_matches_bridge(case):
    from test_ledger_native import fixture
    from portfolio_risk.analytics.balance_stress import run_balance_stress
    from portfolio_risk.core.runtime import RunConfig,run_context
    raw=fixture(case)
    with run_context(RunConfig(compute_backend='rust')):source=run_balance_stress(raw)
    t=example();base=deepcopy(t['capital'][0]);t.update(capital=[],curves=[],positions=[])
    assetgls=['cash','restricted_cash','asset_principal','book_adjustment','allowance','accrued_interest',
             'fair_value_adjustment','recovery_receivable','derivative_value','posted_margin','intercompany_receivable']
    policies=[]
    for a in source['closing_statements'].to_dicts():
        policies.append(dict(account=a['account'],scenario=a['scenario'],capital=base|dict(entity=a['account'],currency=a['currency']),include_aoci=True,
            equity_exclusions=0.,intangible_assets=0.,leverage_addon=0.,leverage_deductions=0.,
            gl_risk_weights={g:0. if g in ['cash','restricted_cash'] else 1. for g in assetgls},instrument_risk_weights={}))
    out=bridge(dict(kind='ledger',treasury=t,policies=policies,amount_multiplier=1.),
        trial_balance=source['trial_balance'],journal_frames=[source['journal']])
    expected={(r['scenario'],r['account']):r for r in source['closing_statements'].to_dicts()}
    for row in out['capital_bridge'].to_dicts():
        ref=expected[row['scenario'],row['account']]
        assert row['book_equity']==pytest.approx(ref['equity'],abs=1e-8)
        assert row['assets']==pytest.approx(ref['assets'],abs=1e-8)
        assert row['liabilities']==pytest.approx(ref['liabilities'],abs=1e-8)


@pytest.mark.parametrize('principal',[40.,100.])
def test_closing_date_loan_repayment_releases_allowance_in_both_engines(principal):
    from test_ledger_native import fixture
    from portfolio_risk.analytics.balance_stress import run_balance_stress
    from portfolio_risk.core.runtime import RunConfig,run_context
    from polars.testing import assert_frame_equal
    raw=fixture('credit');raw['cashflows'][0]['principal']=principal
    results=[]
    for backend in ['python','rust']:
        with run_context(RunConfig(compute_backend=backend)):out=run_balance_stress(raw)
        for scenario in ['baseline','stress']:
            trial=out['trial_balance'].filter((pl.col('scenario')==scenario)&(pl.col('instrument_id')=='s'))
            amounts=dict(zip(trial['gl_account'],trial['balance']))
            # Constant PD/LGD: allowance follows surviving, post-payment principal.
            assert -amounts['allowance']==pytest.approx(amounts['asset_principal']*.2*.45,abs=1e-10)
        assert out['journal'].filter(pl.col('event')=='repayment_allowance_release').height>0
        results.append(out)
    assert_frame_equal(results[0]['trial_balance'],results[1]['trial_balance'],check_exact=False,abs_tol=1e-9)
