"""Raw input transport and table formatting for Rust-owned numerical lifecycles."""
import operator
import numpy as np
import polars as pl
from . import config as cfg
from .quant_native import term_call
from .runtime import assumption,path_count,stress_horizons


def forward_programs(programs, swap_rates, vol_pts, runoff, horizon, seed):
    result=term_call('programs-1', dict(market=market_input(swap_rates,vol_pts,seed), programs=programs,
        horizon=horizon,runoff={} if runoff is None else {k:np.asarray(v).tolist() for k,v in runoff.items()}))
    names=result['names']; months=np.arange(1,horizon+1)
    return dict(nii_incremental=pl.DataFrame(dict(month=months,**dict(zip(names,result['income'])),net=result['net'])),
        balances=pl.DataFrame(dict(month=months,**dict(zip(names,result['balances'])))),
        fwd_dv01=pl.DataFrame(dict(month=months,**dict(zip(names,result['dv01'])))),
        summary=pl.DataFrame(dict(metric=['nii_incremental_total_$','nii_incremental_annualized_$','peak_balance_$','fwd_dv01_at_h27_$'],value=result['summary'])))


def market_input(swap_rates,vol_pts,seed):
    seed=operator.index(seed)
    if seed<0 or seed.bit_length()>32768:
        raise ValueError('native seed must be nonnegative with at most 32768 bits')
    words=[0] if seed==0 else []
    while seed:
        words.append(seed&0xffffffff);seed>>=32
    return dict(config=dict(paths=path_count(cfg.N_PATHS_SENS),months=cfg.N_STEPS,forwards=cfg.N_FWD,
        factors=cfg.N_FACTORS,dt=cfg.DT,tenor=cfg.TENOR,shift=cfg.SHIFT,curve_bump=cfg.CURVE_BUMP,vol_bump=cfg.VOL_BUMP),
        tenors=np.asarray(cfg.SWAP_TENORS).tolist(),swap_rates=np.asarray(swap_rates).tolist(),
        vol_quotes=np.asarray(vol_pts).ravel().tolist(),seed=words)


def deposit_input(book):
    from ..products import deposits as d
    if missing:=d.DEPOSIT_COLS-set(book.columns): raise ValueError(f'deposit book missing columns: {missing}')
    fields=['balance','segment','age_months','avg_account_size','rate_paid','price']
    fields += [name for name in ['svc_cost','attrition_base','attrition_amp','attrition_slope','attrition_gap'] if name in book.columns]
    return dict(book=book.select(fields).to_dicts(),assumptions=dict(segments=assumption('deposit_segments',d.SEGMENTS),
        size_x=d.SIZE_X.tolist(),size_y=d.SIZE_Y.tolist(),age_knots=d.AGE_KNOTS.tolist(),
        age_coefficients=d.AGE_COEFS.ravel().tolist(),velocity_coefficient=d.VEL_COEF,attrition_cap=d.ATTR_CAP))


def deposit_deck(book):
    result=term_call('deposit-deck-1',deposit_input(book))
    return {name:value if name=='n' else np.asarray(value,dtype=np.float64) for name,value in result.items()}


def deposit_request(book,swap_rates,vol_pts,history,seed,rate_model,oas):
    from ..products.deposits import LogisticBetaECM
    if rate_model is not None and type(rate_model) is not LogisticBetaECM:
        raise ValueError('Custom deposit rate models are not supported by the Rust backend')
    return dict(**deposit_input(book),history=history.select(['ff','dep_rate']).rows(),
                market=market_input(swap_rates,vol_pts,seed),fixed_oas=[] if oas is None else np.asarray(oas).tolist())


