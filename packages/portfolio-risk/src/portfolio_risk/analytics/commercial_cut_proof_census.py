"""Raw-only phase-one compact-cut inventory; always a publication refusal.

This explicit independent verifier never reads a native receipt or invokes native
finance. Supported first source variants are selected finite transfer, fixed
coupons and supplied/dated-supplied principal with complete supplied credit tails.
Generated/exercise/draw/reviewed/pending alternatives refuse before projection.
A complete raw-derived native source/allocation-layout census remains a named
unproved gate. None of the partial dimensions below can grant execution authority.
"""
from __future__ import annotations
from dataclasses import asdict, dataclass, fields, field
from copy import deepcopy
import math
import struct

SCHEMA = 'commercial-cut-proof-census-1'
MAX_RAW = 2 << 20
MAX_WORK = 32_000_000
MAX_BYTES = 128 << 20
MAX_COMPONENT_WORK = 8_000_000
MAX_COMPONENT_BYTES = 32 << 20
MAX_STREAM = 4 << 20
MAX_DEPTH = 64
MAX_CELLS = 100_000
U64 = (1 << 64)-1
# Closed first-tier raw class key layouts. New optional variants need review and
# focused raw controls; this is deliberately not an evolving permissive decoder.
RAW_LAYOUTS = tuple(tuple(sorted(s.split())) for s in (
    'account balance classification id kind rate risk_weight rsf_weight',
    'account_id accounting_category ledger_classification ownership position_id pre_purchase_accrued pre_purchase_basis pre_purchase_principal source unit_id',
    'accounting accounting_rates credit factor_policy schema source',
    'accounting_category capital_policy_revision graph template_id unit_face unit_id weights',
    'accounts horizon_days policies positions reverse_severities ruleset scenarios version',
    'acquisition current current_accounting_rate_reviews original policy schema transfer',
    'acquisition_releases path_id policy',
    'action amount event_id fee_id ordinal reason review sequence source',
    'adoption_version effective_from_ordinal framework income_policy initial_credit_policy review',
    'allocated_cost fee_id review',
    'allocation as_of_ordinal commercial owners reporting_end_ordinal run_mapping schema specification verified_opening_identity',
    'allocations discount_spread histories identity original_pricing original_signed_fee_quote schema total_purchase_cost',
    'amount fee_id ordinal review sequence',
    'annual_effective_rates interval_index review',
    'annual_rate kind',
    'as_of_ordinal base_scenario_id bump_bp currency identity original_scenario positions reporting_ordinals scenarios schema',
    'asf hqla_l2a outflow30 rsf rwa',
    'asof_ordinal assumption contract currency direction measure model_version monetary_unit observed_publication_cutoff_unix_seconds opening_principal path periods position_id principal_plan reporting_end_ordinal scenario_id schema tail_end_ordinal',
    'asof_ordinal complete_end_ordinal currency events fees measure monetary_unit policy reporting_end_ordinal schema',
    'base_oas kind source_id source_revision',
    'base_scenario_id currency identity reporting_ordinals schema units',
    'by_origin interval_index review',
    'cancel_future_cash cancel_future_service cancel_unearned_entitlement recoveries refund_prepaid_fraction review waive_unpaid_earned_fraction',
    'cash cet1_floor currency entity equity id lcr_floor leverage_floor nsfr_floor',
    'cash_application default_disposition exit_disposition identity recognition unrecognized_cash',
    'classification_review entitlement fee_id position_id prepaid_refunds_allowed service source tranche_id',
    'clock full_checkpoints observation_mapping overnight partial_calendar_day review',
    'coincident_order complete_end_ordinal default events exercise fees maturity policy prepayment schema transfer',
    'coincident_order contract_origin conveyance coupon_allocations economic_ordinal loan_clean_cost recognition_ordinal regime review schema settlement_ordinal source_work_policy transaction_cost transaction_cost_policy',
    'collateral_curve run_id scenario_id severity',
    'collateral_index kind',
    'complete_recovery_tails credit_conversion_factor end_year hazards_per_year interest_recovery_fraction loss_given_default',
    'completed_candidate cut_ordinal revised_candidate schema',
    'conveyance origin_service review valuation_release_policy',
    'cost identity income_review original_annual_effective_rate original_yield_anchor',
    'coupon day_count end_ordinal fee payment_ordinal period_id start_ordinal',
    'credit_identity fees histories oas_target original_pricing original_signed_cash_quote position_id',
    'crn_id grid_revision input_revision model_revision',
    'currency curve_id discount_factors right_tail role source_id source_revision times_years',
    'draw_stream identity instrument_kind intervals max_jumps_per_path opening_undrawn_commitment paths',
    'face unit_id',
    'fees identity recognition reset_income',
    'fraction lag_days',
    'id revision',
    'initial_live_state path_id path_key',
    'input_revision model_revision paired_history_revision paired_history_source randomness_domain',
    'kind principal_payments',
    'kind seed stream_id stream_revision',
    'lag_years weight',
    'ledger_scenario_id severity source_scenario_id',
    'name start_day',
    'path_id policy',
    'path_id source weight',
    'period_id purchased_accrued_cost',
    'physical pricing pricing_down pricing_up scenario_id',
    'kind principal_events',
    'amount event_id payment_ordinal',
))


# Trusted, closed field grammar. Runtime keys walk fixed128-way ASCII edges;
# no hash probe, ordered-key sort, or label-dependent schema extension occurs.
# Static construction below has no caller input; its complete list/node storage
# is still charged to every application ticket before the first field lookup.
_FIELD_NAMES=tuple(sorted({k for names in RAW_LAYOUTS for k in names}|set('path_key year phase policy source_event_id effective_interest basis_realization cash postings next event coupon_id earning_origin_index posting account amount principal recognized_interest legal_interest native_legal_reduction_difference book_adjustment gross_carry in_default reviewed_credit_state lots pending_coupon_obligation_indices canonical_legal_coupons source_id source_revision native_audit_index native_cash_index contractual_earned legal_cash_collected legal_remaining recognized_earned recognized_cash_applied recognized_remaining default_claim opening_entitlement_principal cumulative_factor actual_interest_collected event_id fee_id source review ordinal sequence action reason earned cancelled_unearned waived_earned recovered_waived net_service_cash fee_receivable fee_payable deferred_fee_income prepaid_fee_expense unearned_entitlement acquired_fee_asset_valuation_adjustment acquired_fee_liability_valuation_adjustment transferred_net_stock acquired_cost released_acquired_cost acquired_fee_asset acquired_fee_liability refunded_earned acquired_stock_selected commercial_source_lineage commercial_daily commercial_closing commercial_fee_lineage commercial_fee_daily commercial_fee_closing journal ledger exposures path breaches summary trial_balance closing_statements consolidated attribution'.split())))
_FIELD_ROOT=[None]*129
_FIELD_NODE_COUNT=1
for _number,_name in enumerate(_FIELD_NAMES):
    _node=_FIELD_ROOT
    for _ch in _name:
        _edge=ord(_ch)
        if _node[_edge]is None:
            _node[_edge]=[None]*129;_FIELD_NODE_COUNT+=1
        _node=_node[_edge]
    _node[128]=1<<_number
_FIELD_LIMBS=(len(_FIELD_NAMES)+29)//30
_FIELD_BYTES=_FIELD_NODE_COUNT*1152+sum(4*len(k)+256 for k in _FIELD_NAMES)

