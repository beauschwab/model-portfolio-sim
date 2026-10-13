"""Native capital from independently verified realized paths, not FTP or ECL."""
from copy import deepcopy
import json
from pathlib import Path
import polars as pl
import pytest
from portfolio_risk.analytics import credit_path_ledger
from portfolio_risk.analytics.balance_stream import PartitionWriter
from portfolio_risk.analytics.treasury import bridge
from portfolio_risk.analytics.credit_path_tables import SCHEMAS
from test_credit_treasury_bridge import credit_case


def path_case(kind):
    raw=json.loads((Path(__file__).resolve().parents[3]/'examples/models/credit-path-ledger-1.json').read_text())
    if kind=='signed':
        raw['specification']['positions'][0]['opening_accrued']=-2.
        raw['specification']['accounts'][0]['equity']=64.
        for run in raw['runs']:
            q=run['batches'][0]['request'];q['bindings'][0]['chargeoffs']=[]
            q['path']['paths'][0]['accrued_interest']=-2.
            q['path']['intervals'][0]['accrual_rates'][0]=-.365
            for tail in q['path']['intervals'][0]['complete_recovery_tails']:tail[0]['lag_years']=20.
    elif kind=='batches':
        second=deepcopy(raw['specification']['positions'][0])
        second.update(id='other-loan',balance=50.,commitment=0.,book_adjustment=0.,allowance=0.,opening_accrued=-3.,maturity_day=2)
        raw['specification']['positions'].append(second);raw['specification']['accounts'][0]['equity']=113.
        for run in raw['runs']:
            first=run['batches'][0];first['request']['bindings'][0]['chargeoffs'][0]['recognition_ordinal']=739893
            b=deepcopy(first);b['batch_id']='second';b['account_by_position'][0]['position_id']='other-loan'
            q=b['request'];q['bindings'][0].update(position_id='other-loan',claim_id='other-claim',opening_allowance=0.,chargeoffs=[])
            q['path']['paths'][0].update(path_id='other-loan',principal=50.,undrawn_commitment=0.,accrued_interest=-3.,book_adjustment=0.)
            q['path']['intervals'][0].update(hazards_per_year=[[0.]*5 for _ in range(3)],accrual_rates=[0.]*3)
            q['path']['boundaries']=[dict(event_id='maturity',policy_id='original-terms',year=2./365.,action=dict(kind='maturity'))]
            run['batches'].append(b)
    return raw


def source(tmp_path,kind='base'):
    path=credit_path_ledger.run_credit_path_ledger(path_case(kind),tmp_path,partition_rows=7)
    manifest=json.loads(path.read_text());writer=PartitionWriter(path.parent,7,schemas=credit_path_ledger.SCHEMAS)
    writer.parts={name:table['parts'] for name,table in manifest['tables'].items()}
    trial=pl.concat(list(writer.frames('trial_balance')))
    spec=credit_case()[0];spec['policies'][0]['scenario']='baseline'
    other=deepcopy(spec['policies'][0]);other['scenario']='synthetic-flat';other['capital']['scenario']='synthetic-flat'
    spec['policies'].append(other)
    return spec,trial,writer,manifest


@pytest.mark.parametrize('kind,assets,liabilities,equity',[('base',66.05,0.,66.05),('signed',66.,2.05,63.95),('batches',113.05,0.,113.05)])
def test_verified_paths_native_capital_hand_conservation(tmp_path,kind,assets,liabilities,equity):
    spec,trial,writer,manifest=source(tmp_path,kind)
    assert manifest['validation']['physical_paths_recomputed']
    out=bridge(spec,trial_balance=trial,journal_frames=writer.frames('journal'),credit_protocol=5)
    assert out['source_verification']['credit_protocol']==5
    for row in out['capital_bridge'].iter_rows(named=True):
        assert row['assets']==pytest.approx(assets,abs=1e-9)
        assert row['liabilities']==pytest.approx(liabilities,abs=1e-9)
        assert row['book_equity']==pytest.approx(equity,abs=1e-9)
    assert 'not an additional capital deduction' in out['credit_source_basis']
    if kind=='batches':
        journal=pl.concat(list(writer.frames('journal')))
        assert any(a>b for a,b in zip(journal['day'],journal['day'][1:]))


@pytest.mark.parametrize('failure',['balance','sequence','identity','scope','date','schema','marker'])
def test_path_bridge_rejects_corrupt_closing_or_journal(tmp_path,failure):
    spec,trial,writer,_=source(tmp_path)
    journal=pl.concat(list(writer.frames('journal')))
    if failure=='balance':trial=trial.with_columns((pl.col('balance')+1.).alias('balance'))
    elif failure=='sequence':journal=journal.with_columns(pl.lit(9).alias('transaction_number'))
    elif failure=='identity':journal=journal.with_columns(pl.lit('["opening","bad","bank"]').alias('transaction_key'))
    elif failure=='scope':trial=trial.with_columns(pl.lit('changed').alias('currency'))
    elif failure=='date':journal=journal.with_columns(pl.when(pl.col('transaction_number')==3).then(pl.lit(-1)).otherwise(pl.col('day')).alias('day'))
    elif failure=='schema':journal=journal.drop('transaction_key')
    else:spec['credit_protocol']=5
    with pytest.raises((ValueError,ArithmeticError)):
        bridge(spec,trial_balance=trial,journal_frames=[journal.head(3),journal.slice(3)],credit_protocol=5)


def test_protocol_five_never_silently_uses_protocol_four_replay(tmp_path):
    spec,trial,writer,_=source(tmp_path,'batches')
    with pytest.raises(ValueError,match='schema'):
        bridge(spec,trial_balance=trial,journal_frames=writer.frames('journal'))


def test_signed_live_and_default_payables_are_gross_and_basis_stays_contra_asset():
    spec,_,original=credit_case();rows=original.to_dicts();balances={}
    for row in rows:
        number=row['transaction_number'];gl=row['gl_account']
        if number==1:
            if gl=='accrued_interest':row['value']=-2.
            elif gl=='credit_default_accrued_interest':row['value']=-3.
            elif gl=='equity_opening':row['value']=-95.
        elif number==2:row['value']=1. if gl=='accrued_interest' else -1.
        row.update(entity='bank',currency='USD',transaction_key=json.dumps(
            ['opening','physical-base','bank'] if number==1 else ['path','physical-base','reviewed','loan-1',number]))
        key=(gl,row['instrument_id']);balances[key]=balances.get(key,0.)+row['value']
    journal=pl.DataFrame(rows,schema=SCHEMAS['journal'])
    trial=pl.DataFrame([dict(run_id='physical-base',scenario='baseline',severity=1.,account='bank',entity='bank',currency='USD',
        gl_account=gl,instrument_id=instrument,balance=value) for (gl,instrument),value in balances.items()],schema=SCHEMAS['trial_balance'])
    out=bridge(spec,trial_balance=trial,journal_frames=[journal.head(5),journal.slice(5)],credit_protocol=5)
    row=out['capital_bridge'].row(0,named=True)
    assert row['assets']==96. and row['liabilities']==4. and row['book_equity']==92.
    assert row['common_equity']==95. and row['retained_earnings']==-3.
    assert not out['rwa_contributions'].filter(pl.col('gl_account').str.contains('accrued_interest')).height
    assert out['rwa_contributions'].filter(pl.col('gl_account').str.contains('book_adjustment'))['balance'].sum()==-7.