def deposit_risk(book,swap_rates,vol_pts,history,seed,rate_model,oas):
    result=term_call('deposit-risk-1',deposit_request(book,swap_rates,vol_pts,history,seed,rate_model,oas))
    risk=result['risk'];labels=[f'krd01_{int(t)}y' for t in cfg.SWAP_TENORS]+[f'vega_{int(e)}x{int(t)}' for e,t,_ in vol_pts]
    return book.with_columns(pl.Series('oas_bps',np.asarray(risk['oas'])*1e4),
        pl.Series('model_price',np.asarray(risk['price'])*100),pl.Series('premium_pct',result['premium']),
        pl.Series('wal_y',result['wal']),pl.Series('dv01',risk['dv01']),
        *[pl.Series(name,values) for name,values in zip(labels,np.asarray(risk['sensitivities']).reshape(len(labels),len(book)))])


def deposit_stress(book,swap_rates,vol_pts,history,shocks,seed,rate_model,oas):
    request=deposit_request(book,swap_rates,vol_pts,history,seed,rate_model,oas)
    horizons=stress_horizons(cfg.STRESS_HORIZONS_M).tolist()
    shocks=list(cfg.STRESS_SHOCKS_BP if shocks is None or len(shocks)==0 else shocks)
    request.update(horizons=horizons,shocks=shocks)
    result=term_call('deposit-stress-1',request)
    n=len(book);h=len(horizons);s=len(shocks)
    values=np.asarray(result['base_value']).reshape(h,n);balances=np.asarray(result['balance']).reshape(h,n)
    pnl=np.asarray(result['pnl']).reshape(s,h,n);eve=np.asarray(result['eve']).reshape(s,h,n)
    frames=[]
    for j,shock in enumerate(shocks):
        for hi,horizon in enumerate(horizons):
            frames.append(book.select(['id','segment']).with_columns(pl.lit(horizon,dtype=pl.Int64).alias('horizon_m'),
                pl.lit(shock,dtype=pl.Float64).alias('shock_bp'),pl.Series('fwd_liab_value_base',values[hi]),
                pl.Series('fwd_balance',balances[hi]),pl.Series('stress_pnl',pnl[j,hi]),pl.Series('eve_pnl',eve[j,hi])))
    agg=pl.DataFrame(dict(horizon_m=horizons*s,shock_bp=np.repeat(shocks,h),
        **{'eve_pnl_$':result['aggregate_eve'],'liab_mv_$':result['aggregate_value'],'balance_$':result['aggregate_balance']})).sort(['shock_bp','horizon_m'])
    profile=None if result['profile'] is None else pl.DataFrame({'horizon_m':horizons,'fwd_liab_dv01_$':result['profile']})
    return pl.concat(frames),agg,profile


def hedge_input(swaps, swaptions=None):
    from ..products.hedges import SWAP_COLS, SWPN_COLS
    if missing := SWAP_COLS - set(swaps.columns):
        raise ValueError(f'swap book missing columns: {missing}')
    fields = ['notional', 'side', 'fixed_rate', 'maturity']
    if 'float_spread' in swaps.columns:
        fields.append('float_spread')
    options = None
    if swaptions is not None:
        if missing := SWPN_COLS - set(swaptions.columns):
            raise ValueError(f'swaption book missing columns: {missing}')
        options = swaptions.select(['notional', 'side', 'strike', 'expiry_m', 'tenor_y']).to_dicts()
    return dict(swaps=swaps.select(fields).to_dicts(), swaptions=options)


def hedge_deck(swaps, asof):
    from ..products.corp import CorpDeck
    from .quant_native import _term_deck_fields
    request = dict(swaps=hedge_input(swaps)['swaps'], asof=asof, months=cfg.N_STEPS)
    result = term_call('hedge-deck-1', request)
    legs = {}
    for key in ('fix', 'flt'):
        legs[key] = object.__new__(CorpDeck)
        legs[key].__dict__.update(_term_deck_fields(result[key], 'corporate'))
    return dict(frame=swaps, **legs, n=result['n'], side=np.asarray(result['side']),
                notional=np.asarray(result['notional']))


