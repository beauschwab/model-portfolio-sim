"""Protocol 5 transport schemas and canonical JSON cell columns."""
import polars as pl

MODEL_VERSION='credit-path-ledger-1'
SCOPE=dict.fromkeys(('run_id','scenario','account','entity','currency'),pl.String)|dict(severity=pl.Float64)
DATED=dict(SCOPE,day=pl.Int64,ordinal=pl.Int64)
POSITION=dict(SCOPE,position=pl.String,path_key=pl.UInt64)
SOURCE=dict.fromkeys(('source_id','source_revision'),pl.String)
SCHEMAS={
    'journal':dict(DATED,transaction_number=pl.Int64,transaction_key=pl.String,event=pl.String,gl_account=pl.String,instrument_id=pl.String,value=pl.Float64),
    'credit_path_sources':dict(POSITION,**SOURCE,batch_id=pl.String,claim_id=pl.String,measure=pl.String,monetary_unit=pl.String,binding=pl.String,economic=pl.String),
    'credit_path_lineage':dict(POSITION,**SOURCE,batch_id=pl.String,day=pl.Int64,ordinal=pl.Int64,transaction_number=pl.Int64,transaction_key=pl.String,
        event=pl.String,claim_id=pl.String,policy_id=pl.String,policy_revision=pl.String,exact_event=pl.String,accrual=pl.String,event_ids=pl.String,contractual_policy_id=pl.String,timing=pl.String),
    'credit_path_daily':dict(POSITION,batch_id=pl.String,claim_id=pl.String,day=pl.Int64,ordinal=pl.Int64,state=pl.String),
    'credit_path_recoveries':dict(POSITION,**SOURCE,batch_id=pl.String,claim_id=pl.String,recovery=pl.String),
    'credit_path_claims':dict(POSITION,ordinal=pl.Int64,claim=pl.String),
    'daily_balance':dict(DATED,**dict.fromkeys(('cash','assets','liabilities','net_assets','equity','allowance','reconciliation_error'),pl.Float64)),
    'trial_balance':dict(SCOPE,gl_account=pl.String,instrument_id=pl.String,balance=pl.Float64),
    'summary':dict(SCOPE,**dict.fromkeys(('final_cash','final_assets','final_liabilities','final_net_assets','final_equity','max_reconciliation_error'),pl.Float64)),
}
JSON_FIELDS={
    'credit_path_sources':('binding','economic'),
    'credit_path_lineage':('exact_event','accrual','event_ids'),
    'credit_path_daily':('state',),'credit_path_recoveries':('recovery',),'credit_path_claims':('claim',),
}

