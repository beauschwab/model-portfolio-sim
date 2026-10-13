"""Transport for native capital coverage and funds-transfer-pricing reports.

Explicit eligible amounts, curve vintages and requirement policies are inputs.
The synthetic example is not a regulatory rulebook or a saved-book projection.
"""
from copy import deepcopy
import hashlib
import json
import polars as pl
from ..core.quant_native import term_call

CAPITAL_METRICS = ('cet1', 'tier1', 'total_capital', 'advanced_cet1', 'advanced_tier1',
    'advanced_total_capital', 'tier1_leverage', 'slr', 'tlac_rwa', 'tlac_leverage',
    'ltd_rwa', 'ltd_leverage', 'tce_ta')

_COMMON = {k:pl.String for k in ('entity','currency','scenario','period')}
_SCHEMAS = {
    'capital_metrics': _COMMON | {'metric':pl.String,'status':pl.String} | {k:pl.Float64 for k in
        ('numerator','denominator','ratio','minimum','buffer','required_ratio','management_target',
         'headroom','management_headroom','headroom_bp')},
    'ftp_positions': _COMMON | {k:pl.String for k in ('id','loan_id','cohort_id','curve_id')} |
        {'flat_tail_used':pl.Boolean} | {k:pl.Float64 for k in ('reference_rate','liquidity_spread',
        'reference_charge','liquidity_charge','option_charge','contingent_liquidity_charge','ftp_charge',
        'external_profit','business_profit','capital_cost','economic_profit','raroc')},
    'ftp_reconciliation': _COMMON | {k:pl.Float64 for k in ('external_profit','business_profit',
        'treasury_profit','consolidated_profit','capital_cost','elimination_error')},
}
_SCHEMAS.update({
    'capital_bridge': {k:pl.String for k in ('scenario','account','entity','currency')} |
        {k:pl.Float64 for k in ('assets','liabilities','book_equity','common_equity','retained_earnings',
         'book_aoci','eligible_aoci','equity_exclusions','credit_rwa','reconciliation_error')},
    'rwa_contributions': {k:pl.String for k in ('scenario','account','gl_account','instrument_id')} |
        {k:pl.Float64 for k in ('balance','risk_weight','rwa_contribution')},
    'ftp_preparation': {k:pl.String for k in ('book','id','entity','currency','scenario','period')} |
        {'month':pl.Int64} | {k:pl.Float64 for k in ('opening_balance','principal','closing_balance',
        'average_balance','funding_years','residual_tail_years','tail_principal','tail_share','external_interest')},
})


def _replay_path_journal(frames):
    """Independent protocol-5 replay in producer order, without global date sorting.

    Batches restart their calendars. Only each path's intent order is chronological;
    transaction numbers are global and contiguous across retained partitions.
    """
    import math
    balances={};runs={};labels={};paths={};openings=set();clocks={}
    current=None;sequence=0;values=[];count=0
    def balanced():
        if abs(math.fsum(values))>1e-9*max(1.,math.fsum(abs(v) for v in values)):
            raise ArithmeticError('unbalanced credit path journal transaction')
    for frame in frames:
        for row in frame.iter_rows(named=True):
            run=row['run_id'];scope=(row['scenario'],row['severity'])
            label=(row['entity'],row['currency'])
            if (not math.isfinite(row['severity']) or row['severity']<=0. or row['day']<0 or
                    row['ordinal']<=0 or any(not row[k] for k in ('run_id','scenario','account','entity','currency'))):
                raise ValueError('invalid credit path journal scope or clock')
            offset=row['ordinal']-row['day']
            if run in clocks and clocks[run]!=offset:
                raise ValueError('credit path journal calendar mismatch')
            clocks[run]=offset
            if (run in runs and runs[run]!=scope or
                    (run,row['account']) in labels and labels[run,row['account']]!=label):
                raise ValueError('credit path journal scope changed')
            runs[run]=scope;labels[run,row['account']]=label
            transaction=tuple(row[k] for k in ('transaction_number','transaction_key','run_id','scenario',
                'severity','account','entity','currency','day','ordinal','event'))
            if transaction!=current:
                if current is not None:balanced()
                sequence+=1
                if row['transaction_number']!=sequence:
                    raise ValueError('credit path journal transaction sequence mismatch')
                identity=json.loads(row['transaction_key'])
                if not isinstance(identity,list) or len(identity) not in (3,5) or identity[1]!=run:
                    raise ValueError('invalid credit path transaction identity')
                if identity[0]=='opening' and len(identity)==3:
                    if identity[2]!=row['account'] or row['day']!=0 or (run,row['account']) in openings:
                        raise ValueError('duplicate or misplaced credit path opening')
                    openings.add((run,row['account']))
                elif identity[0]=='path' and len(identity)==5:
                    if ((run,row['account']) not in openings or not isinstance(identity[2],str) or
                            not isinstance(identity[3],str) or type(identity[4]) is not int or identity[4]<0):
                        raise ValueError('invalid credit path intent identity')
                    key=tuple(identity[:4]);previous=paths.get(key)
                    if previous and (identity[4]<=previous[0] or row['day']<previous[1] or
                            row['ordinal']<previous[2] or row['account']!=previous[3]):
                        raise ValueError('credit path intent order mismatch')
                    paths[key]=(identity[4],row['day'],row['ordinal'],row['account'])
                else:raise ValueError('unknown credit path transaction identity')
                if len(paths)>250000 or len(labels)>10000:
                    raise ValueError('credit path replay exceeds scope admission')
                current=transaction;values=[]
            value=row['value']
            if not math.isfinite(value):raise ValueError('invalid credit path journal amount')
            key=tuple(row[k] for k in ('run_id','scenario','severity','account','gl_account','instrument_id'))
            balances[key]=balances.get(key,0.)+value
            if len(balances)>250000 or len(values)>=250000:
                raise ValueError('credit path replay exceeds GL admission')
            if not math.isfinite(balances[key]):raise ArithmeticError('credit path journal overflow')
            values.append(value);count+=1
    if current is not None:balanced()
    return balances,count,labels