def hedge_risk(swaps, swaptions, asof, swap_rates, vol_pts, seed, horizon):
    request = dict(**hedge_input(swaps, swaptions), asof=asof, horizon=horizon,
                   market=market_input(swap_rates, vol_pts, seed))
    result = term_call('hedge-risk-1', request)
    return {'positions': swaps.with_columns(pl.Series('mtm_$', result['mtm']),
                    pl.Series('carry_y1_$', result['carry_y1'])),
            'swaptions': None if swaptions is None else swaptions.with_columns(pl.Series('value_$', result['option_values'])),
            'book_dv01_$': result['dv01'],
            'book_krd': {f'krd01_{int(t)}y': v for t, v in zip(cfg.SWAP_TENORS, result['krd'])},
            'book_vega_$': result['vega'], 'carry_monthly_$': np.asarray(result['carry_monthly']),
            'mtm_total_$': result['mtm_total']}


def mortgage_input(port, swap_rates, vol_pts, histories, seed):
    from .quant_native import _mortgage_inputs, prepay_speed
    a = _mortgage_inputs(port, swap_rates, vol_pts, *histories, seed, None, None)
    c = np.asarray(a[8]).tolist()
    keys = ['base_paths','sensitivity_paths','months','forwards','factors','dt','tenor','shift',
            'float32_paths','curve_bump','vol_bump']
    config = dict(zip(keys, c[:11]))
    for key in keys[:5]: config[key] = int(config[key])
    config['float32_paths'] = bool(config['float32_paths'])
    config.update(hpi=c[11:14], incentive_lag=int(c[14]), ps_spot=c[15], rational_sigmoid=bool(c[16]))
    flat = lambda i: np.asarray(a[i]).ravel().tolist()
    return dict(tenors=flat(0),swap_rates=flat(1),vol_quotes=flat(2),cc_history=flat(3),ps_history=flat(4),
        book=flat(5),original_hpi=flat(6),prepay_multiplier=prepay_speed(port),seed=[int(v) for v in flat(7)],fixed_oas=flat(9),config=config,
        prepay=dict(month_of_year=flat(10),seasonality=flat(11),parameters=flat(12),ltv_knots=flat(13),
            ltv_coefficients=flat(14),smm_table=flat(15),smm_scale=a[16][0],burnout_scale=a[16][1],
            burnout_table=flat(17),cc_vol_points=flat(18),fico_x=flat(19),fico_y=flat(20),size_x=flat(21),
            size_y=flat(22),state_multipliers=flat(23),channel_multipliers=flat(24)))


def accounting_request(bs, swap_rates, vol_pts, dep_hist, horizon, seed, asof,
               forecast_plan, accounting_anchor, capture_anchor, crn, capture_cashflows):
    import datetime as dt
    from .quant_native import _term_deck_request
    from ..products.cds import CD_EW_PARAMS
    from ..products.mm import MM_COLS
    seed = cfg.SEED if seed is None else seed
    asof = asof or dt.date(2026, 6, 10)
    if forecast_plan is not None and accounting_anchor is None:
        raise ValueError('conditional forecast requires base accounting anchors')
    raw_market = market_input(swap_rates, vol_pts, seed)
    draws = None
    if crn is not None:
        raw_market['config']['paths'] = crn.n
        draws = dict(rates=np.asarray(crn.Z).ravel().tolist(),spread=np.asarray(crn.eps_ps).ravel().tolist(),
                     hpi=np.asarray(crn.eps_h).ravel().tolist(),shape=list(crn.Z.shape))
    def book_yields(frame):
        return frame['book_yield'].to_list() if 'book_yield' in frame.columns else None
    terms = [dict(key=key, ids=bs[key]['id'].cast(pl.String).to_list(),book_yields=book_yields(bs[key]),
                  deck=_term_deck_request(bs[key],asof,'cd' if key=='cds' else 'corporate'))
             for key in ('loans','debt','cds') if key in bs]
    mbs = None if 'mbs' not in bs else dict(ids=bs['mbs']['cusip'].cast(pl.String).to_list(),
          book_yields=book_yields(bs['mbs']),request=mortgage_input(bs['mbs'],swap_rates,vol_pts,bs['mbs_hists'],seed))
    deposits = None if 'deposits' not in bs else dict(ids=bs['deposits']['id'].cast(pl.String).to_list(),
        **deposit_input(bs['deposits']),history=dep_hist.select(['ff','dep_rate']).rows())
    mm = None
    if bs.get('mm') is not None:
        if missing := MM_COLS - set(bs['mm'].columns): raise ValueError(f'mm book missing columns: {missing}')
        mm = bs['mm'].select(['id','balance','side','spread_bp']).to_dicts()
    anchors = {}
    for key, value in (accounting_anchor or {}).items():
        if key != 'deposit_initial_rate':
            anchors[key] = {'yield':np.asarray(value['yield']).tolist(),'opening':np.asarray(value['opening']).tolist(),
                           'coupon_income':None if 'coupon_income' not in value else np.asarray(value['coupon_income']).ravel().tolist()}
    request = dict(market=raw_market,asof=asof,horizon=horizon,mbs=mbs,terms=terms,deposits=deposits,mm=mm,
        hedges=None if bs.get('hedges') is None else hedge_input(bs['hedges'][0])['swaps'],
        withdrawal_parameters=np.asarray(assumption('cd_ew_params',CD_EW_PARAMS)).tolist(),anchors=anchors,
        deposit_initial_rate=(accounting_anchor or {}).get('deposit_initial_rate'),
        forecast=None if forecast_plan is None else dict(targets={k:np.asarray(v).tolist() for k,v in forecast_plan['targets'].items()}),
        draws=draws,capture_anchor=capture_anchor,capture_cashflows=capture_cashflows)
    return request