def _static_mask(names):
    mask=0
    for name in names:
        node=_FIELD_ROOT
        for ch in name:node=node[ord(ch)]
        mask|=node[128]
    return mask

_RAW_MASKS=tuple(_static_mask(names)for names in RAW_LAYOUTS)


def _add(a, b):
    v = a+b
    if type(a) is not int or type(b) is not int or a < 0 or b < 0 or v > U64:
        raise ArithmeticError('compact census checked integer overflow')
    return v


def _mul(a, b):
    v = a*b
    if type(a) is not int or type(b) is not int or a < 0 or b < 0 or v > U64:
        raise ArithmeticError('compact census checked integer overflow')
    return v


@dataclass
class Ticket:
    """Fixed application component ticket; no caller allowance or native hint."""
    work: int = 0
    retained: int = 0
    peak_retained: int = 0
    strings: list = field(default_factory=list)
    string_trie: object = field(default=None, repr=False)
    field_grammar_charged: bool = field(default=False, repr=False)
    checkpoint: object = field(default=lambda:False,repr=False)
    def debit(self, amount):
        self.checkpoint()
        value = _add(self.work, amount)
        if value > MAX_WORK:
            raise ArithmeticError('independent compact census work exhausted before scan/growth')
        self.work = value
    def grow(self, amount):
        self.checkpoint()
        value = _add(self.retained, amount)
        if value > MAX_BYTES:
            raise ArithmeticError('independent compact census bytes exhausted before growth')
        self.retained = value
        self.peak_retained=max(self.peak_retained,value)
    def release(self, amount):
        if amount > self.retained:
            raise ArithmeticError('independent compact census lifetime imbalance')
        self.retained -= amount


@dataclass(frozen=True)
class Shape:
    encoded_bytes_upper: int = 0
    cells: int = 0
    declared_field_visits: int = 0
    emitted_fields: int = 0
    string_utf8_bytes: int = 0
    def plus(self, other):
        return Shape(*(_add(getattr(self, f.name), getattr(other, f.name)) for f in fields(Shape)))
    def times(self, n):
        return Shape(*(_mul(getattr(self, f.name), n) for f in fields(Shape)))
    def maximum(self, other):
        return Shape(*(max(getattr(self, f.name), getattr(other, f.name)) for f in fields(Shape)))
    def census_work(self):
        return _add(_add(self.cells, self.declared_field_visits), self.string_utf8_bytes)
    def comparison_work(self, other):
        nodes = _add(_add(self.cells,self.declared_field_visits),_add(other.cells,other.declared_field_visits))
        return _add(_mul(4,nodes),_add(self.string_utf8_bytes,other.string_utf8_bytes))


def _text(value, ticket):
    if type(value) is not str:
        raise ArithmeticError('compact census string category')
    # At most four UTF8 bytes/character, paid before reading/encoding characters.
    ticket.debit(_mul(len(value),4))
    width = 2; utf8 = 0
    for ch in value:
        code = ord(ch)
        if 0xD800 <= code <= 0xDFFF:
            raise ArithmeticError('compact census unpaired surrogate')
        n = 1 if code < 128 else 2 if code < 2048 else 3 if code < 65536 else 4
        utf8 += n
        width += 2 if ch in ('"','\\','\n','\r','\t','\b','\f') else 6 if code < 32 else n
    # Fixed-fanout deterministic trie: no expected-case hash/probe assumption.
    # A node is one257-slot list;2304 bounds64-bitCPython list header+slots.
    # The retained string ceiling includes worst-case4-byte Unicode storage.
    ticket.debit(utf8+1);ticket.grow(utf8+64)
    data=value.encode('utf8')
    if ticket.string_trie is None:
        ticket.grow(2304);ticket.string_trie=[None]*257
    node=ticket.string_trie
    for byte in data:
        ticket.debit(3)
        child=node[byte]
        if child is None:
            ticket.grow(2304);child=[None]*257;node[byte]=child
        node=child
    ticket.debit(1)
    if node[256] is None:
        ticket.grow(4*len(value)+288)
        node[256]=len(ticket.strings)
        ticket.strings.append(value)
    ticket.release(utf8+64)

    return width, utf8


def _ordered_items(value,ticket):
    if type(value)is not dict or len(value)>256:
        raise ArithmeticError('compact census bounded dictionary required')
    n=len(value);scratch=64+128*n
    ticket.grow(scratch);ticket.debit(n)
    maximum=0
    for key in value:
        if type(key)is not str:raise ArithmeticError('compact census string key required')
        maximum=max(maximum,len(key))
    # Insertion sort, including shifts, visits at mostn^2 key pairs. Every
    # lexical comparison scans at mostmaximum characters; no dict key lookup.
    ticket.debit(n*n*(maximum+2)+n)
    result=[None]*n
    used=0
    for item in value.items():
        j=used
        while j and result[j-1][0]>item[0]:
            result[j]=result[j-1];j-=1
        result[j]=item;used+=1
    return result,scratch


def _field_bit(key,ticket):
    if type(key)is not str:raise ArithmeticError('compact census string key required')
    ticket.debit(3*len(key)+_FIELD_LIMBS+1)
    node=_FIELD_ROOT
    for ch in key:
        edge=ord(ch)
        if edge>=128 or node[edge]is None:return 0
        node=node[edge]
    return node[128]or 0


def _keys_match(value,layouts,ticket,*,subset=False):
    if type(value)is not dict or len(value)>256:return False
    if not ticket.field_grammar_charged:
        ticket.grow(_FIELD_BYTES);ticket.field_grammar_charged=True
    ticket.debit(len(value)*_FIELD_LIMBS+1)
    mask=0
    for key in value:
        bit=_field_bit(key,ticket)
        if not bit:return False
        mask|=bit
    if layouts is RAW_LAYOUTS:
        ticket.debit(len(_RAW_MASKS)*_FIELD_LIMBS)
        return any(mask==allowed for allowed in _RAW_MASKS)
    for names in layouts:
        allowed=0
        ticket.debit((len(names)+1)*_FIELD_LIMBS)
        for name in names:allowed|=_field_bit(name,ticket)
        if mask==allowed or(subset and mask&~allowed==0):return True
    return False


def shape(value, ticket=None, *, strict_raw=False, depth=0):
    ticket = ticket if ticket is not None else Ticket()
    ticket.debit(1)
    if depth > MAX_DEPTH:
        raise ArithmeticError('compact census nesting exceeds bounded layout')
    if value is None:return Shape(4,1)
    if type(value) is bool:return Shape(5,1)
    if type(value) in (int,float):
        if type(value) is float and not math.isfinite(value):
            raise ArithmeticError('compact census finite scalar required')
        if type(value) is int and not -(1<<63) <= value <= U64:
            raise ArithmeticError('compact census integer outside native JSON domain')
        return Shape(32,1)
    if type(value) is str:
        if strict_raw and len(value)>512:raise ArithmeticError('compact census raw string layout domain')
        width, utf8 = _text(value,ticket)
        return Shape(width,1,0,0,utf8)
    if type(value) is list:
        if len(value)>MAX_CELLS:raise ArithmeticError('compact census bounded array required')
        result=Shape(2+max(len(value)-1,0),1)
        for child in value:result=result.plus(shape(child,ticket,strict_raw=strict_raw,depth=depth+1))
        return result
    if type(value) is dict:
        if len(value)>256:raise ArithmeticError('compact census unsupported raw field layout')
        if strict_raw:
            supported=_keys_match(value,RAW_LAYOUTS,ticket)
            if not supported:raise ArithmeticError('compact census unsupported raw field layout')
        result=Shape(2+max(len(value)-1,0),1,len(value),len(value))
        for key,child in value.items():
            ticket.debit(1)
            width, utf8 = _text(key,ticket)
            result=result.plus(Shape(width+1,0,0,0,utf8)).plus(shape(child,ticket,strict_raw=strict_raw,depth=depth+1))
        return result
    raise ArithmeticError('compact census unsupported scalar/container')


