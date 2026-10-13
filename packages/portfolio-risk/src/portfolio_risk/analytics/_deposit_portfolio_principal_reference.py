"""Independent artifact oracle. Never a runtime/native callback; raw histories own economics."""
from copy import deepcopy
import math,sys,json
from . import _deposit_portfolio_income_reference as c
from ._deposit_portfolio_project_reference import project
SCHEMA='deposit-source-income-2'
OWNERSHIP='source_retained_mass_counted_fifo-2'

def income(q,atoms,check):
    p=q['opening_principal'];a=q['opening_accrued_interest'];carry=q['opening_gross_carrying_value'];y=q['original_annual_effective_rate'];date=q['as_of_ordinal'];rows=[]
    for e in q['events']:
        c.tick(check);opening=carry;eir=carry*math.expm1(math.log1p(y)*(e['payment_ordinal']-date)/365.);amort=eir-e['contractual_interest_earned']-e['integral_fee_cash']
        carry+=eir-e['principal_cash']-e['interest_cash']-e['integral_fee_cash']
        local=[v for v in atoms if v['ordinal']==e['payment_ordinal']]
        if local:p=local[-1]['principal_after']
        a+=e['contractual_interest_earned']-e['interest_cash'];cash=e['principal_cash']+e['interest_cash']+e['integral_fee_cash']+e['nonintegral_fee_cash'];mod=0.;review=None
        if e['yield_change'] is not None:
            review=deepcopy(e['yield_change']['review']);value=sum(v['amount']*math.exp(-math.log1p(y)*(v['payment_ordinal']-e['payment_ordinal'])/365.) for v in e['yield_change']['complete_future_cash']);mod=value-carry;carry=value
        bound=64.*sys.float_info.epsilon*(abs(opening)+abs(cash)+abs(eir))
        if carry<0. and p==0. and abs(a)<=bound and abs(carry)<=bound:carry=0.
        if carry<0.:raise ArithmeticError('negative gross carrying value')
        row=dict(event_id=e['event_id'],source=deepcopy(e['source']),payment_ordinal=e['payment_ordinal'],opening_gross_carrying_value=opening,annual_effective_rate=y,effective_interest=eir,contractual_interest_earned=e['contractual_interest_earned'],basis_amortization=amort,modification_gain_loss=mod,recognized_nonintegral_fee_income=e['nonintegral_fee_cash'],cash_received=cash,closing_principal=p,closing_accrued_interest=a,closing_gross_carrying_value=carry,closing_book_adjustment=carry-p-a,yield_review=review)
        for v in row.values():
            if type(v) is float:c.money(v)
        rows.append(row);date=e['payment_ordinal']
    return dict(schema='deposit-source-owned-income-result-1',**{k:deepcopy(q[k]) for k in ('contract','policy','original_yield_anchor','currency','monetary_unit','measure')},rows=rows,remaining_principal=p,remaining_accrued_interest=a,remaining_gross_carrying_value=carry)

def admission(raw):
    c.admit(c.typed(raw['continuation']));return {}

def source_owned(r,changed,check):
    raw=r['continuation'];atoms=[];source=project(raw['current' if changed else 'original'],check,atoms);source['total_interest_expense']=float(source['total_interest_expense']);transitions=[];p=raw['anchor']['original_principal'];cash_by_id={x['event_id']:x for x in source['cashflows']};seen=set()
    for atom in atoms:
        c.tick(check);c.exact(p,atom['principal_before']);pay=0.
        for name in atom['cash_event_ids']:
            if name in seen:raise ArithmeticError('double cash atom')
            seen.add(name);event=cash_by_id[name];c.exact(event['ordinal'],atom['ordinal']);pay-=event['principal']
        by_cash=p-pay;delta=atom['principal_after']-by_cash
        transitions.append(dict(allocation_index=atom['allocation_index'],ordinal=atom['ordinal'],principal_cash=pay,cash_subtraction_principal=by_cash,principal_partition_delta=delta,basis_partition_delta=-delta));p=atom['principal_after']
    c.exact(p,0.);q=c.income_request(raw,source,changed);inc=income(q,atoms,check);rights=c.rights(source,q['full_contract_end_ordinal'],check)
    for o in rights['origins']:c.near(o['remaining'],0.)
    for s in rights['settlements']:c.near(s['retained_cash_roundoff_right'],0.)
    c.near(rights['total_right'],0.)
    for k in ('remaining_principal','remaining_accrued_interest','remaining_gross_carrying_value'):c.near(inc[k],0.)
    return dict(source=source,atoms=atoms,transitions=transitions,income_request=q,income=inc,rights=rights)

