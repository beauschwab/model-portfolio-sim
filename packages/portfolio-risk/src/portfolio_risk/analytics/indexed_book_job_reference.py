"""Independent complete fully-bound book publication oracle; never execution."""
from copy import deepcopy
import hashlib
import json
import math
from . import market_indexed_reference as ref
from .market_indexed_graph_reference import verify as verify_graph

SCHEMA = 'market-indexed-books-1'
CACHE_BYTES, CACHE_ENTRIES = 8 * 1024 * 1024, 1024


def _fields(value, required, optional=''):
    if type(value) is not dict or not set(required.split()) <= set(value) <= set((required+' '+optional).split()):
        raise ValueError('unknown or missing fully-bound raw field')


def _row(raw):
    required = 'maturity freq_months daycount coupon notional price'
    defaults = dict(is_float=False,cap=10.,floor=-10.,call_threshold=.005,
        amort_type='bullet',sink_schedule=None,call_schedule=None,put_schedule=None,
        channel='',penalty_months=0.,ew_mult=1.)
    _fields(raw,required,' '.join(defaults))
    row = defaults | deepcopy(raw)
    if type(row['is_float']) is not bool or row['amort_type'] != 'bullet' or row['daycount'] not in ['ACT/360','ACT/365F']:
        raise ValueError('explicit floating bullet/day count required')
    ref._int(row['maturity'],1,3652059);ref._int(row['freq_months'],1,1200)
    for key in ['coupon','notional','price','cap','floor','call_threshold','penalty_months','ew_mult']:
        row[key] = ref._float(row[key],-1e12,1e12)
    if row['notional'] <= 0 or row['price'] <= 0 or not row['is_float'] or row['cap'] != 10. or row['floor'] != -10. or row['penalty_months'] != 0. or row['ew_mult'] != 1. or row['channel'] != '' or any(row[k] is not None for k in ['call_schedule','put_schedule','sink_schedule']):
        raise ValueError('unsupported complete indexed row')
    return row