def _bits(value):
    return struct.pack('>d',float(value)).hex()


def _total_order(value):
    bits=int(_bits(value),16)
    return (~bits)&U64 if bits>>63 else bits|(1<<63)


def _day(year):
    x=year*365.;rounded=round(x)
    d=rounded if rounded/365.==year else math.ceil(x)
    return d+1 if year>d/365. else d


def _exact(a,b,ticket):
    ticket.debit(1)
    if type(a) is not type(b):return False
    if type(a) is float:return _bits(a)==_bits(b)
    if isinstance(a,dict):
        if len(a)!=len(b):return False
        left,lb=_ordered_items(a,ticket);right,rb=_ordered_items(b,ticket)
        try:
            for (ka,va),(kb,vb)in zip(left,right,strict=True):
                ticket.debit(4*max(len(ka),len(kb))+1)
                if ka!=kb or not _exact(va,vb,ticket):return False
            return True
        finally:ticket.release(lb+rb)
    if isinstance(a,list):return len(a)==len(b) and all(_exact(x,y,ticket)for x,y in zip(a,b))
    if isinstance(a,str):ticket.debit(4*max(len(a),len(b)))
    return a==b


# Declared struct widths include hidden serde discriminator visits. Financial
# values come solely from independent replay, never a native monetary checkpoint.
_STATE_ZERO = tuple('acquired_fee_asset_valuation_adjustment acquired_fee_liability_valuation_adjustment transferred_net_stock acquired_cost released_acquired_cost acquired_fee_asset acquired_fee_liability refunded_earned'.split())
_STATE_ALWAYS = tuple('earned cancelled_unearned waived_earned recovered_waived net_service_cash fee_receivable fee_payable deferred_fee_income prepaid_fee_expense unearned_entitlement'.split())
_STATE_ZERO_MASK=_static_mask(_STATE_ZERO+('acquired_stock_selected',))


def _fee_state(state,ticket):
    if not _keys_match(state,(tuple(sorted(_STATE_ZERO+_STATE_ALWAYS+('acquired_stock_selected',))),),ticket,subset=True):
        raise ArithmeticError('compact census unmapped fee state')
    ticket.debit(19);ticket.grow(19*128)
    zero_mask=_STATE_ZERO_MASK
    # No temporary hash table or repeated linear string-key membership scans.
    visible=[]
    for k,v in state.items():
        bit=_field_bit(k,ticket)
        if not bit&zero_mask or v!=0.:visible.append((k,shape(v,ticket)))
    result=_object(visible,ticket)
    ticket.release(19*128)
    return result.plus(Shape(0,0,19-len(visible)))


def _snapshot(state,ticket):
    supported=tuple(sorted('path_key year principal recognized_interest legal_interest native_legal_reduction_difference book_adjustment gross_carry in_default reviewed_credit_state lots pending_coupon_obligation_indices canonical_legal_coupons'.split()))
    if not _keys_match(state,(supported,),ticket,subset=True)or state.get('reviewed_credit_state') is not None:
        raise ArithmeticError('compact census unmapped reviewed/loan snapshot')
    lot_fields=tuple(sorted('coupon_id source_event_id source_id source_revision native_audit_index native_cash_index earning_origin_index contractual_earned legal_cash_collected legal_remaining recognized_earned recognized_cash_applied recognized_remaining default_claim'.split()))
    canonical_fields=tuple(sorted('coupon_id native_audit_index native_cash_index opening_entitlement_principal cumulative_factor contractual_earned actual_interest_collected legal_remaining'.split()))
    ticket.debit(14*len(state['lots'])+8*len(state.get('canonical_legal_coupons',[])))
    if any(not _keys_match(lot,(lot_fields,),ticket)for lot in state['lots'])or any(not _keys_match(c,(canonical_fields,),ticket)for c in state.get('canonical_legal_coupons',[])):
        raise ArithmeticError('compact census unmapped lot/canonical fields')
    result=shape(state,ticket)
    # All current optional reviewed/pending/canonical fields are visited even
    # when absent in independent visible serialization.
    return result.plus(Shape(0,0,13-len(state)))


def group_shape(group,kind,ticket):
    if kind=='loan':
        if not _keys_match(group,(tuple(sorted('path_key year phase policy source_event_id effective_interest basis_realization cash postings next'.split())),),ticket):
            raise ArithmeticError('compact census unmapped atomic loan group')
        ticket.debit(5*len(group['postings'])+2)
        if not _keys_match(group['policy'],(('id','revision'),),ticket)or any(not _keys_match(p,(('coupon_id','earning_origin_index','posting'),),ticket)or not _keys_match(p['posting'],(('account','amount'),),ticket)for p in group['postings']):
            raise ArithmeticError('compact census unmapped loan posting/identity')
        _snapshot(group['next'],ticket)
        result=shape(group,ticket)
        # Replace the already counted visible next width with its hidden visits.
        return result.plus(Shape(0,0,13-len(group['next'])))
    if kind=='fee':
        if not _keys_match(group,(('event','next','policy','postings'),),ticket):
            raise ArithmeticError('compact census unmapped atomic fee group')
        ticket.debit(4+2*len(group['postings']))
        if not _keys_match(group['policy'],(('id','revision'),),ticket)or not _keys_match(group['event'],(tuple(sorted('event_id fee_id source review ordinal sequence action reason amount'.split())),),ticket)or any(not _keys_match(p,(('account','amount'),),ticket)for p in group['postings']):
            raise ArithmeticError('compact census unmapped fee event/posting/identity')
        ticket.grow(4*64)
        outer={k:v for k,v in group.items()if k!='next'}
        base=shape(outer,ticket)
        ticket.release(4*64)
        state=_fee_state(group['next'],ticket)
        # Replacing object3 byobject4 adds comma + "next": before state.
        w,n=_text('next',ticket)
        return base.plus(Shape(w+2,0,1,1,n)).plus(state)
    raise ArithmeticError('compact census source kind')


