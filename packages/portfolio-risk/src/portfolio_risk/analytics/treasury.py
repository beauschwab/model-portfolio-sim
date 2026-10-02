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


def bridge(specification: dict, *, trial_balance=None, journal_frames=None, openings=None, cashflows=None) -> dict:
    """Verify source ledger, then transport source tables to the native preparer.

    Journal replay is an independent publication check. It is not a Python
    financial calculation callback. Captured cashflows must be base/accounting
    outputs; conditional published forecasts are not admitted by the API route.
    """
    request=deepcopy(specification)
    if request['kind']=='ledger':
        import math
        from .balance_stream import replay_partitions
        if trial_balance is None or journal_frames is None: raise ValueError('ledger bridge requires journal and trial balance')
        if trial_balance.height>250000: raise ValueError('ledger bridge exceeds 250000 closing GL keys')
        replayed,count=replay_partitions(journal_frames)
        if len(replayed)!=trial_balance.height: raise ValueError('source journal/trial key mismatch')
        seen=set()
        for r in trial_balance.iter_rows(named=True):
            key=tuple(r[k] for k in ('scenario','account','gl_account','instrument_id'))
            if key in seen or key not in replayed or not math.isclose(replayed[key],r['balance'],rel_tol=1e-12,abs_tol=1e-9):
                raise ValueError('source journal/trial balance mismatch')
            seen.add(key)
        request['trial_balance']=trial_balance.to_dicts()
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
    if request['kind']=='ledger':result['source_verification']=dict(journal_replayed=True,journal_rows=count)
    return result


def source_template(kind, rows, *, asof=None, horizon=None, basis=None):
    """Editable scaffolding; nulls mark inputs which cannot safely be inferred."""
    treasury=dict(as_of=asof or 'SET_REPORT_DATE',policy_id='SET_POLICY_VERSION',capital=[],curves=[],positions=[])
    if kind=='ledger':
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