def accounting(bs, swap_rates, vol_pts, dep_hist, horizon, seed, asof,
               forecast_plan, accounting_anchor, capture_anchor, crn, capture_cashflows):
    request = accounting_request(bs, swap_rates, vol_pts, dep_hist, horizon, seed, asof,
                                 forecast_plan, accounting_anchor, capture_anchor, crn, capture_cashflows)
    output = term_call('accounting-1', request)
    return accounting_result(output,horizon,capture_anchor,capture_cashflows)


def accounting_result(output,horizon,capture_anchor=False,capture_cashflows=False):
    months = np.arange(1, horizon+1)
    runoff = {key:np.asarray(output['runoff'][key]) for key in output['runoff_order']}
    result = dict(monthly=pl.DataFrame({'month':months,**{k:output['monthly'][k] for k in output['column_order']}}),
        summary=pl.DataFrame({'metric':['nii_total_$','nii_annualized_$','nim_model_%'],
                             'value':[float('nan') if v is None else v for v in output['summary']]}),
        book_yields=pl.DataFrame(output['book_yields'],schema={'book':pl.String,'id':pl.String,'book_yield':pl.Float64,'balance':pl.Float64}),
        runoff=pl.DataFrame({'month':months,**runoff}),runoff_vectors=runoff)
    if capture_anchor:
        result['accounting_anchor'] = {key:{'yield':np.asarray(v['yield']),'opening':np.asarray(v['opening']),
            **({'coupon_income':np.asarray(v['coupon_income']).reshape(-1,cfg.N_STEPS)} if v['coupon_income'] is not None else {})}
            for key,v in output['anchors'].items()}
        if output['deposit_initial_rate'] is not None:
            result['accounting_anchor']['deposit_initial_rate'] = output['deposit_initial_rate']
    if capture_cashflows:
        flow_keys = ['book','id','month','principal','cash_interest','accrual_interest','book_amortization']
        open_keys = ['book','id','balance','book_adjustment','side','market_price']
        result['instrument_cashflows'] = pl.DataFrame([{k:r[k] for k in flow_keys} for r in output['instrument_cashflows']])
        result['instrument_openings'] = pl.DataFrame([{k:r[k] for k in open_keys} for r in output['instrument_openings']])
    return result


def balance_risk_request(bs,swap_rates,vol_pts,dep_hist,seed,bump_bp,oas):
    import datetime as dt
    asof = bs.get('asof') or dt.date.today()
    books = accounting_request(bs,swap_rates,vol_pts,dep_hist,1,seed,asof,None,None,False,None,False)
    return dict(books=books,bump_bp=bump_bp,oas={k:np.asarray(v).tolist() for k,v in (oas or {}).items()},
                swaptions=None if bs.get('hedges') is None else hedge_input(*bs['hedges'])['swaptions'])


