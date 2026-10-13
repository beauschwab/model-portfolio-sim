"""Independent reviewed signed-interest-basis closing-capital hand calculations."""
import polars as pl
import pytest
from portfolio_risk.analytics.treasury import bridge
from test_treasury_bridge import ledger_case


def case(gross_gl,basis_gl,basis):
    specification,_,_=ledger_case()
    values={'cash':100.,gross_gl:10.,basis_gl:basis,
        'pnl:original_effective_interest':-3.,'pnl:reviewed_basis_realization':-1.,
        'opening_equity':-(106.+basis)}
    journal=pl.DataFrame([dict(transaction_number=1,scenario='baseline',day=0,
        account='bank',event='reviewed_signed_rights',gl_account=gl,
        instrument_id='' if gl in {'cash','opening_equity'} else 'loan',value=value)
        for gl,value in values.items()])
    trial=journal.select('scenario','account','gl_account','instrument_id',pl.col('value').alias('balance'))
    policy=specification['policies'][0]
    policy.update(equity_exclusions=0.,intangible_assets=0.,leverage_addon=0.,leverage_deductions=0.,
        instrument_risk_weights={},gl_risk_weights={'cash':0.,gross_gl:1.,basis_gl:1.})
    return specification,trial,journal


@pytest.mark.parametrize('gross_gl,basis_gl',[
    ('dated_interest_receivable','live_interest_basis_adjustment'),
    ('credit_default_interest_receivable','default_interest_basis_adjustment')])
@pytest.mark.parametrize('basis,assets,rwa', [(-10.,100.,0.),(2.,112.,12.)])
def test_reviewed_gross_interest_and_signed_basis_remain_asset_accounts(gross_gl,basis_gl,basis,assets,rwa):
    raw,trial,journal=case(gross_gl,basis_gl,basis)
    out=bridge(raw,trial_balance=trial,journal_frames=[journal],ledger_journal_protocol=3)
    closing=out['capital_bridge'].row(0,named=True)
    assert closing['assets']==assets and closing['liabilities']==0.
    assert closing['credit_rwa']==rwa and closing['retained_earnings']==4.
    assert closing['book_equity']==assets and closing['reconciliation_error']==0.
    contributions={r['gl_account']:r for r in out['rwa_contributions'].to_dicts()}
    assert contributions[gross_gl]['balance']==10.
    assert contributions[basis_gl]['balance']==basis
    assert contributions[basis_gl]['rwa_contribution']==basis
    assert out['source_verification']['journal_replayed']


@pytest.mark.parametrize('basis_gl,gross_gl',[
    ('live_interest_basis_adjustment','dated_interest_receivable'),
    ('default_interest_basis_adjustment','credit_default_interest_receivable')])
def test_interest_basis_requires_explicit_nonzero_weight_and_preserves_reviewed_weights(basis_gl,gross_gl):
    raw,trial,journal=case(gross_gl,basis_gl,2.)
    raw['policies'][0]['gl_risk_weights'].pop(basis_gl)
    with pytest.raises(ValueError,match='risk-weight'):
        bridge(raw,trial_balance=trial,journal_frames=[journal],ledger_journal_protocol=3)
    raw['policies'][0]['gl_risk_weights'][basis_gl]=.5
    out=bridge(raw,trial_balance=trial,journal_frames=[journal],ledger_journal_protocol=3)
    assert out['capital_bridge']['credit_rwa'][0]==11.
    raw['policies'][0]['instrument_risk_weights']['loan']=.25
    out=bridge(raw,trial_balance=trial,journal_frames=[journal],ledger_journal_protocol=3)
    assert out['capital_bridge']['credit_rwa'][0]==3.