def _replay_signed_dated_journal(frames):
    """Independent replay of native protocol-3 signed postings in source order."""
    import math
    balances={};sequences={};last_day={};current=None;amounts=[];count=0
    required={'transaction_number','scenario','day','account','event','gl_account','instrument_id','value'}
    def balanced():
        if abs(math.fsum(amounts))>1e-9*max(1.,math.fsum(abs(v) for v in amounts)):
            raise ArithmeticError('unbalanced mixed dated journal transaction')
    for frame in frames:
        if set(frame.columns)!=required:raise ValueError('mixed dated journal schema mismatch')
        for row in frame.iter_rows(named=True):
            scenario=row['scenario'];day=row['day'];tx=row['transaction_number'];value=row['value']
            if (type(tx) is not int or tx<=0 or type(day) is not int or day<0 or
                    any(not isinstance(row[k],str) or not row[k] for k in ('scenario','account','event','gl_account')) or
                    not isinstance(row['instrument_id'],str) or isinstance(value,bool) or
                    not isinstance(value,(int,float)) or not math.isfinite(value)):
                raise ValueError('invalid mixed dated posting identity or amount')
            key=(scenario,tx,day,row['account'],row['event'])
            if key!=current:
                if current is not None:balanced()
                sequences[scenario]=sequences.get(scenario,0)+1
                if tx!=sequences[scenario] or day<last_day.get(scenario,0):
                    raise ValueError('mixed dated journal transaction sequence mismatch')
                last_day[scenario]=day;current=key;amounts=[]
            scope=(scenario,row['account'],row['gl_account'],row['instrument_id'])
            balances[scope]=balances.get(scope,0.)+value
            if not math.isfinite(balances[scope]):raise ArithmeticError('mixed dated GL overflow')
            if len(balances)>250000 or len(amounts)>=250000:
                raise ValueError('mixed dated source exceeds bridge admission')
            amounts.append(value);count+=1
    if current is not None:balanced()
    return balances,count


