"""Independent artifact closing-capital acceptance; never a finance backend.

The immutable raw source and every persisted row pass the full mixed oracle
before closing trial amounts can be used. No native callback is imported.
"""
from dataclasses import dataclass
from copy import deepcopy
from datetime import date
import math
from portfolio_risk.analytics import mortgage_mixed_reference as mixed
from portfolio_risk.analytics import _mortgage_mixed_financial_reference as financial

PENDING_GLS = frozenset('mortgage_pending_default_principal mortgage_pending_default_interest_receivable mortgage_pending_default_basis mortgage_pending_prepay_principal mortgage_pending_prepay_interest_receivable mortgage_pending_prepay_basis'.split())
ASSETS = frozenset('cash restricted_cash asset_principal book_adjustment allowance accrued_interest fair_value_adjustment recovery_receivable derivative_value posted_margin intercompany_receivable dated_interest_receivable dated_prepaid_expense credit_default_principal credit_default_book_adjustment credit_default_interest_receivable live_interest_basis_adjustment default_interest_basis_adjustment'.split()) | PENDING_GLS
LIABILITIES = frozenset('funding_principal secured_funding received_margin intercompany_payable dated_interest_payable dated_deferred_income credit_default_deferred_income'.split())
SEAL = object()
TREATMENTS=frozenset(('reject_negative_exposure_groups','research_signed_exposure_groups','reviewed_nonnegative_group_exposure'))

# Declared financial DTO fields, separate from the immutable reviewed JSON.
# Browser JSON spells integral money/rates as integers; Rust deserializes these
# fields as f64. Independent report expectations must use the same declared type
# without weakening exact input, policy, boolean or metadata identity checks.
CAPITAL_AMOUNTS = tuple('common_equity retained_earnings eligible_aoci cet1_deductions additional_tier1 tier2 eligible_tlac eligible_ltd credit_rwa market_rwa operational_rwa average_assets total_leverage_exposure tangible_common_equity tangible_assets'.split())
POLICY_AMOUNTS = ('equity_exclusions', 'intangible_assets', 'leverage_addon', 'leverage_deductions')


def _financial_policy(policy):
    """Normalize only declared f64 fields in a private independent report copy."""
    financial.keys(policy, 'account scenario capital include_aoci equity_exclusions intangible_assets leverage_addon leverage_deductions gl_risk_weights instrument_risk_weights')
    if type(policy['include_aoci']) is not bool:
        raise ArithmeticError('reviewed AOCI inclusion must be boolean')
    result = deepcopy(policy)

    def number(value):
        if type(value) not in (int, float) or not math.isfinite(value):
            raise ArithmeticError('finite declared capital/weight financial field required')
        return float(value)

    for key in POLICY_AMOUNTS:
        result[key] = number(policy[key])
    for key in ('gl_risk_weights', 'instrument_risk_weights'):
        if type(policy[key]) is not dict:
            raise ArithmeticError('explicit risk weight mapping required')
        result[key] = {name: number(value) for name, value in policy[key].items()}
    capital = result['capital']
    # Option<f64> may be absent in JSON; retain its declared unavailable value.
    capital.setdefault('advanced_rwa', None)
    financial.keys(capital, (*CAPITAL_AMOUNTS, 'entity', 'currency', 'scenario', 'period', 'advanced_rwa', 'requirements'))
    for key in CAPITAL_AMOUNTS:
        capital[key] = number(capital[key])
    if capital['advanced_rwa'] is not None:
        capital['advanced_rwa'] = number(capital['advanced_rwa'])
    if type(capital['requirements']) is not dict:
        raise ArithmeticError('explicit capital requirement mapping required')
    for rule in capital['requirements'].values():
        financial.keys(rule, 'minimum buffer management')
        for key in ('minimum', 'buffer', 'management'):
            rule[key] = number(rule[key])
    return result

def exposure_policy(policy):
    financial.keys(policy,'identity negative_net_carrying')
    financial.domain.identity(policy['identity'])
    if type(policy['negative_net_carrying']) is not str or policy['negative_net_carrying'] not in TREATMENTS:
        raise ArithmeticError('explicit reviewed negative net carrying treatment required')
    return policy['negative_net_carrying']