def _source_dimensions(raw,ticket):
    if raw['schema']!='commercial-source-credit-accounting-proof-1':
        raise ArithmeticError('compact census source schema')
    source=raw['source'];current=source['current'];transfer=source.get('transfer')
    if (transfer is None or transfer.get('source_work_policy')!='counted_finite_transfer_contract_factor_work-1'
        or raw.get('performing_draw_tranches') or raw.get('reviewed_credit_actions') is not None
        or raw.get('prepayment_terms') is not None or raw.get('maturity_basis_policy') is not None
        or raw['factor_policy'].get('terminal_obligations') is not None):
        raise ArithmeticError('compact census unsupported finite source ownership')
    contracts=(source['original'],current,transfer['contract_origin'])
    total_periods=total_principal=calendar_span=0;period_squares=0
    for contract in contracts:
        count=len(contract['periods']);hcount=len(contract['principal_plan'].get('principal_events',[]))
        ticket.debit(16*count+8*hcount+16)
        total_periods+=count;total_principal+=hcount;period_squares+=count*count
        calendar_span+=contract['tail_end_ordinal']-min([contract['asof_ordinal']]+[p['start_ordinal']for p in contract['periods']])
        if contract['schema']!='term-enhancements-1' or contract['model_version']!='term-contracts-1':
            raise ArithmeticError('compact census source contract version')
        if contract['principal_plan']['kind'] not in ('supplied','dated_supplied_period_opening_nominal-1'):
            raise ArithmeticError('compact census unsupported principal variant')
        if any(p['coupon']['kind']!='fixed' for p in contract['periods']):
            raise ArithmeticError('compact census indexed source requires complete mapping')
    intervals=raw['credit']['intervals']
    ticket.debit(len(intervals)*(64+3*32))
    if any(len(i['complete_recovery_tails'])!=3 or any(not 1<=len(t)<=32 for t in i['complete_recovery_tails'])for i in intervals):
        raise ArithmeticError('compact census complete1..32tails perorigin required')
    from .commercial_credit_reference import full_end
    days=_day(full_end(raw));p=len(current['periods']);h=len(current['principal_plan'].get('principal_events',[]))
    n=len(raw['credit']['paths']);j=raw['credit']['max_jumps_per_path']
    ticket.debit(16*len(raw['credit']['intervals'])+sum(len(t)for i in raw['credit']['intervals']for t in i['complete_recovery_tails']))
    tails=[len(t)for i in raw['credit']['intervals']for t in i['complete_recovery_tails']]
    if not n==1 or not 0<=days<=1080 or not 0<=j<=4096 or not tails or not 1<=max(tails)<=32:
        raise ArithmeticError('compact census bounded complete source dimensions')
    r=max(tails);i=days+len(raw['credit']['intervals'])+1;b=p+h;s=i+b+j+1;l=4*p
    rows=n*(s+b+j+4+2*r+p+h+l*(1+r)+s*p)
    # Explicit independent reference ticket before any oracle/typed copy. This is
    # an application projection ceiling, not a guessed native source reservation.
    complete_rows=rows+4*(total_periods+total_principal)+period_squares+calendar_span
    reference_work=_mul(_mul(1024,complete_rows),1+total_periods+total_principal+l+r)
    reference_bytes=_mul(_mul(65536,complete_rows),1+total_periods+total_principal)
    return dict(days=days,periods=p,principal_rows=h,paths=n,maximum_jumps=j,maximum_recoveries=r,
                reference_rows_upper=complete_rows,contract_periods=total_periods,contract_principal_rows=total_principal,complete_calendar_span=calendar_span,reference_work_upper=reference_work,reference_bytes_upper=reference_bytes)


TABLES = ('commercial_source_lineage','commercial_daily','commercial_closing',
          'commercial_fee_lineage','commercial_fee_daily','commercial_fee_closing',
          'journal','ledger','exposures','path','breaches','summary','trial_balance',
          'closing_statements','consolidated','attribution')
BASE_GLS = tuple('cash opening_equity oci asset_principal book_adjustment allowance accrued_interest restricted_cash dated_interest_receivable dated_deferred_income credit_default_interest_receivable credit_default_deferred_income memo:commercial_legal_coupon_right memo:commercial_legal_coupon_contra memo:commercial_legal_principal memo:commercial_legal_principal_contra'.split())
FEE_GLS = tuple('commercial_fee_receivable commercial_fee_payable commercial_deferred_fee_income commercial_prepaid_fee_expense commercial_acquired_fee_asset commercial_acquired_fee_liability commercial_acquired_fee_asset_valuation_adjustment commercial_acquired_fee_liability_valuation_adjustment'.split())


def _histories(position,original_scenario):
    for i,h in enumerate(position['original_pricing']):
        yield original_scenario,'pricing',True,h,'original_pricing/'+str(i)
    for bi,bundle in enumerate(position['histories']):
        for role in ('physical','pricing','pricing_down','pricing_up'):
            for hi,h in enumerate(bundle[role]):
                yield bundle['scenario_id'],role,False,h,f'histories/{bi}/{role}/{hi}'


def _normalize(candidate,horizon,ticket):
    from .commercial_reports_reference import normalize_specification
    from ._deposit_continuation_models import Account,Position,Scenario
    raw_width=shape(candidate['specification'],ticket)
    body=65536+raw_width.cells*512+32768*(len(candidate['specification']['positions'])+len(candidate['specification']['accounts'])+len(candidate['specification']['scenarios']))
    ticket.debit(raw_width.census_work()+8192);ticket.grow(2*body)
    raw=deepcopy(candidate['specification']);raw['horizon_days']=horizon
    spec=normalize_specification(raw)
    ticket.release(body)
    for key,cls in (('accounts',Account),('positions',Position),('scenarios',Scenario)):
        for row in spec[key]:
            for field in fields(cls):
                if field.type is float:row[field.name]=float(row[field.name])
    return spec


def _scope(candidate,ticket):
    if candidate['schema']!='commercial-candidate-journal-1' or candidate['commercial']['schema']!='commercial-units-1':
        raise ArithmeticError('compact census candidate/units schema')
    units=candidate['commercial']['units']
    if not 1<=len(units)<=32 or len(candidate['owners'])!=len(units) or len(candidate['allocation'])!=len(units):
        raise ArithmeticError('compact census complete owner/allocation coverage')
    spec=candidate['specification']
    if any(spec.get(k) for k in ('netting_sets','policies','cashflows','reverse_severities')):
        raise ArithmeticError('compact census generic overlay unsupported')
    if any(any(a.get(k,0.)!=0. for k in ('annual_fees','annual_costs','annual_dividends','tax_rate','capital_deductions'))for a in spec['accounts']):
        raise ArithmeticError('compact census account overlay unsupported')
    if any(p['kind']not in ('loan','security')or p['classification']!='ac'for p in spec['positions']):
        raise ArithmeticError('compact census exclusive AC placeholder required')
    for unit,owner,allocation in zip(units,candidate['owners'],candidate['allocation'],strict=True):
        if owner['unit_id']!=unit['unit_id']or allocation['unit_id']!=unit['unit_id']or owner['accounting_category']!='AC'or owner['ledger_classification']!='ac'or owner['ownership']!='exclusive_native_commercial_graph-1':
            raise ArithmeticError('compact census owner/allocation identity')
        if type(allocation['face'])not in(int,float)or not math.isfinite(allocation['face'])or not 0<=allocation['face']<=1e15:
            raise ArithmeticError('compact census allocation domain')
        graph=unit['graph']
        if graph['schema']!='commercial-product-graph-1'or len(graph['positions'])!=1:
            raise ArithmeticError('compact census one finite graph position required')
        position=graph['positions'][0]
        if position.get('exercise')or position.get('generated_policy')or position.get('original_physical')or position['oas_target']['kind']!='locked':
            raise ArithmeticError('compact census optional conditional/original Physical proof unsupported')
        for scenario,role,original,history,suffix in _histories(position,graph['original_scenario']['scenario_id']):
            ticket.debit(1)
            if len(history['source']['credit']['paths'])!=1:
                raise ArithmeticError('compact census one native path per history required')
            if not math.isfinite(history['weight'])or not 0<=history['weight']<=1:
                raise ArithmeticError('compact census source weight domain')
            _source_dimensions(history['source'],ticket)


