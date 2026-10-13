"""Independent pooled cash/account reporting for reviewed commercial owners."""
from collections import defaultdict
import math
from .commercial_source_reference import compare


def normalize_specification(raw):
    """Independently extend the frozen opening schema with current policy inputs."""
    from copy import deepcopy
    from ._deposit_continuation_calendar import _normalize
    legacy = deepcopy(raw)
    defaults = dict(lcr_applicability='', nsfr_applicability='',
                    htm_accounting='', htm_taint_materiality=0.)
    policies = [{key: account.pop(key, value) for key, value in defaults.items()}
                for account in legacy.get('accounts', [])]
    normalized = _normalize(legacy)
    for account, policy in zip(normalized['accounts'], policies, strict=True):
        for metric in ('lcr', 'nsfr'):
            selection = policy[metric + '_applicability']
            floor = account[metric + '_floor']
            if type(selection) is not str:
                raise ArithmeticError('commercial liquidity applicability must be a string')
            basis = selection or ('internal_limit' if floor > 0 else 'unconfigured')
            if basis not in ('internal_limit', 'not_applicable', 'unconfigured') or ((basis == 'internal_limit') != (floor > 0)):
                raise ArithmeticError('commercial liquidity applicability/floor differs')
        materiality = policy['htm_taint_materiality']
        if (policy['htm_accounting'] not in ('', 'asc320-htm-v1')
                or type(materiality) not in (int, float) or not math.isfinite(materiality)
                or not 0 <= materiality <= 1 or (materiality > 0 and not policy['htm_accounting'])):
            raise ArithmeticError('commercial HTM policy domain differs')
        policy['htm_taint_materiality'] = float(materiality)
        account.update(policy)
    return normalized


def liquidity_policy(account, metric):
    basis = getattr(account, metric + '_applicability') or (
        'internal_limit' if getattr(account, metric + '_floor') > 0 else 'unconfigured')
    # These exclusively owned commercial sources have no legacy liquidity
    # denominator. A configured limit is unavailable, never a reported pass.
    return basis, 'unavailable' if basis == 'internal_limit' else basis


def scope(spec,owned):
    if {p.id for p in spec['positions']}!=set(owned) or spec['policies'] or spec['netting_sets'] or spec['cashflows'] or spec['reverse_severities']:
        raise ArithmeticError('commercial publication requires explicit exclusively owned commercial sources; legacy mixed overlays require independent adoption')
    for account in spec['accounts']:
        if any(getattr(account,k)!=0. for k in ('annual_fees','annual_costs','annual_dividends','tax_rate')):
            raise ArithmeticError('commercial account operating overlays require independent adoption')
        if any(getattr(account, k) != 0. for k in ('cet1_floor', 'leverage_floor', 'lcr_floor', 'nsfr_floor')) or account.htm_asset_limit != 1.:
            raise ArithmeticError('commercial capital/liquidity limits require independent adoption')


