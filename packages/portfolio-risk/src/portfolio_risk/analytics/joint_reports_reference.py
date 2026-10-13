"""Research daily ratios and statements from one independently replayed joint GL."""
from collections import defaultdict
from . import liquidity_policy_reference as liquidity_policy
from ._deposit_portfolio_calendar import _side
from .joint_derivative_source_reference import gross_state
from .saved_derivative_mixed_reference import LIABILITIES

def observe(spec,positions,accounts,rights,accrued,scenario,day,contexts):
    ratio=lambda a,b:a/b if b>0. else None;output=[]
    for a in accounts:
        assets=a['cash'];liabilities=rwa=outflow=rsf=collateral=undrawn=l2=inflow=htm=0.
        hqla=max(a['cash'],0.);asf=max(a['equity'],0.)
        for context in contexts:
            if context['account']==a['id']:
                da,dl=gross_state(context['book']);assets+=da;liabilities+=dl
        for p in positions:
            if p['account']!=a['id']:continue
            carrying=p['balance']+p['book_adjustment']-p['allowance']
            if _side(p)>0.:
                assets+=carrying;rwa+=max(carrying,0.)*p['risk_weight']*(1.+p['watch_fraction']);rsf+=max(carrying,0.)*p['rsf_weight']
                free=p['balance']*max(1.-p['encumbered_fraction'],0.)
                if p['hqla_level']=='level2a':l2+=free*p['hqla_weight']
                else:hqla+=free*p['hqla_weight']
                inflow+=p['balance']*p['lcr_inflow_weight']
            else:
                liabilities+=p['balance']+p['book_adjustment'];asf+=p['balance']*p['asf_weight'];outflow+=p['balance']*p['lcr_outflow_weight']
            r=rights[p['id']];assets+=r[0]+r[2];liabilities-=r[1]+r[3]
            if p['classification']=='htm':htm+=p['balance']+p['book_adjustment']
        hqla+=min(l2,hqla*.4/(1.-.4));outflow=max(outflow-min(inflow,.75*outflow),0.)
        error=assets-liabilities-a['equity'];cet1=a['equity']-a['capital_deductions'];exposure=max(assets,0.)+undrawn
        if abs(error)>1e-8*max(1.,abs(assets),abs(liabilities)):raise ArithmeticError('joint independent EOD balance sheet fails')
        if a['cash']<a['cash_floor']-1e-9:raise ArithmeticError('joint pooled cash floor fails')
        output.append(dict(scenario=scenario,account=a['id'],entity=a['entity'],currency=a['currency'],day=day,cash=a['cash'],restricted_cash=0.,cash_floor=a['cash_floor'],assets=assets,liabilities=liabilities,equity=a['equity'],aoci=0.,cet1=cet1,rwa=rwa,cet1_ratio=ratio(cet1,rwa),leverage_ratio=ratio(cet1,exposure),usable_collateral=collateral,undrawn=undrawn,hqla_proxy=hqla,lcr_proxy=ratio(hqla,outflow),nsfr_proxy=ratio(asf,rsf),reconciliation_error=error,cash_headroom=a['cash']-a['cash_floor'],cet1_headroom=cet1-a['cet1_floor']*rwa,leverage_headroom=cet1-a['leverage_floor']*exposure,lcr_headroom=hqla-a['lcr_floor']*outflow,nsfr_headroom=asf-a['nsfr_floor']*rsf,htm_headroom=a['htm_asset_limit']*max(assets,0.)-htm))
        liquidity_policy.apply_path(output[-1], a)
    return output

def statements(trial,accounts,scenarios=()):
    funding={(r['scenario'],r['account'],r['instrument_id'])for r in trial if r['gl_account']=='funding_principal'}
    totals=defaultdict(lambda:dict(assets=0.,liabilities=0.,equity=0.,cash=0.,restricted_cash=0.,intercompany_assets=0.,intercompany_liabilities=0.))
    for scenario in scenarios:
        for account in accounts:totals[scenario,account]
    for r in trial:
        key=r['scenario'],r['account'];t=totals[key];gl,value=r['gl_account'],r['balance']
        if gl.startswith('memo:'):raise ArithmeticError('custody memo posted to financial GL')
        if gl in {'cash','restricted_cash'}:t[gl]+=value
        if gl.startswith('pnl:')or gl in {'opening_equity','oci','equity_distributions'}:t['equity']-=value
        elif gl in LIABILITIES|{'secured_funding','received_margin','intercompany_payable'}:t['liabilities']-=value
        elif gl in {'book_adjustment','accrued_interest'}and(*key,r['instrument_id'])in funding:t['liabilities']-=value
        elif gl=='derivative_value'and value<0.:t['liabilities']-=value
        else:t['assets']+=value
        if gl=='intercompany_receivable':t['intercompany_assets']+=value
        elif gl=='intercompany_payable':t['intercompany_liabilities']-=value
    rows=[];consolidated=defaultdict(lambda:dict(assets=0.,liabilities=0.,equity=0.,cash=0.,restricted_cash=0.))
    for(scenario,account),t in totals.items():
        currency=accounts[account].currency;error=t['assets']-t['liabilities']-t['equity']
        if abs(error)>1e-8*max(1.,abs(t['assets']),abs(t['liabilities'])):raise ArithmeticError('joint journal-derived closing statements fail')
        rows.append(dict(scenario=scenario,account=account,currency=currency,**t,reconciliation_error=error));group=consolidated[scenario,currency]
        for field in group:group[field]+=t[field]
        group['assets']-=t['intercompany_assets'];group['liabilities']-=t['intercompany_liabilities']
    return rows,[dict(scenario=s,currency=c,**r)for(s,c),r in consolidated.items()]