def _fee_ticket(selection,ticket):
    if selection is None:return 0
    count=0
    for bundle in [dict(pricing=selection['original_pricing']),*selection['histories']]:
        for role in ('physical','pricing','pricing_down','pricing_up'):
            for history in bundle.get(role,[]):
                ticket.debit(1)
                p=history['policy'];f=len(p['fees']);e=len(p['events']);o=len((p.get('transfer')or{}).get('origin_service',{}).get('events',[]));releases=len(history.get('acquisition_releases',[]))
                ticket.debit(e+o+f+releases+1)
                if not 1<=f<=64 or e>512 or o>512:
                    raise ArithmeticError('compact census bounded full fee policy required')
                count=_add(count,_mul(1024,(1+e+o+f+releases)**2))
    return count


def _build_side(candidate,side,cut,ticket,check):
    from .commercial_credit_reference import replay,full_end
    from .commercial_accounting_reference import groups
    from .commercial_source_reference import project_owned
    from .commercial_secondary_reference import loan_transfer
    from .commercial_fee_adoption_reference import project as project_fee,validate as validate_fee
    from .commercial_reference_math import _cash
    use_shape=Shape();uses=[];bindings=[];prefix=[];all_events=[];families={};horizon=max(30,candidate['reporting_end_ordinal']-candidate['as_of_ordinal']);fee_index=0
    mapping={r['source_scenario_id']:r['ledger_scenario_id']for r in candidate['run_mapping']}
    for ui,(unit,owner,allocation)in enumerate(zip(candidate['commercial']['units'],candidate['owners'],candidate['allocation'],strict=True)):
        graph=unit['graph'];position=graph['positions'][0];selection=position.get('fees');origin=graph['as_of_ordinal']-candidate['as_of_ordinal']
        if selection:validate_fee(selection,graph,position,check)
        for scenario,role,original,history,suffix in _histories(position,graph['original_scenario']['scenario_id']):
            check();raw=history['source'];dim=_source_dimensions(raw,ticket)
            ticket.grow(dim['reference_bytes_upper'])
            result=replay(raw,check)
            if role!='physical':
                result['prepared']['raw_credit_path']['measure']='pricing';result['credit']['result']['measure']='pricing'
            proof={}
            for source_role in ('original','current'):
                projection,segments=project_owned(raw['source'],source_role,check,raw['factor_policy'])
                proof['factor_'+source_role]=dict(projection=projection,segments=segments)
            proof['credit_carry']=result;proof['economic_cash']=_cash(result['credit']);proof['secondary_acquisition']=loan_transfer(raw,check)
            if role=='physical':proof['loan_book']=dict(zip(('groups','closing'),groups(result,check),strict=True))
            if selection:proof['fee_source']=project_fee(selection,scenario,role,history,result,None,check,original=original)
            locator=f'/{side}/commercial/units/{ui}/graph/positions/0/{suffix}'
            family_shapes={}
            for name,value in proof.items():
                width=shape(value,ticket);family_shapes[name]=width
                families[name]=families.get(name,Shape()).plus(width)
            body=shape(proof,ticket)
            wrapper=_object([('raw_locator',shape(locator,ticket)),('role',shape(role,ticket)),('scenario',shape(scenario,ticket)),('original',_BOOL),('weight',_NUM),('proof',body)],ticket)
            use_shape=use_shape.plus(wrapper)
            if body.cells>dim['reference_bytes_upper']//512:
                raise ArithmeticError('independent proof exceeded before-growth cell/body reserve')
            ticket.grow(16384);uses.append(dict(raw_locator=locator,role=role,scenario=scenario,original=original,path_id=history['path_id'],weight=float(history['weight']),dimensions=dim,families={k:asdict(v)for k,v in family_shapes.items()}))
            if role=='physical':
                bi=len(bindings);run=mapping[scenario];loan_groups=proof['loan_book']['groups'];fee=proof.get('fee_source',{}).get('bound_path')
                fee_families=selection['allocations']if selection else []
                binding=dict(index=bi,scenario=run,position_id=owner['position_id'],source_class='issuer_ac_security-1'if raw['credit'].get('instrument_kind')=='issuer_credit_debt'else'commercial_loan-1',path_key=raw['credit']['paths'][0]['path_key'],native_position_id=raw['source']['current']['position_id'],source_weight=float(history['weight']),allocated_face=float(allocation['face']),native_unit_face=float(unit['unit_face']),origin_day=origin,loan_groups=loan_groups,fee=fee,fee_index=fee_index if fee else None,fee_count=len(fee_families),raw_locator=locator)
                retained=512*sum(shape(g,ticket).cells for g in loan_groups)
                if fee:retained+=512*shape(fee,ticket).cells
                ticket.grow(retained);binding['application_retained_upper']=retained;bindings.append(binding)
                horizon=max(horizon,origin+_day(full_end(raw)))
                def event(kind,index,gi,year,group):
                    day=origin+_day(year);global_year=origin/365.+year
                    ticket.grow(2048)
                    e=dict(scenario=run,day=day,global_native_year_bits=_bits(global_year),native_year_bits=_bits(year),kind=kind,binding=index,group_index=gi,group=group)
                    all_events.append(e)
                    if day<=cut:prefix.append(e)
                for gi,g in enumerate(loan_groups):event('loan',bi,gi,g['year'],g)
                if fee:
                    horizon=max(horizon,origin+fee['source']['raw_request']['complete_end_ordinal']-graph['as_of_ordinal'])
                    for gi,(year,g)in enumerate(zip(fee['native_years'],fee['source']['groups'],strict=True)):event('fee',fee_index,gi,year,g)
                    fee_index+=1
            ticket.release(dim['reference_bytes_upper'])
    run_order={name:i for i,name in enumerate(['baseline']+[s['name']for s in candidate['specification']['scenarios']])}
    ticket.debit(_mul(max(len(all_events),1),max(len(all_events),1).bit_length()*16))
    def order(e):return run_order[e['scenario']],e['day'],_total_order(struct.unpack('>d',bytes.fromhex(e['global_native_year_bits']))[0]),0 if e['kind']=='loan'else 1,e['binding'],e['group_index']
    all_events.sort(key=order);prefix.sort(key=order)
    specification=_normalize(candidate,horizon,ticket)
    return dict(whole_use_shape=use_shape,uses=uses,bindings=bindings,prefix=prefix,events=all_events,families=families,fee_bindings=fee_index,horizon=horizon,specification=specification)