def bridge(specification: dict, *, trial_balance=None, journal_frames=None, openings=None, cashflows=None,
           credit_protocol=4,ledger_journal_protocol=2) -> dict:
    """Verify source ledger, then transport source tables to the native preparer.

    Journal replay is an independent publication check. It is not a Python
    financial calculation callback. Captured cashflows must be base/accounting
    outputs; conditional published forecasts are not admitted by the API route.
    """
    request=deepcopy(specification)
    if 'credit_protocol' in request:
        raise ValueError('credit protocol is selected from verified source metadata')
    if 'ledger_journal_protocol' in request:
        raise ValueError('ledger journal protocol is selected from verified source metadata')
    if ledger_journal_protocol not in (2,3) or ledger_journal_protocol==3 and request['kind']!='ledger':
        raise ValueError('unsupported ledger journal source protocol')
    if credit_protocol not in (4,5) or credit_protocol==5 and request['kind']!='credit_ledger':
        raise ValueError('unsupported treasury credit source protocol')
    if request['kind'] in {'ledger','credit_ledger'}:
        import math
        from .balance_stream import replay_partitions
        if trial_balance is None or journal_frames is None: raise ValueError('ledger bridge requires journal and trial balance')
        if trial_balance.height>250000: raise ValueError('ledger bridge exceeds 250000 closing GL keys')
        credit=request['kind']=='credit_ledger'
        source_frames=journal_frames
        def checked_frames():
            required={'run_id','scenario','severity','account','day','ordinal','transaction_number','event','gl_account','instrument_id','value'} if credit else {'transaction_id','scenario','day','account','event','gl_account','instrument_id','debit','credit'}
            if credit and credit_protocol==5:
                from .credit_path_tables import SCHEMAS
                required=set(SCHEMAS['journal'])
            elif not credit and ledger_journal_protocol==3:
                required={'transaction_number','scenario','day','account','event','gl_account','instrument_id','value'}
            for frame in source_frames:
                if set(frame.columns)!=required:raise ValueError('treasury source journal schema mismatch')
                if credit and credit_protocol==5 and dict(frame.schema)!=SCHEMAS['journal']:
                    raise ValueError('treasury path journal type mismatch')
                yield frame
        if credit and credit_protocol==5:
            from .credit_path_tables import SCHEMAS
            if dict(trial_balance.schema)!=SCHEMAS['trial_balance']:
                raise ValueError('treasury path trial schema mismatch')
            replayed,count,labels=_replay_path_journal(checked_frames())
        elif credit:
            from .credit_verification import replay_credit_journal
            replayed,count=replay_credit_journal(checked_frames())
        elif ledger_journal_protocol==3:replayed,count=_replay_signed_dated_journal(checked_frames())
        else:replayed,count=replay_partitions(checked_frames())
        if len(replayed)!=trial_balance.height: raise ValueError('source journal/trial key mismatch')
        seen=set()
        for r in trial_balance.iter_rows(named=True):
            fields=('run_id','scenario','severity','account','gl_account','instrument_id') if credit else ('scenario','account','gl_account','instrument_id')
            key=tuple(r[k] for k in fields)
            if credit and credit_protocol==5 and labels.get((r['run_id'],r['account']))!=(r['entity'],r['currency']):
                raise ValueError('credit path trial scope mismatch')
            if key in seen or key not in replayed or not math.isclose(replayed[key],r['balance'],rel_tol=1e-12,abs_tol=1e-9):
                raise ValueError('source journal/trial balance mismatch')
            seen.add(key)
        # Reverse-severity runs remain part of journal verification. Capital reports
        # select the explicitly admitted severity-one baseline/scenario cases.
        selected=trial_balance.filter(pl.col('severity')==1.) if credit else trial_balance
        if credit and credit_protocol==5:request['credit_protocol']=5
        if credit and selected.select('scenario','run_id').unique()['scenario'].n_unique()!=selected['run_id'].n_unique():
            raise ValueError('credit capital scenarios require one severity-one run each')
        request['trial_balance']=selected.select('scenario','account','gl_account','instrument_id','balance').to_dicts()
    elif request['kind']=='cashflows':
        if openings is None or cashflows is None: raise ValueError('FTP bridge requires captured openings and cashflows')
        if cashflows.height>60000: raise ValueError('FTP bridge exceeds 60000 position-month rows')
        request['openings']=openings.to_dicts();request['cashflows']=cashflows.to_dicts()
    else: raise ValueError('unknown treasury bridge kind')
    result=term_call('treasury-bridge-1',request)
    for name,schema in _SCHEMAS.items():
        if name in result: result[name]=pl.DataFrame(result[name],schema=schema,infer_schema_length=None)
    payload=json.dumps(request,sort_keys=True,separators=(',',':'),allow_nan=False)
    result.update(input_sha256=hashlib.sha256(payload.encode()).hexdigest(),
        bridge_specification=deepcopy(specification),backend='rust')
    if request['kind'] in {'ledger','credit_ledger'}:
        result['source_verification']=dict(journal_replayed=True,journal_rows=count)
        if request['kind']=='ledger' and ledger_journal_protocol==3:
            result['source_verification']['ledger_journal_protocol']=3
        if request['kind']=='credit_ledger':
            result['source_verification']['credit_protocol']=credit_protocol
            result['source_verification']['selected_runs']=selected.select('run_id','scenario','severity').unique().sort('scenario').to_dicts()
    return result


