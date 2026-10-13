"""Independent credit GL hand cases for source-linked capital, not regulation."""
from copy import deepcopy
import polars as pl
import pytest
from portfolio_risk.analytics.treasury import bridge,example


def credit_case():
    journal=[];balances={}
    transactions=[
        {'cash':10.,'asset_principal':80.,'credit_default_principal':20.,'book_adjustment':-5.,
         'credit_default_book_adjustment':-2.,'accrued_interest':2.,'credit_default_accrued_interest':1.,
         'allowance':-3.,'equity_opening':-103.},
        {'accrued_interest':2.,'interest_income':-2.},
        {'credit_loss_expense':4.,'allowance':-4.},
        {'credit_default_principal':-5.,'allowance':5.},
        {'cash':3.,'credit_default_principal':-3.},
    ]
    for number,lines in enumerate(transactions,1):
        for gl,value in lines.items():
            instrument='claim-1' if gl.startswith('credit_default_') else 'loan-1' if gl not in {'cash','equity_opening'} else ''
            key=(gl,instrument);balances[key]=balances.get(key,0.)+value
            journal.append(dict(run_id='physical-base',scenario='baseline',severity=1.,account='bank',day=number-1,
                ordinal=739617+number-1,transaction_number=number,event=f'reviewed-{number}',gl_account=gl,instrument_id=instrument,value=value))
    trial=pl.DataFrame([dict(run_id='physical-base',scenario='baseline',severity=1.,account='bank',gl_account=gl,instrument_id=instrument,balance=value)
        for (gl,instrument),value in balances.items()])
    t=example();capital=deepcopy(t['capital'][0]);capital.update(entity='bank',cet1_deductions=0.)
    t.update(capital=[],curves=[],positions=[])
    weights={'cash':0.,'asset_principal':.5,'book_adjustment':.5,'accrued_interest':.5,'allowance':1.,
        'credit_default_principal':1.,'credit_default_book_adjustment':1.,'credit_default_accrued_interest':1.}
    policy=dict(account='bank',scenario='baseline',capital=capital,include_aoci=False,equity_exclusions=0.,intangible_assets=0.,
        leverage_addon=20.,leverage_deductions=0.,gl_risk_weights=weights,instrument_risk_weights={})
    return dict(kind='credit_ledger',treasury=t,policies=[policy],amount_multiplier=1.),trial,pl.DataFrame(journal)


def test_reviewed_credit_postings_capital_and_default_exposure_count_once():
    spec,trial,journal=credit_case();out=bridge(spec,trial_balance=trial,journal_frames=[journal.head(3),journal.slice(3)])
    row=out['capital_bridge'].row(0,named=True)
    assert row['assets']==101. and row['book_equity']==101.
    assert row['common_equity']==103. and row['retained_earnings']==-2.
    assert row['credit_rwa']==48.5
    assert out['prepared_specification']['capital'][0]['total_leverage_exposure']==121.
    defaults=out['rwa_contributions'].filter(pl.col('gl_account').str.starts_with('credit_default_'))
    assert set(defaults['instrument_id'])=={'claim-1'}
    assert defaults['rwa_contribution'].sum()==11.
    assert out['source_verification']['selected_runs']==[dict(run_id='physical-base',scenario='baseline',severity=1.)]


@pytest.mark.parametrize('failure',['balance','unbalanced','sequence','run','policy','unknown','legacy'])
def test_credit_capital_rejects_unverified_or_unmapped_inputs(failure):
    spec,trial,journal=credit_case()
    if failure=='balance':trial=trial.with_columns((pl.col('balance')+1.).alias('balance'))
    elif failure=='unbalanced':journal=journal.with_columns((pl.col('value')+1.).alias('value'))
    elif failure=='sequence':journal=journal.with_columns(pl.lit(99).alias('transaction_number'))
    elif failure=='run':journal=journal.with_columns(pl.when(pl.col('transaction_number')==2).then(pl.lit(2.)).otherwise(pl.col('severity')).alias('severity'))
    elif failure=='policy':spec['policies'][0]['gl_risk_weights'].pop('credit_default_principal')
    elif failure=='unknown':spec['economic_loss_deduction']=5.
    else:spec['kind']='ledger'
    with pytest.raises((ValueError,ArithmeticError)):bridge(spec,trial_balance=trial,journal_frames=[journal])