def _compiled(side,ticket):
    loan_rows=fee_rows=postings=lots=0
    loan_postings=fee_postings=loan_groups=fee_groups=max_lots=max_coupon=0
    two_search=reconcile_search=stock_clones=legal_clones=fee_clones=0
    loan_shape=fee_shape=Shape();all_group_shape=Shape();loan_snapshot=fee_state=Shape()
    for b in side['bindings']:
        bp=sum(len(g['postings'])for g in b['loan_groups']);bl=sum(len(g['next']['lots'])for g in b['loan_groups']);bg=len(b['loan_groups'])
        ticket.debit(bg*2)
        two_search+=2*bp*bp;reconcile_search+=(4*bg+2*bl)*bp;stock_clones+=bg*bp;legal_clones+=bg*bl
        loan_postings+=bp;loan_groups+=bg
        for g in b['loan_groups']:
            max_lots=max(max_lots,len(g['next']['lots']))
            for name in [p['coupon_id']for p in g['postings']]+[l['coupon_id']for l in g['next']['lots']]:
                if name is not None:
                    _,width=_text(name,ticket);max_coupon=max(max_coupon,width)
            ticket.debit(1);postings+=len(g['postings']);lots+=len(g['next']['lots'])
            loan_rows+=len(g['postings'])+2*len(g['next']['lots'])+1
            width=group_shape(g,'loan',ticket);loan_shape=loan_shape.plus(width);all_group_shape=all_group_shape.plus(width);loan_snapshot=loan_snapshot.maximum(_snapshot(g['next'],ticket))
        if b['fee']:
            fee_groups+=len(b['fee']['source']['groups']);fee_clones+=13*b['fee_count']*len(b['fee']['source']['groups'])
            for g in b['fee']['source']['groups']:
                fee_postings+=len(g['postings'])
                ticket.debit(1);postings+=len(g['postings']);fee_rows+=len(g['postings'])+2
                width=group_shape(g,'fee',ticket);fee_shape=fee_shape.plus(width);all_group_shape=all_group_shape.plus(width);fee_state=fee_state.maximum(_fee_state(g['next'],ticket))
    work=(loan_rows+fee_rows)*(side['horizon']+1)*16
    return dict(loan_stage_rows=loan_rows,fee_stage_rows=fee_rows,work_upper=work,
        retained_component_formula_bytes=(loan_rows+fee_rows)*4096,loan_bindings=len(side['bindings']),fee_bindings=side['fee_bindings'],
        postings=postings,lot_visits=lots,stock_key_upper=loan_postings,legal_key_upper=lots,
        loan_groups=loan_groups,fee_groups=fee_groups,loan_postings=loan_postings,fee_postings=fee_postings,maximum_lots_per_group=max_lots,maximum_coupon_key_bytes=max_coupon,
        loan_two_stock_search_steps_upper=two_search,loan_reconcile_stock_search_steps_upper=reconcile_search,
        loan_stock_clone_entries_upper=stock_clones,loan_legal_clone_entries_upper=legal_clones,fee_stock_clone_entries_upper=fee_clones,
        fee_stock_key_upper=13*sum(b['fee_count']for b in side['bindings']),
        fee_native_year_shape=Shape(32*fee_groups,fee_groups),
        all_loan_groups_shape=loan_shape,all_fee_groups_shape=fee_shape,all_groups_shape=all_group_shape,
        maximum_loan_snapshot=loan_snapshot,maximum_fee_state=fee_state)


_NUM = Shape(32,1)
_BOOL = Shape(5,1)

def _object(parts,ticket):
    # The small template descriptor has a fixed before-growth envelope.
    ticket.grow(256+len(parts)*128);ticket.debit(len(parts))
    result=Shape(2+max(len(parts)-1,0),1,len(parts),len(parts))
    for key,value in parts:
        width,n=_text(key,ticket)
        result=result.plus(Shape(width+1,0,0,0,n)).plus(value)
    ticket.release(256+len(parts)*128)
    return result


def _labels(spec,side,ticket):
    from .commercial_ledger_reference import GL as LOAN_GL
    from .commercial_fee_reference import GL as FEE_GL
    categories={k:Shape()for k in ('scenario','account','entity','currency','position','phase','gl','fee_id')}
    def put(key,text):categories[key]=categories[key].maximum(shape(text,ticket))
    put('scenario','baseline');put('position','')
    for row in spec['scenarios']:put('scenario',row['name'])
    for row in spec['accounts']:
        for key,name in (('account','id'),('entity','entity'),('currency','currency')):put(key,row[name])
    for row in spec['positions']:put('position',row['id'])
    for name in BASE_GLS+(FEE_GLS if side['fee_bindings']else()):put('gl',name)
    for name in ('opening','native_verified_cut_opening','commercial_nonintegral_fee'):put('phase',name)
    for b in side['bindings']:
        for g in b['loan_groups']:
            put('phase',g['phase'])
            for p in g['postings']:
                code=p['posting']['account']
                if code in ('live_accrued_interest','default_accrued_interest'):
                    pre='credit_default_'if code.startswith('default')else'dated_'
                    for name in ('interest_receivable','deferred_income'):put('gl',pre+name)
                else:put('gl',LOAN_GL[code])
        if b['fee']:
            for g in b['fee']['source']['groups']:
                put('fee_id',g['event']['fee_id'])
                for p in g['postings']:put('gl',FEE_GL[p['account']])
    return categories