def admit(raw, check=lambda:False):
    ref._check(check)
    r = deepcopy(raw)
    _fields(r,'schema identity balance_sheet_extras legacy bindings')
    if r['schema'] != SCHEMA or r['balance_sheet_extras'] not in [None,{}]:
        raise ValueError('fully-bound pricing-only schema/extras')
    _fields(r['identity'],'source_id source_revision input_revision model_version currency as_of_ordinal')
    for key in ['source_id','source_revision','input_revision','currency']:
        ref._text(r['identity'][key],256)
    if r['identity']['model_version'] != SCHEMA:
        raise ValueError('book source version')
    ref._int(r['identity']['as_of_ordinal'],1,3652059)
    legacy = r['legacy']
    _fields(legacy,'base current spread_shift overrides include_analytics include_key_rates kpis auxiliary')
    legacy['spread_shift'] = ref._float(legacy['spread_shift'],0,0)
    if legacy['include_analytics'] is not False or legacy['include_key_rates'] is not False or legacy['kpis'] is not None or legacy['auxiliary'] is not None:
        raise ValueError('fully-bound price-only request')
    if type(legacy['overrides']) is not dict or len(legacy['overrides']) > 2:
        raise ValueError('bounded zero spread overrides')
    for book, rows in legacy['overrides'].items():
        if book not in ['loans','debt'] or type(rows) is not dict or len(rows)>64:
            raise ValueError('bounded spread override ownership')
        for ident,value in rows.items():
            ref._text(ident,256);rows[ident]=ref._float(value,0,0)
    orders = []
    for side in ['base','current']:
        ref._check(check)
        a = legacy[side]
        _fields(a,'market asof horizon mbs terms deposits mm hedges withdrawal_parameters anchors deposit_initial_rate forecast draws capture_anchor capture_cashflows')
        if any(a[k] is not None for k in ['mbs','deposits','mm','hedges','forecast','draws','deposit_initial_rate']) or a['anchors'] != {} or a['capture_anchor'] is not False or a['capture_cashflows'] is not False:
            raise ValueError('unsupported financial owner in fully-bound request')
        ref._int(a['asof'],1,3652059);ref._int(a['horizon'],1,1080)
        if a['asof'] != r['identity']['as_of_ordinal']:
            raise ValueError('book asof binding')
        if type(a['withdrawal_parameters']) is not list or len(a['withdrawal_parameters'])>5:
            raise ValueError('bounded withdrawal metadata')
        a['withdrawal_parameters']=[ref._float(x,-1e12,1e12) for x in a['withdrawal_parameters']]
        market=a['market'];_fields(market,'config tenors swap_rates vol_quotes seed')
        cfg=market['config'];_fields(cfg,'paths months forwards factors dt tenor shift curve_bump vol_bump')
        for field,maximum in [('paths',8),('months',360),('forwards',512),('factors',4)]:ref._int(cfg[field],1,maximum)
        for field in ['dt','tenor','shift','curve_bump','vol_bump']:cfg[field]=ref._float(cfg[field],-1e12,1e12)
        if cfg['dt']!=1/12 or cfg['tenor']!=.25 or cfg['curve_bump']!=.0001 or cfg['vol_bump']!=.01:
            raise ValueError('unchanged native grid conventions required')
        for field,maximum in [('tenors',16),('swap_rates',16),('vol_quotes',48)]:
            if type(market[field]) is not list or not 1<=len(market[field])<=maximum:raise ValueError('bounded market transport')
            market[field]=[ref._float(x,-1e12,1e12) for x in market[field]]
        if len(market['tenors'])!=len(market['swap_rates']) or len(market['vol_quotes'])%3 or any(x<=0 for x in market['tenors']) or any(b<=a for a,b in zip(market['tenors'],market['tenors'][1:])):
            raise ValueError('native market transport shapes')
        if type(market['seed']) is not list or not 1<=len(market['seed'])<=32:raise ValueError('bounded original CRN seed')
        for value in market['seed']:ref._int(value,0,2**32-1)
        if type(a['terms']) is not list or not 1<=len(a['terms'])<=2:raise ValueError('bounded book containers')
        order=[];seen=set()
        for book in a['terms']:
            _fields(book,'key ids book_yields deck')
            if book['key'] not in ['loans','debt'] or book['key'] in seen or book['book_yields'] is not None:raise ValueError('unique supported book containers')
            seen.add(book['key']);deck=book['deck'];_fields(deck,'product asof months calendar contracts','bdc')
            if deck['product']!='corporate' or deck['asof']!=a['asof'] or deck['months']!=cfg['months']:raise ValueError('bound raw corporate grid')
            ref._int(deck['months'],2,360)
            deck.setdefault('bdc','MF')
            if deck['bdc'] not in ['F','MF','P','NONE']:raise ValueError('BDC must be enum string')
            _fields(deck['calendar'],'name','extra_holidays');ref._text(deck['calendar']['name'],256)
            deck['calendar'].setdefault('extra_holidays',[])
            if type(deck['calendar']['extra_holidays']) is not list or len(deck['calendar']['extra_holidays'])>4096:raise ValueError('bounded calendar')
            for day in deck['calendar']['extra_holidays']:ref._int(day,1,3652059)
            if type(book['ids']) is not list or type(deck['contracts']) is not list or len(book['ids'])!=len(deck['contracts']) or len(book['ids'])>16:raise ValueError('complete row arrays')
            if book['key']=='debt' and book['ids']:raise ValueError('nonempty debt has no supported indexed owner')
            for ident in book['ids']:ref._text(ident,256)
            deck['contracts']=[_row(x) for x in deck['contracts']]
            order.extend((book['key'],ident) for ident in book['ids'])
        orders.append(order)
    if orders[0]!=orders[1] or len(orders[0])!=len(set(orders[0])):raise ValueError('complete ordered original/current IDs')
    base,current=legacy['base'],legacy['current']
    if base['horizon']!=current['horizon'] or base['market']['config']!=current['market']['config'] or base['market']['seed']!=current['market']['seed']:raise ValueError('original/current grids and shared draws')
    if type(r['bindings']) is not list or not 1<=len(r['bindings'])<=16:raise ValueError('bounded bindings')
    if [(b.get('book'),b.get('position_id')) for b in r['bindings']]!=orders[0]:raise ValueError('all rows must be explicitly bound in source order')
    for binding in r['bindings']:
        ref._check(check)
        _fields(binding,'book position_id original_row current_row quote_review graph')
        _fields(binding['quote_review'],'id revision policy')
        for field in ['id','revision']:ref._text(binding['quote_review'][field],256)
        if binding['quote_review']['policy']!='original_native_pv_quote_reconciliation-1':raise ValueError('explicit original quote review')
        for field in ['original_row','current_row']:binding[field]=_row(binding[field])
        _fields(binding['graph'],'schema product source')
        if binding['graph']['schema']!='market-indexed-graph-1' or binding['graph']['product']!='indexed_loan':raise ValueError('bound indexed graph version')
        binding['graph']['source']=ref.admit(binding['graph']['source'],check)[0]
        graph=binding['graph']['source'];bridge=graph['bridge'];contract=bridge['contract']
        for side,field in [('base','original_row'),('current','current_row')]:
            book=next(b for b in legacy[side]['terms'] if b['key']==binding['book'])
            row=book['deck']['contracts'][book['ids'].index(binding['position_id'])]
            ref._source_equal(row,binding[field],check,'complete_'+side+'_row')
        original=binding['original_row'];current=deepcopy(binding['current_row']);current['price']=original['price']
        ref._source_equal(current,original,check,'original_terms_except_displayed_quote')
        if binding['book']!='loans' or contract['contract_id']!=binding['position_id'] or contract['maturity_ordinal']!=original['maturity'] or contract['notional']!=original['notional'] or contract['margin']!=original['coupon'] or graph['index_tenor_months']!=original['freq_months'] or bridge['identity']['currency']!=r['identity']['currency'] or bridge['identity']['as_of_ordinal']!=base['asof'] or original['daycount']!=('ACT/360' if contract['index_day_count']=='actual360' else 'ACT/365F'):
            raise ValueError('complete quote/amount/index/date/currency binding')
    return r