@dataclass(frozen=True)
class ClosingControl:
    mixed: object
    receipt: dict
    receipt_digest: str
    seal: object

def verify_source(raw, header, frames, check=lambda: False, **operational):
    """frames is a bounded repeatable transport facade or raw-message iterable."""
    if set(header)=={'schema','model_schema','prepared_request','source_receipt'}:
        header={'workflow':header}
    control = mixed.verify_workflow(raw, header, check, **operational)
    receipt = mixed.verify_stream(raw, control, frames, check)
    for source in control.sources.values():
        financial.closed(source['closing'])  # all seven slots and economic claims
    return ClosingControl(control, receipt, mixed.digest(receipt), SEAL)

def native_request(specification, raw, closing_review, control):
    """Private transport preparation; callers cannot select a legacy protocol."""
    if control.seal is not SEAL or control.receipt.get('verified') is not True:
        raise ArithmeticError('complete independent source/persisted replay required')
    c=control.mixed;c.guard.tick()
    if mixed.digest(control.receipt)!=control.receipt_digest or mixed._control_digest(c.raw,c.specification,c.events,c.bindings,c.sources,c.topology,c.admission)!=c.content_digest:
        raise ArithmeticError('verified source/GL control changed before closing')
    # Keep exact original queued identity separate from declared DTO f64
    # normalization. Both are independently derived by source admission.
    if mixed.digest(raw)!=c.input_digest:
        financial.exact(raw,c.raw,'declared normalized source identity')
    financial.keys(specification, 'kind treasury policies amount_multiplier exposure_policy')
    exposure_policy(specification['exposure_policy'])
    if specification['kind'] != 'ledger':
        raise ArithmeticError('mortgage closing never authorizes FTP/credit protocol')
    financial.domain.identity(closing_review)
    end = control.mixed.raw['dated']['dated']['as_of_ordinal'] + control.mixed.specification['horizon_days']
    if specification['treasury']['as_of'] != date.fromordinal(end).isoformat():
        raise ArithmeticError('capital date must equal complete tail, never report date')
    if specification['treasury']['curves'] or specification['treasury']['positions']:
        raise ArithmeticError('closing source cannot fund FTP positions')
    accounts = {a['id']:a for a in control.mixed.specification['accounts']}
    scopes = {(r['scenario_id'],o['account_id']) for o in raw['owners'] for r in o['runs']}
    scopes |= {(r['scenario_id'],o['account_id']) for o in raw['dated']['dated']['owners'] for r in raw['dated']['dated']['runs']}
    policies = specification['policies']; seen=set()
    for policy in policies:
        key=(policy['scenario'],policy['account'])
        if key in seen or key not in scopes: raise ArithmeticError('exact capital policy scope')
        seen.add(key); a=accounts[key[1]]
        if any(policy['capital'][k] != a[k] for k in ('currency','entity')) or policy['capital']['scenario'] != key[0]:
            raise ArithmeticError('capital currency/entity/scenario mapping')
    if seen != scopes: raise ArithmeticError('missing closing capital policy scope')
    return dict(schema='mortgage-closing-capital-1', treasury=deepcopy(specification['treasury']), policies=deepcopy(policies),
                amount_multiplier=specification['amount_multiplier'], raw_mixed=deepcopy(c.raw), closing_review=deepcopy(closing_review),exposure_policy=deepcopy(specification['exposure_policy']))