def source_template(kind, rows, *, asof=None, horizon=None, basis=None):
    """Editable scaffolding; nulls mark inputs which cannot safely be inferred."""
    treasury=dict(as_of=asof or 'SET_REPORT_DATE',policy_id='SET_POLICY_VERSION',capital=[],curves=[],positions=[])
    if kind in {'ledger','credit_ledger'}:
        capital=example()['capital'][0]
        capital.update(common_equity=0.,retained_earnings=0.,eligible_aoci=0.,credit_rwa=0.,
            total_leverage_exposure=0.,tangible_common_equity=0.,tangible_assets=0.,requirements={},
            cet1_deductions=None,additional_tier1=None,tier2=None,eligible_tlac=None,eligible_ltd=None,
            market_rwa=None,operational_rwa=None,advanced_rwa=None,average_assets=None,period='SET_PERIOD')
        return dict(kind=kind,treasury=treasury,amount_multiplier=None,policies=[dict(account=r['account'],scenario=r['scenario'],
            capital=capital|dict(entity=r['account'],currency=r['currency'],scenario=r['scenario']),include_aoci=None,equity_exclusions=None,
            intangible_assets=None,leverage_addon=None,leverage_deductions=None,
            gl_risk_weights={},instrument_risk_weights={}) for r in rows])
    if kind!='cashflows':raise ValueError('unknown treasury bridge kind')
    mappings=[]
    for row in rows:
        ident=row['id'];source_id=ident.removeprefix('HL-') if row['book']=='mbs' else ident
        mappings.append(dict(book=row['book'],id=ident,entity='SET_ENTITY',currency='USD',scenario='base',
            curve_id='SET_CURVE_ID',loan_id=source_id if basis=='individual_model_reprice' else None,
            cohort_id=source_id if basis=='representative_cohort_reprice' else None,
            repricing_years=None,residual_tail_years=None,operating_cost_rate=0.,expected_loss_rate=0.,
            capital_ratio=0.,cost_of_capital=0.,option_spread=0.,annual_fee_rate=0.,contingent_charge_rate=0.))
    return dict(kind=kind,treasury=treasury,horizon=horizon,mappings=mappings)


def validate_capital_limits(limits, units, scenarios, horizon):
    """Independent reference validation of externally prepared capital rows."""
    import math
    if len(limits)>2048: raise ValueError('too many capital limits')
    labels=set()
    fields={'label','policy_id','metric','scenario','month','units','numerator','denominator',
            'required_ratio','numerator_per_unit','denominator_per_unit'}
    for r in limits:
        if set(r)!=fields: raise ValueError('invalid capital limit fields')
        if (not isinstance(r['label'],str) or not 1<=len(r['label'])<=200 or r['label'] in labels
                or not isinstance(r['policy_id'],str) or not 1<=len(r['policy_id'])<=200
                or r['metric'] not in CAPITAL_METRICS or r['units']!=units
                or type(r['scenario']) is not int or not 0<=r['scenario']<scenarios
                or type(r['month']) is not int or not 0<=r['month']<horizon):
            raise ValueError('invalid capital limit or stale unit grid')
        labels.add(r['label'])
        vectors=[r['numerator_per_unit'],r['denominator_per_unit']]
        if any(len(v)!=len(units) for v in vectors): raise ValueError('capital vector shape')
        if (any(not math.isfinite(v) for v in [r['numerator'],r['denominator'],r['required_ratio'],*vectors[0],*vectors[1]])
                or r['denominator']<=0 or not 0<=r['required_ratio']<=1
                or any(v<0 for v in vectors[1])):
            raise ValueError('invalid capital amounts')