def parallel_dv01s(bs,swap_rates,vol_pts,dep_hist,seed,bump_bp,oas,valued_books):
    result=term_call('balance-risk-1',balance_risk_request(bs,swap_rates,vol_pts,dep_hist,seed,bump_bp,oas))
    if valued_books is not None:
        for key,prices in result['valued_prices'].items():
            valued_books[key]=bs[key].with_columns(pl.Series('price',np.asarray(prices)*100))
    return result['dv01s']


def kpi_request(mode,bs,asof,nii=None,stress_aoci=None,dv01s=None,shocks=(-200,-100,100,200),risk=None):
    from ..analytics import kpis as k
    from ..analytics.balance_rules import INFLOW_CAP
    books={}
    for key in ['mbs','loans','debt','deposits','cds','mm']:
        frame=bs.get(key)
        if frame is None: continue
        bal='balance' if 'balance' in frame.columns else 'current_face' if 'current_face' in frame.columns else 'face'
        sid='cusip' if key=='mbs' else 'id'
        fields=[sid,bal]+[name for name in ['price','maturity','side','segment','channel'] if name in frame.columns]
        books[key]=[dict(id=str(row[sid]),balance=row[bal],price=row.get('price',100.),maturity=row.get('maturity'),
                        side=row.get('side'),segment=row.get('segment'),channel=row.get('channel')) for row in frame.select(fields).to_dicts()]
    weights=dict(lcr_runoff=k.LCR_RUNOFF,lcr_cd_runoff=k.LCR_CD_RUNOFF,lcr_secured=k.LCR_SECURED,lcr_inflow=k.LCR_INFLOW,
                 nsfr_asf=k.NSFR_ASF,nsfr_rsf=k.NSFR_RSF,rwa=k.RWA_W,l2_cap=k.L2_CAP,l2a_factor=k.L2A_FACTOR,
                 inflow_cap=INFLOW_CAP,rwa_density=k.RWA_DENSITY_TARGET,cet1_ratio=k.CET1_RATIO_T0,ni_to_nii=k.NI_TO_NII,payout=k.PAYOUT)
    return dict(mode=mode,books=books,equity=bs.get('equity'),asof=asof,weights=weights,nii=None if nii is None else nii['nii'].to_list(),
                stress_aoci=[] if stress_aoci is None else list(stress_aoci),shocks=list(shocks),dv01s=dv01s or {},risk=risk)


def kpi_output(value):
    # JSON represents undefined ratios as null; retain the public NaN contract.
    if value is None:return float('nan')
    if isinstance(value,dict):return {k:kpi_output(v) for k,v in value.items()}
    if isinstance(value,list):return [kpi_output(v) for v in value]
    return value


def kpis(mode,bs,asof,**kwargs):
    return kpi_output(term_call('kpi-1',kpi_request(mode,bs,asof,**kwargs)))


def unit_request(swap_rates,vol_pts,mbs_hists,dep_hist,grid_m,horizon,seed,asof,templates):
    import datetime as dt
    from ..core.scenarios import REQUIRED_COLS
    from ..products.deposits import DEPOSIT_COLS
    from ..products.cds import CD_EW_PARAMS
    mbs = pl.DataFrame(schema={k:pl.String if k in ['cusip','state','channel'] else pl.Float64 for k in REQUIRED_COLS})
    deposits = pl.DataFrame(schema={k:pl.String if k in ['id','segment'] else pl.Float64 for k in DEPOSIT_COLS})
    request=dict(market=market_input(swap_rates,vol_pts,seed),mortgage=mortgage_input(mbs,swap_rates,vol_pts,mbs_hists,seed),
        templates=[dict(name=k,template=v) for k,v in templates.items()],grid_m=list(range(0,horizon,6) if grid_m is None else grid_m),
        horizon=horizon,asof=asof or dt.date(2026,6,10),deposit_assumptions=deposit_input(deposits)['assumptions'],
        deposit_history=dep_hist.select(['ff','dep_rate']).rows(),withdrawal_parameters=np.asarray(assumption('cd_ew_params',CD_EW_PARAMS)).tolist())
    return request