def _table_inventory(side,compiled,days,finish,opening_keys,ticket):
    spec=side['specification'];labels=_labels(spec,side,ticket);tables={}
    def row(strings,numbers='',children=()):
        return _object([(k,labels[v]if v in labels else shape(v,ticket))for k,v in strings]+[(k,_NUM)for k in numbers.split()]+list(children),ticket)
    def record(name,count,template):
        ticket.debit(1);ticket.grow(4096)
        tables[name]=dict(rows_upper=count,shape_upper=asdict(template.times(count)),row_shape=asdict(template))
    scenarios=1+len(spec['scenarios']);accounts=len(spec['accounts']);positions=len(spec['positions']);group_count=len(side['events'])
    loan_groups=sum(len(b['loan_groups'])for b in side['bindings']);fee_groups=group_count-loan_groups
    line_upper=sum(2*len(g['postings'])+2*len(g['next']['lots'])+2*int(g['next'].get('reviewed_credit_state')is not None)for b in side['bindings']for g in b['loan_groups'])
    line_upper+=sum(len(g['postings'])for b in side['bindings']if b['fee']for g in b['fee']['source']['groups'])
    key_upper=(3*accounts+4*positions)*scenarios+line_upper
    # Full all-group structural maxima bound every native daily row. No weight,
    # allocation, net-zero cash or realized-hazard pruning enters these counts.
    loan_atom=Shape();fee_atom=Shape()
    for b in side['bindings']:
        for g in b['loan_groups']:loan_atom=loan_atom.maximum(group_shape(g,'loan',ticket))
        if b['fee']:
            for g in b['fee']['source']['groups']:fee_atom=fee_atom.maximum(group_shape(g,'fee',ticket))
    record('commercial_source_lineage',loan_groups,row([('scenario','scenario'),('position_id','position'),('source_class','issuer_ac_security-1'),('date_policy','actual_credit_clock_order_day_end_journal_no_retiming_source-1')],
        'day native_credit_year source_origin_day global_native_credit_year path_key source_weight allocation_factor native_group_index transaction_number',[('native_atomic_group',loan_atom)]))
    loan_row=row([('scenario','scenario'),('position_id','position'),('source_class','issuer_ac_security-1')],'day path_key source_weight allocation_factor source_origin_day',[('unweighted_book',compiled['maximum_loan_snapshot']),('capital_ftp_eligible',_BOOL)])
    record('commercial_daily',days*len(side['bindings']),loan_row)
    record('commercial_closing',len(side['bindings'])if finish else 0,loan_row)
    record('commercial_fee_lineage',fee_groups,row([('scenario','scenario'),('position_id','position')],'day path_key native_year source_weight allocation_factor native_group_index transaction_number',[('native_fee_group',fee_atom)]))
    fee_row=row([('scenario','scenario'),('position_id','position'),('fee_id','fee_id')],'day path_key source_weight allocation_factor',[('native_fee_snapshot',compiled['maximum_fee_state'])])
    fee_count=sum(b['fee_count']for b in side['bindings'])
    record('commercial_fee_daily',days*fee_count,fee_row);record('commercial_fee_closing',fee_count if finish else 0,fee_row)
    record('journal',line_upper+opening_keys,row([('scenario','scenario'),('account','account'),('event','phase'),('gl_account','gl'),('instrument_id','position')],'transaction_number day value'))
    record('ledger',group_count,row([('scenario','scenario'),('account','account'),('event','phase')],'day cash earnings aoci memo_amount'))
    record('exposures',days*positions*scenarios,row([('scenario','scenario'),('account','account'),('position','position'),('kind','security')],'day balance allowance watch_fraction commitment market_factor encumbered_fraction accrued_interest'))
    record('path',days*accounts*scenarios,row([('scenario','scenario'),('account','account'),('entity','entity'),('currency','currency'),('lcr_status','not_applicable'),('nsfr_status','not_applicable')],
        'day cash restricted_cash cash_floor assets liabilities equity aoci cet1 rwa cet1_ratio leverage_ratio usable_collateral undrawn hqla_proxy lcr_proxy nsfr_proxy reconciliation_error cash_headroom cet1_headroom leverage_headroom lcr_headroom nsfr_headroom htm_headroom'))
    record('breaches',6*accounts*scenarios,row([('scenario','scenario'),('account','account'),('metric','leverage')],'day'))
    record('summary',accounts*scenarios if finish else 0,row([('scenario','scenario'),('account','account'),('entity','entity'),('currency','currency'),('lcr_applicability','not_applicable'),('lcr_status','not_applicable'),('nsfr_applicability','not_applicable'),('nsfr_status','not_applicable')],
        'final_cash final_equity minimum_cash peak_cash_shortfall max_reconciliation_error first_cash_breach_day first_capital_breach_day first_leverage_breach_day first_lcr_breach_day first_nsfr_breach_day first_htm_breach_day htm_reclassification_day'))
    record('trial_balance',key_upper if finish else 0,row([('scenario','scenario'),('account','account'),('gl_account','gl'),('instrument_id','position')],'balance'))
    record('closing_statements',accounts*scenarios if finish else 0,row([('scenario','scenario'),('account','account'),('currency','currency')],
        'assets liabilities equity cash restricted_cash intercompany_assets intercompany_liabilities reconciliation_error'))
    record('consolidated',accounts*scenarios if finish else 0,row([('scenario','scenario'),('currency','currency')],'assets liabilities equity cash restricted_cash'))
    record('attribution',group_count*len(spec['scenarios'])if finish else 0,row([('scenario','scenario'),('account','account'),('event','phase')],'cash_delta earnings_delta aoci_delta'))
    if not _keys_match(tables,(tuple(sorted(TABLES)),),ticket):raise ArithmeticError('compact census complete16table coverage')
    return tables,key_upper


def _prefix_inventory(old,new,cut,ticket):
    if len(old['prefix'])!=len(new['prefix']):raise ArithmeticError('compact census changed prefix group coverage')
    # These schedules are a static audited64bit enum ceiling, not a producer
    # capacity hint. Native has a size/layout guard before using its comparator.
    groups=len(old['events'])+len(new['events']);runs=1+len(old['specification']['scenarios'])
    ordering=groups*groups*32*runs*(cut+1)
    # Actual prefix calendar may contain a day with no source events. Use the
    # requested cut supplied by the caller, never infer it from realized groups.
    left=right=Shape();comparison=0
    for a,b in zip(old['prefix'],new['prefix'],strict=True):
        ticket.debit(1)
        keys=('scenario','day','global_native_year_bits','native_year_bits','kind','binding','group_index')
        if any(a[k]!=b[k]for k in keys):raise ArithmeticError('compact census exact ordered prefix clock/owner differs')
        sa=group_shape(a['group'],a['kind'],ticket);sb=group_shape(b['group'],b['kind'],ticket)
        ticket.debit(sa.comparison_work(sb))
        if not _exact(a['group'],b['group'],ticket):raise ArithmeticError('compact census exact gross unweighted prefix differs')
        comparison+=sa.comparison_work(sb);left=left.plus(sa);right=right.plus(sb)
    binding_comparison=0;binding_pair=future_binding=Shape()
    names=('source_class','scenario','position_id','path_key','origin_day','source_weight','allocated_face','native_unit_face')
    def identity(v):
        ticket.grow(2048);ticket.debit(len(names))
        return {k:v[k]for k in names}
    for a,b in zip(old['bindings'],new['bindings']):
        va=identity(a);vb=identity(b);sa=shape(va,ticket);sb=shape(vb,ticket)
        ticket.debit(sa.comparison_work(sb))
        if not _exact(va,vb,ticket):raise ArithmeticError('compact census surviving binding differs')
        binding_pair=binding_pair.plus(sa).plus(sb);binding_comparison+=sa.comparison_work(sb)
        ticket.release(4096)
        if a['fee']:
            ticket.grow(1024)
            va=dict(loan_binding=a['index'],native_position_id=a['native_position_id']);vb=dict(loan_binding=b['index'],native_position_id=b['native_position_id'])
            sa=shape(va,ticket);sb=shape(vb,ticket);ticket.debit(sa.comparison_work(sb))
            if not _exact(va,vb,ticket):raise ArithmeticError('compact census fee-parent binding differs')
            binding_pair=binding_pair.plus(sa).plus(sb);binding_comparison+=sa.comparison_work(sb)
            ticket.release(1024)
    for b in new['bindings'][len(old['bindings']):]:
        if b['origin_day']<=cut:raise ArithmeticError('compact census appended binding is not future-only')
        value=identity(b);future_binding=future_binding.plus(shape(value,ticket));ticket.release(2048)
        if b['fee']:
            ticket.grow(512)
            future_binding=future_binding.plus(shape(dict(loan_binding=b['index'],native_position_id=b['native_position_id']),ticket))
            ticket.release(512)
    scenarios=['baseline']+[s['name']for s in old['specification']['scenarios']]
    ticket.debit(len(scenarios)+len(old['bindings'])+len(new['bindings']))
    maximum_scenario=max(len(s.encode('utf8'))for s in scenarios+[b['scenario']for b in old['bindings']+new['bindings']])
    accessor_strings=2*(24+2*maximum_scenario)
    accessor_lookup=4*(groups+1)*(2*maximum_scenario+2)*runs*(cut+1)
    clone_bytes=4*sum(len(s.encode('utf8'))for s in scenarios)*(cut+1)
    ticket.grow(4096*len(old['prefix']))
    return dict(ordering_comparison_steps_upper=ordering,scheduled_layout_bytes_upper=24,ordered_event_workspace_bytes_upper=groups*6*24+2*24+accessor_strings,
        accessor_lookup_work_upper=accessor_lookup,scenario_clone_bytes_upper=clone_bytes,accessor_string_workspace_bytes_upper=accessor_strings,
        binding_identity_pair_shape=asdict(binding_pair),future_binding_identity_shape=asdict(future_binding),
        binding_comparison_work_upper=binding_comparison,fee_clock_comparison_work_upper=8*sum(e['kind']=='fee'for e in old['prefix']),
        immutable_specification_typed_comparison_complete=False,complete_sim_bound_available=False,
        events=len(old['prefix']),loans=sum(e['kind']=='loan'for e in old['prefix']),fees=sum(e['kind']=='fee'for e in old['prefix']),
        comparison_work_upper=comparison,left_shape=asdict(left),right_shape=asdict(right),
        ordered_keys=[{k:e[k]for k in ('scenario','day','global_native_year_bits','native_year_bits','kind','binding','group_index')}for e in old['prefix']])