def replay_capital_limits(limits, units, allocation):
    import math
    amounts={(r['template'],r['purchase_m']):r['notional'] for r in allocation}
    x=[amounts.get((u['template'],u['h']),0.) for u in units]
    rows=[]
    for r in limits:
        n=r['numerator']+sum(a*b for a,b in zip(x,r['numerator_per_unit']))
        d=r['denominator']+sum(a*b for a,b in zip(x,r['denominator_per_unit']))
        need=d*r['required_ratio']
        if not math.isfinite(n) or not math.isfinite(d) or d<=0 or n<need-max(.01,abs(need)*1e-8):
            raise RuntimeError('independent capital limit replay failed')
        rows.append({k:r[k] for k in ('label','policy_id','metric','scenario','month','required_ratio')} |
                    dict(numerator=n,denominator=d,ratio=n/d,headroom=n-need))
    return rows


def evaluate(specification: dict) -> dict:
    payload = json.dumps(specification, sort_keys=True, separators=(',', ':'), allow_nan=False)
    result = term_call('treasury-1', specification)
    result['input_sha256'] = hashlib.sha256(payload.encode()).hexdigest()
    result['specification'] = deepcopy(specification)
    for name in ('capital_metrics', 'ftp_positions', 'ftp_reconciliation'):
        result[name] = pl.DataFrame(result[name], schema=_SCHEMAS[name], infer_schema_length=None)
    result['backend'] = 'rust'
    return result


def example() -> dict:
    """Synthetic USD entity; all money in dollars, all rates in decimals.

    Requirements deliberately belong to the example, not global defaults.
    Capital snapshots are eligible inputs with retained earnings excluded from
    common_equity to avoid double counting. TLAC is an eligible total.
    """
    floors = dict(cet1=.045, tier1=.06, total_capital=.08,
                  advanced_cet1=.045, advanced_tier1=.06, advanced_total_capital=.08,
                  tier1_leverage=.04, slr=.03, tlac_rwa=.18,
                  tlac_leverage=.075, ltd_rwa=.06, ltd_leverage=.045, tce_ta=.05)
    capital = dict(entity='demo-bank', currency='USD', scenario='base', period='2026-10',
        common_equity=100_000_000., retained_earnings=10_000_000., eligible_aoci=-2_000_000.,
        cet1_deductions=8_000_000., additional_tier1=20_000_000., tier2=15_000_000.,
        eligible_tlac=220_000_000., eligible_ltd=80_000_000., credit_rwa=750_000_000.,
        market_rwa=50_000_000., operational_rwa=100_000_000., advanced_rwa=950_000_000.,
        average_assets=1_500_000_000., total_leverage_exposure=2_000_000_000.,
        tangible_common_equity=100_000_000., tangible_assets=1_480_000_000.,
        requirements={k:dict(minimum=v,buffer=.025 if k in ('cet1','tier1','total_capital') else 0.,
                            management=.005) for k,v in floors.items()})
    curve = dict(id='demo-usd-20261001', entity='demo-bank', currency='USD', as_of='2026-10-01',
                 tenors=[0.,1.,5.,10.], reference_rates=[.04,.039,.037,.038],
                 liquidity_spreads=[.001,.002,.005,.007])
    loan = dict(id='loan-1',loan_id='loan-1',cohort_id='mortgage-prime',entity='demo-bank',
        currency='USD',scenario='base',period='2026-10',curve_id=curve['id'],side='asset',
        average_balance=1_000_000.,accrual_fraction=1/12,repricing_years=5.,funding_years=7.,
        external_interest=5_000.,fees=100.,operating_cost=150.,expected_loss=75.,
        allocated_capital=100_000.,cost_of_capital=.12,option_spread=.001,
        contingent_liquidity_charge=10.)
    deposit = loan | dict(id='deposit-1',loan_id=None,cohort_id='stable-savings',side='liability',
        repricing_years=.25,funding_years=3.,external_interest=-1500.,fees=50.,operating_cost=200.,
        expected_loss=0.,allocated_capital=10_000.,option_spread=0.,contingent_liquidity_charge=20.)
    stress = deepcopy(capital)
    stress.update(scenario='adverse',retained_earnings=-20_000_000.,eligible_aoci=-12_000_000.,
                  credit_rwa=850_000_000.,advanced_rwa=1_100_000_000.)
    return dict(as_of=curve['as_of'],policy_id='synthetic-example-not-regulatory-advice-v1',
                capital=[capital,stress],curves=[curve],positions=[loan,deposit])
