"""Independent supplied-policy oracle; never calls a financial runtime."""
import copy
import json
import math
import struct
import unicodedata

LANES = ('active_principal','pending_default_principal','pending_prepay_principal',
         'default_claim_principal','live_interest_receivable','live_interest_payable',
         'default_interest_receivable','default_interest_payable','principal_basis',
         'live_interest_basis','default_principal_basis','default_interest_basis',
         'borrower_principal_memo','borrower_interest_memo','economic_principal_memo',
         'economic_interest_memo','undrawn_memo','allowance')
INCOME = ('original_eir','reviewed_basis','incurred_credit','recovery_income',
          'recovery_cost','initial_allowance','deposit_revision')
FLOATS = {'amount','bank_cash','bank_earnings','opening_funding_balance','source_weight',
          'unit_face','allocated_face','reference_years','liquidity_years'}

def _require(ok, message):
    if not ok: raise ValueError(message)

def _keys(v, fields):
    _require(type(v) is dict and set(v)==set(fields.split()), 'strict object fields')

def _text(v):
    _require(type(v) is str and v.strip() and len(v.encode())<=200 and
             all(unicodedata.category(c)!='Cc' for c in v), 'bounded explicit label')

def _identity(v):
    _keys(v,'id revision')
    _text(v['id']); _text(v['revision'])

def _money(v):
    _require(type(v) in (int,float) and math.isfinite(v) and abs(v)<=10000, 'bounded finite money')

def _int(v, lo, hi):
    _require(type(v) is int and lo<=v<=hi, 'exact bounded integer')

def _near(a,b): return abs(a-b)<=1e-9+1e-12*abs(b)

def _normalize(v):
    if type(v) is dict:
        return {k:(float(x) if k in FLOATS else _normalize(x)) for k,x in v.items()}
    if type(v) is list: return [_normalize(x) for x in v]
    return v

def _exact(a,b):
    if type(a) is not type(b): return False
    if type(a) is float: return struct.pack('>d',a)==struct.pack('>d',b)
    if type(a) is dict: return set(a)==set(b) and all(_exact(a[k],b[k]) for k in a)
    if type(a) is list: return len(a)==len(b) and all(_exact(x,y) for x,y in zip(a,b))
    return a==b