def unit_output(result, templates):
    horizon=result['horizon']
    return dict(units=result['units'],grid_m=result['grid_m'],horizon=horizon,templates=templates,
                **{k:np.asarray(result[k]).reshape(-1,horizon) for k in ['nii','runoff','balance','cash_interest']},
                dv01=np.asarray(result['dv01']),vectors={(v['template'],v['purchase_m']):np.asarray(v['values']).reshape(10,horizon) for v in result['vectors']})


def unit_library(swap_rates,vol_pts,mbs_hists,dep_hist,grid_m,horizon,seed,asof,templates):
    request=unit_request(swap_rates,vol_pts,mbs_hists,dep_hist,grid_m,horizon,seed,asof,templates)
    return unit_output(term_call('unit-library-1',request), templates)


def prepare_coefficients(lib):
    result=term_call('unit-coefficients-1',dict(units=lib['units'],templates=[dict(name=k,template=dict(v,kind=v.get('kind','coefficient'))) for k,v in lib['templates'].items()],
        horizon=lib['horizon'],**{k:np.asarray(lib[k]).ravel().tolist() for k in ('nii','balance','dv01')}))
    return {(r['template'],r['purchase_m']):np.asarray(r['values']).reshape(10,lib['horizon']) for r in result}


def evaluate_strategy(lib,allocations,base_kpis):
    from .quant_native import call
    from ..analytics.kpis import L2_CAP,NI_TO_NII,PAYOUT
    h=lib['horizon'];amounts=[];vectors=[]
    prepared=lib.get('vectors')
    if prepared is None:prepared=prepare_coefficients(lib)
    for allocation in allocations:
        month=allocation['purchase_m'];name=allocation['template']
        if not isinstance(month,(int,np.integer)) or month<0:
            raise ValueError('purchase_m must be a nonnegative integer')
        if name not in lib['templates']:raise ValueError(f'unknown template {name}')
        if month>=h:vector=np.zeros((10,h))
        else:
            vector=prepared.get((name,month))
            if vector is None:raise ValueError('Rust evaluation requires prepared unit-library coefficients')
        vectors.append(vector);amounts.append(float(allocation['notional']))
    base=[]
    if base_kpis:
        e,l,n,c=[base_kpis[k] for k in ['eve','lcr','nsfr','capital']]
        if e['eve_$']<=0:raise ValueError('positive base EVE is required for relative EVE limits')
        base=[e['eve_$'],e['dv01_net_$'],e['mv_assets_$'],l.get('hqla_l1_$',l['hqla_$']),
              l.get('hqla_l2a_uncapped_$',l.get('hqla_l2a_$',0.)),l['net_outflows_$'],n['asf_$'],n['rsf_$'],
              c['cet1_path'][-1]['cet1_$'],c['rwa_total_$'],L2_CAP,NI_TO_NII,PAYOUT]
    values,path,scalars=call(31,[np.asarray(vectors).reshape(-1,10,h),amounts,base],[(11,h),(3,h),(7,)])
    result=dict(nii_incremental=values[0],balance=values[1],fwd_dv01=values[2],
                **{'nii_total_$':float(scalars[0]),'dv01_at_t0_$':float(scalars[1])},
                funding_gap=values[10],horizon_months=h,dv01_method='base unit DV01 scaled by outstanding balance')
    if base_kpis:
        result['kpis']=dict(zip(['d_eve_pct_eve_+200','duration_gap_y','lcr_pct','nsfr_pct','cet1_q9_pct'],map(float,scalars[2:])))
        result['kpis']['cet1_horizon_pct']=float(scalars[6])
        result['kpi_path']=dict(zip(['d_eve_pct_eve_+200','lcr_pct','nsfr_pct'],path))
    return result