def verify_capital(request, actual, control, check=lambda: False):
    """Replay verified source GL independently, then check native capital output."""
    if control.seal is not SEAL: raise ArithmeticError('private closing control required')
    c=control.mixed; c.guard.tick(extra=check)
    # Recheck complete original raw and reviewed policy through the same admission.
    private=native_request({k:request[k] for k in ('treasury','policies','amount_multiplier','exposure_policy')} | {'kind':'ledger'},request['raw_mixed'],request['closing_review'],control)
    financial.exact(request,private,'immutable bridge request')
    financial.exact(request['schema'],'mortgage-closing-capital-1','public capital schema')
    source=actual['mortgage_closing_source']
    financial.keys(source,'schema raw_mixed_sha256 raw_mixed_hash_policy raw_mixed review full_end_ordinal source_closings native_raw_replayed native_ordered_journal_replayed journal_rows source_work_units bridge_work_units bridge_work_bound bridge_structural_work_bound stream_decode_bytes_charged bridge_work_limit closing_capital_eligible capital_ftp_eligible interim_capital_eligible accounting_standard_approval empirical_validation production_adoption regulatory_credit_rwa_approval')
    financial.exact(source['schema'],'mortgage-closing-capital-source-1','successor closing schema')
    financial.exact(source['raw_mixed'],c.raw,'native raw original/current bridge clone')
    financial.exact(source['review'],request['closing_review'],'reviewed closing permission')
    end=c.raw['dated']['dated']['as_of_ordinal']+c.specification['horizon_days']
    financial.exact(source['full_end_ordinal'],end,'exact full-tail native date')
    for k in ('native_raw_replayed','native_ordered_journal_replayed','closing_capital_eligible'):
        financial.exact(source[k],True,k)
    for k in ('capital_ftp_eligible','interim_capital_eligible','accounting_standard_approval','empirical_validation','production_adoption','regulatory_credit_rwa_approval'):
        financial.exact(source[k],False,k)
    expected=[dict(source_id=item['source_id'],closing=c.sources[item['source_id']]['closing']) for item in c.raw['sources']]
    financial.equal(source['source_closings'],expected,'complete unweighted closing source/claims')
    for field in ('journal_rows','source_work_units','bridge_work_units','bridge_work_bound','bridge_structural_work_bound','stream_decode_bytes_charged','bridge_work_limit'):
        if type(source[field]) is not int or source[field]<0:
            raise ArithmeticError('exact nonnegative integer closing work receipt/'+field)
    if source['bridge_work_limit']!=2_000_000 or not 0<=source['bridge_work_units']<=source['bridge_work_bound']<=2_000_000 or source['bridge_structural_work_bound']>2_000_000 or source['stream_decode_bytes_charged']>source['bridge_work_units']:
        raise ArithmeticError('separate bounded closing work receipt')
    from .mortgage_capital_identity import digest as identity_digest, POLICY
    financial.exact(source['raw_mixed_hash_policy'],POLICY,'typed hash policy')
    financial.exact(source['raw_mixed_sha256'],identity_digest(c.raw),'native typed raw source digest')
    digest=source['raw_mixed_sha256']
    if type(digest) is not str or len(digest)!=64 or any(x not in '0123456789abcdef' for x in digest):
        raise ArithmeticError('native normalized raw source SHA256 identity')
    financial.exact(source['source_work_units'],c.admission['work_units'],'source work admission')
    financial.exact(source['journal_rows'],sum(1 for _ in mixed._rows(c,'journal',check)),'complete authoritative journal coverage count')
    policies={(p['scenario'],p['account']):_financial_policy(p) for p in request['policies']}; amounts={}; contributions=[];groups={}
    treatment=exposure_policy(request['exposure_policy'])
    rows=list(mixed._rows(c,'trial_balance',check))  # bounded closing GL only, no journal hydration
    funding={(r['scenario'],r['account'],r['instrument_id']) for r in rows if r['gl_account']=='funding_principal'}
    multiplier=request['amount_multiplier']
    if type(multiplier) not in (float,int) or not math.isfinite(multiplier) or multiplier<=0: raise ArithmeticError('positive monetary multiplier required')
    for row in rows:
        c.guard.tick(extra=check); key=(row['scenario'],row['account']); p=policies[key]
        v=row['balance']*multiplier; gl=row['gl_account']; ident=row['instrument_id']
        a=amounts.setdefault(key,[0.]*7)
        if gl in PENDING_GLS: financial.near(row['balance'],0.,'closed pending key')
        if gl in ('opening_equity','equity_distributions'): a[2]-=v
        elif gl.startswith('pnl:'): a[3]-=v
        elif gl=='oci': a[4]-=v
        elif gl in LIABILITIES or gl in ('book_adjustment','accrued_interest') and (*key,ident) in funding or gl=='derivative_value' and v<0: a[1]-=v
        elif gl in ASSETS:
            weight=p['instrument_risk_weights'].get(ident,p['gl_risk_weights'].get(gl))
            if weight is None:
                if v==0: weight=0.
                else: raise ArithmeticError('explicit asset risk weight required')
            if type(weight) not in (float,int) or not math.isfinite(weight) or weight<0: raise ArithmeticError('risk weight domain')
            a[0]+=v; a[5]+=v*weight
            group=groups.setdefault((*key,ident),[0.,weight])
            if group[1]!=weight:raise ArithmeticError('one reviewed risk weight required per exposure group')
            group[0]+=v
            contributions.append(dict(scenario=key[0],account=key[1],gl_account=gl,instrument_id=ident,balance=v,risk_weight=weight,rwa_contribution=v*weight))
        else: raise ArithmeticError('unreviewed closing GL mapping '+gl)
        a[6]+=v
    group_audit=[];research_scopes=set()
    for a in amounts.values():a[5]=0.
    for (scenario,account,instrument),(net,weight) in sorted(groups.items()):
        if net<0. and treatment=='reject_negative_exposure_groups':raise ArithmeticError('negative net carrying exposure group')
        exposure=max(net,0.) if treatment=='reviewed_nonnegative_group_exposure' else net
        if net<0. and treatment=='research_signed_exposure_groups':research_scopes.add((scenario,account))
        contribution=exposure*weight;amounts[scenario,account][5]+=contribution
        group_audit.append(dict(scenario=scenario,account=account,instrument_id=instrument,accounting_net_carrying=net,policy_exposure=exposure,risk_weight=weight,policy_rwa_contribution=contribution,negative_accounting_exposure=net<0.))
    financial.equal(actual['exposure_groups'],group_audit,'independent grouped carrying/exposure policy')
    # Computed tolerances do not make a negative-exposure boolean interchangeable
    # with a positive/zero group. If independent arithmetic differs at zero, fail.
    for a,e in zip(actual['exposure_groups'],group_audit,strict=True):
        financial.exact(a['negative_accounting_exposure'],e['negative_accounting_exposure'],'negative exposure identity')
        financial.exact(a['negative_accounting_exposure'],a['accounting_net_carrying']<0.,'reported group sign consistency')
        financial.exact(a['risk_weight'],float(e['risk_weight']),'supplied group risk weight')
        if treatment=='reviewed_nonnegative_group_exposure' and a['accounting_net_carrying']<0.:
            financial.exact(a['policy_exposure'],0.,'explicit reviewed zero exposure')
    financial.exact(actual['exposure_policy'],request['exposure_policy'],'reviewed exposure policy')
    financial.exact(actual['signed_exposure_research_only'],bool(research_scopes),'research signed exposure disclosure')
    financial.exact(actual['regulatory_credit_rwa_approval'],False,'no regulatory approval')
    audits=[]; capitals=[]
    for key,a in sorted(amounts.items()):
        p=policies[key]; equity=a[2]+a[3]+a[4]; capital=deepcopy(p['capital'])
        capital.update(scenario=key[0],common_equity=a[2]-p['equity_exclusions'],retained_earnings=a[3],eligible_aoci=a[4] if p['include_aoci'] else 0.,credit_rwa=a[5],total_leverage_exposure=a[0]+p['leverage_addon']-p['leverage_deductions'],tangible_common_equity=equity-p['equity_exclusions']-p['intangible_assets'],tangible_assets=a[0]-p['intangible_assets'])
        capitals.append(capital)
        audits.append(dict(scenario=key[0],account=key[1],entity=capital['entity'],currency=capital['currency'],assets=a[0],liabilities=a[1],book_equity=equity,common_equity=capital['common_equity'],retained_earnings=a[3],book_aoci=a[4],eligible_aoci=capital['eligible_aoci'],equity_exclusions=p['equity_exclusions'],credit_rwa=a[5],reconciliation_error=a[6]))
    financial.equal(actual['capital_bridge'],audits,'native closing capital independent GL')
    financial.equal(actual['rwa_contributions'],contributions,'signed carrying/RWA and explicit weights')
    financial.equal(actual['prepared_specification']['capital'],capitals,'prepared eligible amounts')
    metrics=[]
    for capital in capitals:
        cet1=capital['common_equity']+capital['retained_earnings']+capital['eligible_aoci']-capital['cet1_deductions']
        tier1=cet1+capital['additional_tier1']; total=tier1+capital['tier2']
        rwa=capital['credit_rwa']+capital['market_rwa']+capital['operational_rwa']
        exposure=capital['total_leverage_exposure']
        components=[('cet1',cet1,rwa),('tier1',tier1,rwa),('total_capital',total,rwa),
            ('advanced_cet1',cet1,capital['advanced_rwa']),('advanced_tier1',tier1,capital['advanced_rwa']),('advanced_total_capital',total,capital['advanced_rwa']),
            ('tier1_leverage',tier1,capital['average_assets']),('slr',tier1,exposure),('tlac_rwa',capital['eligible_tlac'],rwa),('tlac_leverage',capital['eligible_tlac'],exposure),
            ('ltd_rwa',capital['eligible_ltd'],rwa),('ltd_leverage',capital['eligible_ltd'],exposure),('tce_ta',capital['tangible_common_equity'],capital['tangible_assets'])]
        for label,numerator,denominator in components:
            denominator=denominator if denominator is not None and denominator>0 else None
            rule=capital['requirements'].get(label)
            ratio=numerator/denominator if denominator is not None else None
            required=rule['minimum']+rule['buffer'] if rule else None
            management=required+rule['management'] if rule else None
            head=numerator-required*denominator if required is not None and denominator is not None else None
            management_head=numerator-management*denominator if management is not None and denominator is not None else None
            status='unavailable' if denominator is None else 'unconfigured' if rule is None else 'breach' if head<0 else 'below_management_target' if management_head<0 else 'pass'
            if label in ('cet1','tier1','total_capital','tlac_rwa','ltd_rwa') and any(policies[key]['capital']['entity']==capital['entity'] and policies[key]['capital']['currency']==capital['currency'] and policies[key]['capital']['period']==capital['period'] and key[0]==capital['scenario'] for key in research_scopes):status='research_only_signed_exposure'
            metrics.append({k:capital[k] for k in ('entity','currency','scenario','period')} | dict(metric=label,numerator=numerator,denominator=denominator,ratio=ratio,minimum=rule['minimum'] if rule else None,buffer=rule['buffer'] if rule else None,required_ratio=required,management_target=management,headroom=head,management_headroom=management_head,headroom_bp=(ratio-required)*10000 if ratio is not None and required is not None else None,status=status))
    financial.equal(actual['capital_metrics'],metrics,'thirteen independent native ratios/statuses')
    if actual['ftp_positions'] or actual['ftp_reconciliation']:
        raise ArithmeticError('closing source may not infer modeled FTP')
    for closing in mixed._rows(c,'closing_statements',check):
        audit=next(a for a in audits if (a['scenario'],a['account'])==(closing['scenario'],closing['account']))
        for x,y in [('assets','assets'),('liabilities','liabilities'),('book_equity','equity')]: financial.near(audit[x],closing[y]*multiplier,'verified native closing statement')
    c.guard.tick(extra=check)
    return dict(schema='mortgage-closing-capital-source-1',verified=True,native_mortgage_source_recomputed=True,ordered_mixed_journal_recomputed=True,complete_unweighted_tail_verified=True,full_end_ordinal=end,closing_capital_eligible=True,capital_ftp_eligible=False,interim_capital_eligible=False,accounting_standard_approval=False,regulatory_credit_rwa_approval=False,signed_exposure_research_only=bool(research_scopes),exposure_policy=deepcopy(request['exposure_policy']),empirical_validation=False,production_adoption=False,raw_request_sha256=control.receipt['raw_request_sha256'],stream_sha256=control.receipt['stream_sha256'],review=deepcopy(request['closing_review']))