def reports(prepared,events,check=lambda:False):
    """Every scalar/report comes from raw source postings, not producer output."""
    spec=prepared['spec'];horizon=prepared['horizon'];positions={p.id:p for p in spec['positions']};scenarios=['baseline']+[s.name for s in spec['scenarios']]
    wanted={name:[] for name in ('journal','path','summary','ledger','actions','breaches','exposures','funding_claims','event_lineage','dated_rights','reverse_grid','trial_balance','closing_statements','consolidated','attribution')}
    total_events=defaultdict(lambda:[0.,0.,0.]);closing_by_scenario={}
    for scenario in scenarios:
        check();sequence=0;gl=defaultdict(float);money={a.id:[float(a.cash),float(a.equity)] for a in spec['accounts']};minimum={a.id:float(a.cash) for a in spec['accounts']};first={};maximum_error=0.;day_groups=defaultdict(list)
        for e in events:
            if e[0]==scenario:day_groups[e[1]].append(e)
        def post(day,account,event,position,lines):
            nonlocal sequence
            lines=[(name,value) for name,value in lines if value!=0.]
            if not lines:return
            sequence+=1
            for name,value in lines:
                instrument='' if name in ('cash','opening_equity','oci') else position;gl[account,name,instrument]+=value
                wanted['journal'].append(dict(transaction_id=f'{scenario}:{sequence}',scenario=scenario,day=day,account=account,event=event,gl_account=name,instrument_id=instrument,debit=max(value,0.),credit=max(-value,0.)))
        for a in spec['accounts']:post(0,a.id,'opening','',[('cash',float(a.cash)),('opening_equity',-float(a.equity))])
        for day in range(horizon+1):
            check();aggregate={}
            for e in day_groups[day]:
                want,lines=e[5],e[6];account=positions[want['position_id']].account;group=want['native_atomic_group'];post(day,account,group['phase'],want['position_id'],lines)
                cash=sum(v for k,v in lines if k=='cash');equity=-sum(v for k,v in lines if k.startswith('pnl:'));money[account][0]+=cash;money[account][1]+=equity
                key=(account,group['phase']);values=aggregate.setdefault(key,[0.,0.,0.]);values[0]+=cash;values[1]+=equity
            for a in spec['accounts']:
                cash,equity=money[a.id];assets=cash;liabilities=0.
                for (account,name,_),value in gl.items():
                    if account!=a.id or name in ('cash','opening_equity','oci') or name.startswith('pnl:') or name.startswith('memo:'):continue
                    if name in ('dated_deferred_income','credit_default_deferred_income','commercial_fee_payable','commercial_deferred_fee_income'):liabilities-=value
                    else:assets+=value
                error=assets-liabilities-equity;maximum_error=max(maximum_error,abs(error));minimum[a.id]=min(minimum[a.id],cash);cet1=equity-a.capital_deductions
                margins=dict(cash=cash-a.cash_floor,cet1=cet1,leverage=cet1,lcr=max(cash,0.),nsfr=max(equity,0.),htm=max(assets,0.))
                for metric,value in margins.items():
                    if metric in ('lcr', 'nsfr') and liquidity_policy(a, metric)[0] != 'internal_limit':continue
                    if value < -1e-9 and (a.id,metric) not in first:first[a.id,metric]=day;wanted['breaches'].append(dict(scenario=scenario,account=a.id,day=day,metric=metric))
                if day<=30 or day%30==0 or day==horizon:
                    row=dict(scenario=scenario,account=a.id,entity=a.entity,currency=a.currency,day=day,cash=cash,restricted_cash=0.,cash_floor=float(a.cash_floor),assets=assets,liabilities=liabilities,equity=equity,aoci=0.,cet1=cet1,rwa=0.,cet1_ratio=None,leverage_ratio=cet1/assets if assets>0. else None,usable_collateral=0.,undrawn=0.,hqla_proxy=max(cash,0.),lcr_proxy=None,nsfr_proxy=None,reconciliation_error=error)
                    row.update({k+'_headroom':float(v) for k,v in margins.items()})
                    for metric in ('lcr', 'nsfr'):
                        basis, status = liquidity_policy(a, metric)
                        row[metric + '_status'] = status
                        if basis != 'internal_limit':row[metric + '_headroom'] = None
                    wanted['path'].append(row)
            if day==0 or day%30==0 or day==horizon:
                for p in spec['positions']:wanted['exposures'].append(dict(scenario=scenario,day=day,account=p.account,position=p.id,kind=p.kind,balance=0.,allowance=0.,watch_fraction=float(p.watch_fraction),commitment=0.,market_factor=float(p.opening_market_price),encumbered_fraction=0.,accrued_interest=0.))
            for (account,event),value in aggregate.items():
                if any(v!=0. for v in value):
                    totals=total_events[scenario,account,event]
                    for i in range(3):totals[i]+=value[i]
                    wanted['ledger'].append(dict(scenario=scenario,day=day,account=account,event=event,cash=value[0],earnings=value[1],aoci=0.,memo_amount=0.))
        statements=[]
        for a in spec['accounts']:
            cash,equity=money[a.id];wanted['summary'].append(dict(scenario=scenario,account=a.id,entity=a.entity,currency=a.currency,final_cash=cash,final_equity=equity,minimum_cash=minimum[a.id],peak_cash_shortfall=max(a.cash_floor-minimum[a.id],0.),max_reconciliation_error=maximum_error,
                **{field:first.get((a.id,metric)) for field,metric in (('first_cash_breach_day','cash'),('first_capital_breach_day','cet1'),('first_leverage_breach_day','leverage'),('first_lcr_breach_day','lcr'),('first_nsfr_breach_day','nsfr'),('first_htm_breach_day','htm'))}))
            wanted['summary'][-1]['htm_reclassification_day'] = None
            for metric in ('lcr', 'nsfr'):
                basis, status = liquidity_policy(a, metric)
                wanted['summary'][-1][metric + '_applicability'] = basis
                wanted['summary'][-1][metric + '_status'] = status
            assets=liabilities=equity=0.
            for (account,name,_),value in sorted(gl.items()):
                if account!=a.id:continue
                if name in ('opening_equity','oci') or name.startswith('pnl:'):equity-=value;continue
                if name in ('dated_deferred_income','credit_default_deferred_income','commercial_fee_payable','commercial_deferred_fee_income'):liabilities-=value
                else:assets+=value
            statements.append(dict(scenario=scenario,account=a.id,currency=a.currency,assets=assets,liabilities=liabilities,equity=equity,cash=cash,restricted_cash=0.,intercompany_assets=0.,intercompany_liabilities=0.,reconciliation_error=assets-liabilities-equity))
        wanted['closing_statements'].extend(statements);closing_by_scenario[scenario]=statements
        bycurrency=defaultdict(lambda:dict(assets=0.,liabilities=0.,equity=0.,cash=0.,restricted_cash=0.))
        for row in statements:
            for k in bycurrency[row['currency']]:bycurrency[row['currency']][k]+=row[k]
        for currency,row in bycurrency.items():wanted['consolidated'].append(dict(scenario=scenario,currency=currency,**row))
        for (account,name,instrument),value in sorted(gl.items()):wanted['trial_balance'].append(dict(scenario=scenario,account=account,gl_account=name,instrument_id=instrument,balance=value))
    keys=sorted({(a,e) for _,a,e in total_events})
    for scenario in scenarios[1:]:
        for account,event in keys:
            base=total_events['baseline',account,event];stress=total_events[scenario,account,event];wanted['attribution'].append(dict(scenario=scenario,account=account,event=event,cash_delta=stress[0]-base[0],earnings_delta=stress[1]-base[1],aoci_delta=0.))
    return wanted


def verify(prepared,events,writer,check):
    wanted=reports(prepared,events,check)
    for table,expected in wanted.items():
        rows=[]
        for frame in writer.frames(table):check();rows.extend(frame.to_dicts())
        compare(expected,rows,f'$.persisted.{table}',check)
    return len(wanted['journal'])