def prepare(raw, check=lambda:False, *, independent_source_residuals=False):
    portfolio=raw.get("schema")=="modeled-product-ftp-source-channels-2"
    def probe():
        if check(): raise InterruptedError('proof cancelled')
    probe()
    _keys(raw,'schema measure source original_yield_anchor path_id entity currency_unit scenario_id side start_ordinal full_end_ordinal source_weight unit_face allocated_face opening_funding_balance policy states cash changes earned resets')
    _require(len(json.dumps(raw,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode())<=(8388608 if portfolio else 1048576),'raw bound')
    _require(raw['schema'] in ('modeled-product-ftp-source-channels-1','modeled-product-ftp-source-channels-2') and raw['measure']=='physical','physical proof')
    for name in ('path_id','entity','currency_unit','scenario_id'): _text(raw[name])
    _identity(raw['source']); _identity(raw['original_yield_anchor'])
    start,end=raw['start_ordinal'],raw['full_end_ordinal']
    _int(start,1,3652059); _int(end,1,3652059)
    n=end-start
    _require(1<=n<=(365 if portfolio else 64) and len(raw['states'])==n+1,'complete daily clock')
    _require(raw['side'] in ('asset','liability'),'side')
    p=raw['policy']; _keys(p,'review accrual_basis averaging negative_funding principal_zero_rights termination tail channels'); _identity(p['review'])
    _require(p['accrual_basis']=='ACT/365F','explicit accrual basis')
    _require(p['averaging'] in ('opening_daily','trapezoid_daily') and
             p['negative_funding'] in ('reject_negative','research_signed') and
             p['principal_zero_rights'] in ('reject_unsettled','explicit_reviewed_all_channels') and
             p['termination']=='complete_gross_rights_and_commitment' and
             p['tail']=='no_synthetic_cash_or_tail','explicit policies')
    _require(len(p['channels'])==18,'all channel policies')
    for i,c in enumerate(p['channels']):
        _keys(c,'channel coefficient review'); _identity(c['review']); _int(c['coefficient'],-1,1)
        _require(c['channel']==LANES[i] and (i<12 or c['coefficient']==0),'ordered channel/memo policy')
    for key in ('source_weight','unit_face','allocated_face','opening_funding_balance'): _money(raw[key])
    _require(raw['unit_face']>0 and raw['allocated_face']>=0 and 0<=raw['source_weight']<=1,'scale')
    scale=float(raw['allocated_face'])/raw['unit_face']*raw['source_weight']
    _require(math.isfinite(scale) and scale<=10000,'scale limit')
    events=len(raw['cash'])+len(raw['changes'])+len(raw['earned'])
    _require(events<=4096 and 1<=len(raw['resets'])<=(366 if portfolio else 65),'event/reset count')
    _require(all(len(s.get('origins',[]))<=(368 if portfolio else 128) for s in raw['states']),'bounded source origins')
    work=(n+1)*18*16+events*(n+1)*20+sum(len(s.get('origins',[])) for s in raw['states'])*80+len(raw['resets'])*16
    _require(work<=(8000000 if portfolio else 2000000),'work preadmission')
    balances=[]; ids=set()
    for j,s in enumerate(raw['states']):
        probe(); _keys(s,'ordinal source_checkpoint_id channels origins'); _int(s['ordinal'],start,end)
        _text(s['source_checkpoint_id'])
        _require(s['ordinal']==start+j and s['source_checkpoint_id'] not in ids and len(s['channels'])==18,'state coverage')
        ids.add(s['source_checkpoint_id'])
        for i,c in enumerate(s['channels']):
            _keys(c,'channel amount'); _money(c['amount'])
            # Only a freshly independently reconstructed source may retain its
            # numerical memo residue. Native/supplied gross domains stay strict.
            residual = independent_source_residuals and i in (12,13,14,15) and _near(c['amount'],0)
            _require(c['channel']==LANES[i] and (8<=i<=11 or c['amount']>=0 or residual),'gross channel domain')
        origin_keys=set()
        for origin in s['origins']:
            _keys(origin,'origin_id source_event_id channel amount');_text(origin['origin_id']);_text(origin['source_event_id']);_money(origin['amount'])
            _require(origin['channel'] in LANES,'origin channel')
            key=(origin['channel'],origin['origin_id']); index=LANES.index(origin['channel'])
            residual = independent_source_residuals and index in (12,13,14,15) and _near(origin['amount'],0)
            _require(key not in origin_keys and (8<=index<=11 or origin['amount']>=0 or residual),'origin identity/gross domain');origin_keys.add(key)
        for c in s['channels']:
            total=sum(float(o['amount']) for o in s['origins'] if o['channel']==c['channel'])
            _require(_near(total,c['amount']),'channel/origin reconciliation')
        principal=sum(float(c['amount']) for c in s['channels'][:4])
        if p['principal_zero_rights']=='reject_unsettled' and _near(principal,0):
            _require(all(_near(c['amount'],0) for c in s['channels'][4:]),'unsettled P0 rights')
        # Funding is the declared linear channel allocation, not principal runoff.
        b=sum(float(c['amount'])*policy['coefficient'] for c,policy in zip(s['channels'],p['channels']))
        _money(b); _money(b*scale)
        _require(p['negative_funding']!='reject_negative' or b>=0,'negative funding rejected')
        balances.append(b)
    _require(_near(balances[0],raw['opening_funding_balance']),'opening funding allocation')
    _require(all(_near(c['amount'],0) for c in raw['states'][-1]['channels']),'unweighted complete gross tail')
    _require(all(_near(o['amount'],0) for o in raw['states'][-1]['origins']),'individual origin tail')
    cash_by_id={}
    for c in raw['cash']:
        probe(); _keys(c,'event_id ordinal kind bank_cash funding_change_sequence exclusion_review')
        _text(c['event_id']); _int(c['ordinal'],start+1,end); _money(c['bank_cash'])
        _require(c['kind'] in ('acquisition','principal','interest','gross_recovery','recovery_cost','source_atomic_cash') and c['event_id'] not in cash_by_id,'cash domain')
        _require((c['funding_change_sequence'] is None)!=(c['exclusion_review'] is None),'cash ownership')
        if c['exclusion_review'] is not None: _identity(c['exclusion_review'])
        if c['funding_change_sequence'] is not None: _int(c['funding_change_sequence'],1,4096)
        cash_by_id[c['event_id']]=c
    changes_by_date={}; seen=set(); source_ids=set(); prior=start
    side=1 if raw['side']=='asset' else -1
    for j,c in enumerate(raw['changes']):
        probe(); _keys(c,'sequence ordinal source_event_id kind amount cash_event_id review')
        _identity(c['review']); _text(c['source_event_id']); _int(c['sequence'],1,4096); _int(c['ordinal'],start+1,end); _money(c['amount'])
        _require(c['sequence']==j+1 and c['ordinal']>=prior and c['source_event_id'] not in source_ids,'change ordering')
        source_ids.add(c['source_event_id']); prior=c['ordinal']
        if c['kind']=='cash_assignment':
            _text(c['cash_event_id']); _require(c['cash_event_id'] in cash_by_id and c['cash_event_id'] not in seen,'cash once')
            x=cash_by_id[c['cash_event_id']]; seen.add(c['cash_event_id'])
            _require(x['ordinal']==c['ordinal'] and x['funding_change_sequence']==c['sequence'] and
                     _near(c['amount'],-side*float(x['bank_cash'])),'cash assignment financial reconciliation')
        else:
            _require(c['kind'] in ('reviewed_chargeoff_reallocation','reviewed_basis_reallocation','reviewed_other_reallocation') and c['cash_event_id'] is None,'noncash policy')
        changes_by_date.setdefault(c['ordinal'],[]).append(c)
    _require(all(c['funding_change_sequence'] is None or c['event_id'] in seen for c in raw['cash']),'cash assignments complete')
    earned_ids=set(); income_by_date={}
    for e in raw['earned']:
        probe(); _keys(e,'event_id ordinal channel bank_earnings review'); _identity(e['review'])
        _text(e['event_id']); _int(e['ordinal'],start+1,end); _money(e['bank_earnings'])
        _require(e['event_id'] not in earned_ids and e['channel'] in INCOME,'earned domain')
        earned_ids.add(e['event_id']); income_by_date.setdefault(e['ordinal'],[]).append(e)
    prior=None
    for reset in raw['resets']:
        probe(); _keys(reset,'ordinal reference_years liquidity_years review'); _identity(reset['review'])
        _int(reset['ordinal'],start,end-1)
        _require(prior is None or reset['ordinal']>prior,'reset order'); prior=reset['ordinal']
        for key in ('reference_years','liquidity_years'):
            _require(type(reset[key]) in (int,float) and math.isfinite(reset[key]) and 0<=reset[key]<=100,'reset tenor')
    _require(raw['resets'][0]['ordinal']==start,'opening reset')
    r=_normalize(raw); days=[]; current=float(raw['opening_funding_balance'])
    for index in range(1,n+1):
        probe(); date=start+index; before=current
        changes=changes_by_date.get(date,[])
        for c in changes:
            current+=float(c['amount']); _money(current)
            _require(p['negative_funding']!='reject_negative' or current>=0,'negative intermediate funding')
        _require(_near(current,balances[index]),'funding roll-forward')
        average=before if p['averaging']=='opening_daily' else (before+current)/2
        reset=next(x for x in reversed(r['resets']) if x['ordinal']<date)
        cash=[c for c in r['cash'] if c['ordinal']==date]
        earned=income_by_date.get(date,[])
        income=[dict(channel=label,amount=sum(float(e['bank_earnings']) for e in earned if e['channel']==label)*scale) for label in INCOME]
        money=sum(c['bank_cash'] for c in cash)*scale
        profit=sum(e['amount'] for e in income)
        for v in [average*scale,money,profit]+[e['amount'] for e in income]: _money(v)
        days.append(dict(ordinal=date,direction=float(side),opening_funding_balance=before,closing_funding_balance=current,
            average_funding_balance=average,scaled_average_funding_balance=average*scale,
            accrual_fraction=1.0/365,reference_years=reset['reference_years'],liquidity_years=reset['liquidity_years'],
            reset_review=copy.deepcopy(reset['review']),funding_change_sequences=[c['sequence'] for c in changes],
            cash_event_ids=[c['event_id'] for c in cash],earned_event_ids=[e['event_id'] for e in earned],
            bank_cash=money,original_eir=income[0]['amount'],earnings_by_channel=income,external_earnings=profit))
    _require(_near(current,0),'funding tail')
    output=dict(schema='modeled-product-ftp-preparation-proof-result-2'if portfolio else 'modeled-product-ftp-preparation-proof-result-1',raw_request=r,scale=scale,days=days,
        closing_unweighted_channels=copy.deepcopy(r['states'][-1]['channels']),closing_funding_balance=current,
        admitted_work=work,complete_unweighted_tail_checked=True,source_reconstructed=False,
        ordered_journal_verified=False,ftp_bridge_adoption=False,capital_ftp_eligible=False,
        accounting_standard_approval=False,empirical_validation=False,
        treasury_nonnegative_positions_compatible=all(d['scaled_average_funding_balance']>=0 for d in days))
    _require(len(json.dumps(output,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode())<=(8388608 if portfolio else 1048576),'output bound')
    probe(); return output

def verify(raw, output, check=lambda:False):
    expected=prepare(raw,check)
    def compare(a,b,path=''):
        _require(type(a) is type(b),'wire type '+path)
        if type(b) is dict:
            _require(set(a)==set(b),'wire keys '+path)
            for key in b: compare(a[key],b[key],path+'/'+key)
        elif type(b) is list:
            _require(len(a)==len(b),'wire count '+path)
            for i,(x,y) in enumerate(zip(a,b)): compare(x,y,path+'/'+str(i))
        elif type(b) is float:
            clone=path.startswith('/raw_request') or path.startswith('/closing_unweighted_channels') or path.endswith(('/reference_years','/liquidity_years'))
            _require(_exact(a,b) if clone else math.isfinite(a) and _near(a,b),'financial/identity '+path)
        else: _require(a==b,'identity '+path)
    compare(output,expected)
    return dict(verified=True,supplied_reviewed_preparation_only=True,source_reconstructed=False,
                ordered_journal_verified=False,ftp_bridge_adoption=False,capital_ftp_eligible=False)