def verify(raw, result, check=lambda:False):
    norm=admit(raw,check);ref._check(check)
    _fields(result,'positions totals scope_net_value graph cache graph_scope market_indexed')
    meta=result['market_indexed']
    _fields(meta,'schema original_request current_request sources admitted_work admitted_bytes pricing_only production_adoption saved_library_adoption physical_accounting joint_dynamics observed_fixings calendar_mapping_verified empirical_validation accounting_adoption capital_ftp_eligible')
    for field in ['original_request','current_request']:ref._source_equal(meta[field],norm,check,field)
    ref._source_equal(meta['schema'],SCHEMA,check,'schema')
    for field in ['production_adoption','saved_library_adoption','physical_accounting','joint_dynamics','observed_fixings','calendar_mapping_verified','empirical_validation','accounting_adoption','capital_ftp_eligible']:ref._source_equal(meta[field],False,check,field)
    ref._source_equal(meta['pricing_only'],True,check,'pricing_only')
    ref._source_equal(result['graph_scope'],'legacy_unbound_only; explicit indexed counts retained per native source',check,'graph_scope')
    cache=result['cache'];_fields(cache,'entries bytes max_bytes max_entries shared_entries shared_bytes shared_max_bytes shared_max_entries compact_entries transient_entries identity_entries')
    for key,value in cache.items():ref._int(value,0,128*1024*1024)
    if cache['max_bytes']!=CACHE_BYTES or cache['max_entries']!=CACHE_ENTRIES or cache['bytes']>CACHE_BYTES or cache['entries']>CACHE_ENTRIES or cache['compact_entries']+cache['transient_entries']!=cache['entries'] or any(cache[k]!=0 for k in ['shared_entries','shared_bytes','shared_max_bytes','shared_max_entries']):raise ValueError('bounded single shared cache receipt')
    if type(meta['sources']) is not list or len(meta['sources'])!=len(norm['bindings']):raise ValueError('complete native source count')
    size=ref._size(norm)
    if size>2*1024*1024:raise ValueError('raw source admission')
    cells=max(a['market']['config']['paths']*a['market']['config']['months']*a['market']['config']['forwards'] for a in [norm['legacy']['base'],norm['legacy']['current']])
    rows=len(norm['bindings']);legacy_work=cells*max(rows,1)*128
    joined_bytes=32*size+CACHE_BYTES+4*legacy_work+262144;work=legacy_work+64*size
    bound={}
    for binding,source in zip(norm['bindings'],meta['sources']):
        ref._check(check)
        _fields(source,'schema original_anchor current_request cases risk counts cache admitted_joined_bytes admitted_work pricing_only pairing_policy stochastic_crn physical_accounting production_adoption saved_library_adoption capital_ftp_eligible')
        if type(source['cases']) is not list or len(source['cases'])!=8:raise ValueError('complete paired source cases')
        for case in source['cases']:
            _fields(case,'role source indexed_request indexed_result swap_request swap_result original_oas spread_adjusted_swap_pv indexed_pv selected_pv')
        _fields(source['counts'],'indexed-cash indexed-loan-pv projection swap-clean swap-original-oas')
        for stage,counts in source['counts'].items():
            _fields(counts,'computed reused')
            for value in counts.values():ref._int(value,0,8)
            if counts['computed']+counts['reused']!=(1 if stage=='projection' else 8):raise ValueError('complete source node count receipt')
        _fields(source['cache'],'entries bytes max_bytes max_entries shared_entries shared_bytes shared_max_bytes shared_max_entries compact_entries transient_entries identity_entries')
        for value in source['cache'].values():ref._int(value,0,128*1024*1024)
        verify_graph(binding['graph'],source,binding['graph'],0,0,check,shared_cache=dict(schema=SCHEMA,max_bytes=CACHE_BYTES,max_entries=CACHE_ENTRIES))
        joined_bytes+=source['admitted_joined_bytes'];work+=source['admitted_work']
        # Recompute economics from RAW named curves, never selected_pv/row signs.
        cases=ref.reconstruct(binding['graph']['source'],check)['cases'];values=[]
        for case in cases:
            curve={p['ordinal']:p['discount_factor'] for p in case['swap_request']['discount_curve']['points']}
            oas=binding['graph']['source']['bridge']['original']['locked_oas']
            values.append(math.fsum((event['components']['principal']+event['components']['cash_interest']+event['components']['fees'])*curve[event['payment_ordinal']]*math.exp(-oas*event['day_offset']/365) for event in case['indexed_result']['schedule']['events']))
            for p in case['indexed_request']['periods']:
                for observation in p['coupon']['observations']:
                    if not binding['original_row']['floor']<=observation['rate']+p['coupon']['margin']<=binding['original_row']['cap']:raise ValueError('unsupported bound contractual cap/floor')
        ref._equal(values[0],binding['original_row']['price']*binding['original_row']['notional']/100,check,'original_quote')
        row=binding['current_row'];oas=binding['graph']['source']['bridge']['original']['locked_oas']
        bound[binding['position_id']]=dict(id=binding['position_id'],base_oas_bp=oas*1e4,applied_oas_bp=oas*1e4,model_price=100*values[1]/row['notional'],notional=row['notional'],market_value=values[1])
    if joined_bytes>128*1024*1024 or work>60000000:raise ValueError('aggregate joined admission')
    ref._source_equal(meta['admitted_bytes'],joined_bytes,check,'joined_bytes');ref._source_equal(meta['admitted_work'],work,check,'work')
    expected_positions={};expected_totals={}
    for book in norm['legacy']['current']['terms']:
        expected_positions[book['key']]=[bound[ident] for ident in book['ids']]
        total=0.
        for ident in book['ids']:total+=bound[ident]['market_value']
        expected_totals[book['key']]=total
    ref._equal(result['positions'],expected_positions,check,'positions');ref._equal(result['totals'],expected_totals,check,'totals')
    net=0.
    for key in sorted(expected_totals):net+=expected_totals[key]*(1 if key=='loans' else -1)
    ref._equal(result['scope_net_value'],net,check,'net')
    # A fully-bound request has no legacy finance nodes to compute or reuse.
    if type(result['graph']) is not dict:raise ValueError('legacy graph metadata')
    for count in result['graph'].values():
        _fields(count,'computed reused batches')
        if any(type(x) is not int or x!=0 for x in count.values()):raise ValueError('unowned legacy finance nodes')
    ref._check(check)
    raw_hash=hashlib.sha256(json.dumps(raw,allow_nan=False,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
    return dict(schema='indexed-book-publication-verification-1',verified=True,
        input_sha256=raw_hash,fully_bound_rows=rows,pricing_only=True,
        independent_raw_cash_price_risk_verified=True,persisted_journal_verified=False,
        saved_workspace_adoption=False,empirical_validation=False,capital_ftp_eligible=False)