def reconstruct(raw,check=lambda:False):
    c.tick(check);c.keys(raw,'schema review ownership continuation');c.exact(raw['schema'],SCHEMA);c.exact(raw['ownership'],OWNERSHIP);c.identity(raw['review']);r=c.typed(raw);admit=admission(r);old=r['continuation'];start,rev,end=c.admit(old)
    original=source_owned(r,False,check);current=source_owned(r,True,check);oc=c.obligations(original['source']);cc=c.obligations(current['source'])
    for d in range(start,rev+1):c.exact(oc.get(d),cc.get(d))
    y=old['anchor']['original_annual_effective_rate'];pv=sum(sum(v)*math.exp(-math.log1p(y)*(d-start)/365.) for d,v in oc.items());c.near(pv,old['anchor']['original_gross_carrying_value']);idx=rev-start-1;row=current['income']['rows'][idx];c.near(row['opening_gross_carrying_value']+row['effective_interest']-row['cash_received'],original['income']['rows'][idx]['closing_gross_carrying_value']);rights=c.rights(current['source'],rev,check);c.near(rights['total_right'],row['closing_accrued_interest']);groups=[];gl=dict(deposit_principal=-old['anchor']['original_principal'],deposit_book_basis=-(old['anchor']['original_gross_carrying_value']-old['anchor']['original_principal']),interest_payable=0.)
    def group(n,ordinal,kind,review,lines):
        values=[dict(account=k,amount=v) for k,v in lines]
        if abs(sum(v['amount'] for v in values))>1e-9:raise ArithmeticError('atomic journal balance')
        groups.append(dict(ordinal=ordinal,kind=kind,source_income_row=n,review=deepcopy(review),lines=values))
        for x in values:gl[x['account']]=gl.get(x['account'],0.)+x['amount']
    for n,x in enumerate(current['income']['rows']):
        c.tick(check);e=current['income_request']['events'][n];day=x['payment_ordinal']
        group(n,day,'original_eir_and_earned',old['contract'],[('interest_payable',-x['contractual_interest_earned']),('deposit_book_basis',-x['basis_amortization']),('pnl:deposit_original_eir_expense',x['effective_interest'])])
        group(n,day,'actual_cash_settlement',old['contract'],[('deposit_principal',e['principal_cash']),('interest_payable',e['interest_cash']),('cash',-x['cash_received'])])
        for t in current['transitions']:
            if t['ordinal']==day:group(n,day,'source_principal_basis_reclassification',r['review'],[('deposit_principal',-t['principal_partition_delta']),('deposit_book_basis',t['principal_partition_delta'])])
        if x['yield_review'] is not None:group(n,day,'reviewed_expectation_revision',old['policy']['review'],[('deposit_book_basis',-x['modification_gain_loss']),('pnl:deposit_basis_revision',x['modification_gain_loss'])])
        for k,key in [('deposit_principal','closing_principal'),('deposit_book_basis','closing_book_adjustment'),('interest_payable','closing_accrued_interest')]:c.near(-gl[k],x[key])
    return dict(schema=SCHEMA,raw_request=r,admission=admit,original=original,current=current,original_pv=pv,basis_revision=row['modification_gain_loss'],revision_rights=rights,journal_deltas=groups,journal_closing=dict(sorted(gl.items())),accounting_standard_approval=False,empirical_validation=False,installed_adoption=False,saved_book_adoption=False,mixed_ledger_adoption=False,capital_ftp_eligible=False)

def verify(raw,actual,check=lambda:False):
    expected=reconstruct(raw,check);c.exact(actual['raw_request'],expected['raw_request']);c.exact(actual['admission'],expected['admission']);c.equal(actual,expected)
    # P checkpoint/delta is the independently recomputed raw arithmetic partition, not a free amount within near().
    for k in ('original','current'):
        c.exact(actual[k]['atoms'],expected[k]['atoms']);c.exact(actual[k]['transitions'],expected[k]['transitions'])
    for observed,wanted in zip(actual['journal_deltas'],expected['journal_deltas']):
        if wanted['kind']=='source_principal_basis_reclassification':c.exact(observed,wanted)
    if len(json.dumps(actual,separators=(',',':')).encode())>actual['admission']['reserved_bytes']:raise ArithmeticError('actual byte bound')
    return dict(verified=True,raw_source_recomputed=True,source_principal_partition_recomputed=True,original_eir_recomputed=True,ordered_reclassification_journal_recomputed=True,complete_unweighted_origins_verified=True,installed_adoption=False,capital_ftp_eligible=False)