UNPROVED = ('original_q_native_live_capture_not_retained_in_final_graph',
            'complete_native_source_work_and_retained_wrapper_aliases',
            'all_native_projection_wrapper_fields_and_unit_funding_coefficients',
            'supported_native_allocator_layout_and_all_live_capacities',
            'complete_sim_stock_search_and_reconciliation_kernel_cost',
            'actual_compact_codec_frame_offsets_headers_completion_and_interning',
            'raw_independent_persisted_stream_and_final_publication_fences')


def _census(raw,check=lambda:False):
    """Compute bounded independent dimensions; never grant compact execution.

    Only raw JSON is an input. No producer admission/size scalar is accepted.
    Unsupported variants and app-ticket exhaustion raise before projection;
    supported sources produce an explicitly incomplete/refused census receipt.
    """
    ticket=Ticket(checkpoint=check);check()
    if type(raw)is not dict or not _keys_match(raw,(('completed_candidate','cut_ordinal','revised_candidate','schema'),),ticket)or raw['schema']!='commercial-cut-mixed-journal-1':
        raise ArithmeticError('compact census exact raw root required')
    raw_shape=shape(raw,ticket,strict_raw=True)
    if raw_shape.encoded_bytes_upper>MAX_RAW or raw_shape.cells>MAX_CELLS:
        raise ArithmeticError('compact census raw envelope exceeds2MiB/100000cells')
    # Caller owns raw. Account its logical tree and a separately bounded raw-prefix
    # deep-copy phase; these application limits are not native source authority.
    ticket.grow(_mul(raw_shape.cells,512));ticket.debit(raw_shape.census_work())
    for key in ('completed_candidate','revised_candidate'):_scope(raw[key],ticket)
    origin=raw['completed_candidate']['as_of_ordinal'];cut=raw['cut_ordinal']-origin
    if type(raw['cut_ordinal'])is not int or cut<=0:raise ArithmeticError('compact census strict positive integer cut')
    reserve=0;reference_owners=0
    for side in ('completed_candidate','revised_candidate'):
        for unit in raw[side]['commercial']['units']:
            graph=unit['graph'];p=graph['positions'][0]
            reserve=_add(reserve,_fee_ticket(p.get('fees'),ticket))
            for _,_,_,h,_ in _histories(p,graph['original_scenario']['scenario_id']):
                reference_owners+=1;reserve=_add(reserve,_source_dimensions(h['source'],ticket)['reference_work_upper'])
    ticket.debit(reserve)
    ticket.grow(_mul(raw_shape.cells,2048));ticket.debit(_mul(raw_shape.census_work(),8))
    from .commercial_cut_mixed_reference import raw_prefix
    raw_prefix(raw,check)
    ticket.release(_mul(raw_shape.cells,2048))
    ticket.grow(65536)
    old=_build_side(raw['completed_candidate'],'completed_candidate',cut,ticket,check)
    new=_build_side(raw['revised_candidate'],'revised_candidate',cut,ticket,check)
    if old['horizon']!=new['horizon']or not cut<old['horizon']:
        raise ArithmeticError('compact census same complete horizon and interior cut required')
    prefix=_prefix_inventory(old,new,cut,ticket);a=_compiled(old,ticket);b=_compiled(new,ticket)
    initial=(3*len(old['specification']['accounts'])+4*len(old['specification']['positions']))*(1+len(old['specification']['scenarios']))
    p_tables,oldkeys=_table_inventory(old,a,cut+1,False,initial,ticket)
    v_tables,newkeys=_table_inventory(new,b,new['horizon']-cut+2,True,oldkeys,ticket)
    rows=sum(t['rows_upper']for t in p_tables.values())+sum(t['rows_upper']for t in v_tables.values())
    # Every forced flush can carry each of16tableheaders; completion separate.
    runs=1+len(new['specification']['scenarios']);flushes=rows//16384+2*runs+1
    proof=Shape(2+max(reference_owners-1,0),1).plus(old['whole_use_shape']).plus(new['whole_use_shape'])
    dictionary_bytes=0
    for text in ticket.strings:
        ticket.debit(4*len(text)+1)
        dictionary_bytes+=6*len(text)+3
    result=dict(schema=SCHEMA,status='refused_incomplete_inventory',publication_allowed=False,
        native_receipts_read=False,producer_admission_hints_read=False,raw_source_replayed=True,
        origin_asof_ordinal=origin,cut_day=cut,full_end_ordinal=origin+new['horizon'],
        source_histories=reference_owners,
        source_coverage=dict(expected_uses=reference_owners,original_q_live_uses=sum(u['original']for side in (old,new)for u in side['uses']),
            expected_current_native_source_uses=sum(not u['original']for side in (old,new)for u in side['uses']),original_q_native_capture_verified=False),
        source_uses={k:v['uses']for k,v in (('completed_candidate',old),('revised_candidate',new))},
        compiled={k:{n:asdict(x)if isinstance(x,Shape)else x for n,x in v.items()}for k,v in (('completed_candidate',a),('revised_candidate',b))},
        prefix=prefix,output16=dict(prefix=p_tables,visible=v_tables,all_rows_upper=rows,all_flushes_upper=flushes,per_table_headers_upper=16*flushes),
        dictionary=dict(entries_upper=len(ticket.strings),escaped_bytes_upper=dictionary_bytes,zero_gross_fee_labels=list(FEE_GLS)if b['fee_bindings']else[],old_sink_to_global_index_mapping_required=True),
        known_source_proof_shape=asdict(proof),known_four_serialization_shaped_passes_upper=4*(proof.encoded_bytes_upper+proof.cells),
        source_work_upper=None,native_root_work_upper=None,native_retained_bytes_upper=None,compact_stream_bytes_upper=None,
        application_reference_work_reservation=reserve,application_work_spent=ticket.work,application_retained_upper=ticket.peak_retained,application_final_retained_upper=ticket.retained,
        caps=dict(root_work=MAX_WORK,root_bytes=MAX_BYTES,cut_work=MAX_COMPONENT_WORK,cut_bytes=MAX_COMPONENT_BYTES,complete_stream_bytes=MAX_STREAM),
        unproved_gates=list(UNPROVED))
    return result


def require_complete(receipt):
    """Phase one cannot be promoted to publication by editing reported counters."""
    raise ArithmeticError('compact-cut phase1 lacks complete raw-derived native admission; publication refused')


def census(raw,check=lambda:False):
    try:
        return _census(raw,check)
    except (KeyError,TypeError,ValueError,StopIteration,OverflowError) as exc:
        raise ArithmeticError('compact census unsupported/invalid raw layout: '+str(exc))from exc
