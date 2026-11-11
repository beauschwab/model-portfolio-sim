"""Independent generated law/policy, compact source and coefficient graph replay."""
from copy import deepcopy
from contextlib import contextmanager
from contextvars import ContextVar

_ACCOUNTING_SPOOL = ContextVar("commercial_generated_accounting_spool", default=None)

@contextmanager
def accounting_spool(factory):
    """Optional bounded independent group store; default receipts are unchanged."""
    token = _ACCOUNTING_SPOOL.set(factory)
    try:
        yield
    finally:
        _ACCOUNTING_SPOOL.reset(token)

from .commercial_stochastic_reference import fit_pricing_law,generated_policy_sources
from .commercial_credit_reference import replay
from .commercial_accounting_reference import groups
from .commercial_ledger_reference import compile_original_acquisition,identity
from .commercial_source_reference import compare
from .commercial_reference_math import _cash,_price
from .commercial_dto import graph_identity

def verify(raw,result,check=lambda:False):
    graph_identity(raw,result['raw_source']);p=raw['positions'][0];g=deepcopy(p['generated_policy'])
    from .commercial_generated_physical_reference import validate as validate_physical,current as current_inputs
    validate_physical(g,check)
    fee_acquisition=g.get('fee_acquisition')
    if fee_acquisition:
        if g.get('physical_path_inputs'):raise ArithmeticError('generated fee catalog ownership requires separate admission')
        law=g['pricing_training'].get('nonintegral_fees')
        if not law or any(q.get('nonintegral_fees')!=law for q in [g['original_physical'],*g['current_physical'],*g['current_pricing']]):raise ArithmeticError('generated service contract/spread differs across roles')
        from .commercial_generated_fee_reference import ACQUISITION_SCHEMA
        from .commercial_fee_adoption_reference import price as fee_price
        if fee_acquisition.get('schema')!=ACQUISITION_SCHEMA or fee_acquisition.get('source_work_policy')!='counted_native_contract_factor_work-1':raise ArithmeticError('generated fee acquisition schema differs')
        compare(float(sum(a['allocated_cost']for a in fee_acquisition['allocations'])),float(fee_acquisition['original_signed_fee_quote']),check=check)
        compare(float(p['original_signed_cash_quote']+fee_acquisition['original_signed_fee_quote']),float(fee_acquisition['total_purchase_cost']),check=check)
    elif any(q.get('nonintegral_fees')for q in [g['pricing_training'],g['original_physical'],*g['current_physical'],*g['current_pricing']]):raise ArithmeticError('generated fee acquisition absent')
    if len(raw['positions'])!=1 or p.get('histories') or p.get('original_pricing') or p.get('exercise'):raise ArithmeticError('exclusive generated graph required')
    if p['oas_target']['kind']=='calibrate_positive_cash' and g.get('oas_calibration'):
        from .commercial_calibration_reference import verify as verify_calibration
        if p['oas_target']['target_cash_price'] != p['original_signed_cash_quote']:raise ArithmeticError('original calibration quote differs')
        oas=verify_calibration(g['pricing_training'],g['oas_calibration'],p['original_signed_cash_quote'],result['generated_oas_calibration'],check)
        g.pop('oas_calibration')
        for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:law['fixed_base_oas']=oas
    elif p['oas_target']['kind']=='locked' and not g.get('oas_calibration'):
        if result.get('generated_oas_calibration') is not None:raise ArithmeticError('unexpected original calibration receipt')
        oas=p['oas_target']['base_oas']
    else:raise ArithmeticError('explicit locked OAS or original generated calibration required')
    qpolicy=fit_pricing_law(g['pricing_training'],check)
    original=generated_policy_sources(g['pricing_training'],qpolicy,check)
    original_law=deepcopy(g['original_physical']);original_law['source']['source']['current']=deepcopy(original_law['source']['source']['original'])
    op=generated_policy_sources(original_law,qpolicy,check,original=True,physical_inputs=g.get('physical_path_inputs',{}).get('original_physical'))
    proof=compile_original_acquisition([dict(source=s['raw_request'],weight=s['weight']) for s in op],[s['native_stops'] for s in op],check,compact_generated=True)
    actual=result['original_acquisition_proofs']
    keys={'original_stopped_cash_quote','conditional_tranche_proofs','acquired_cash_partitions','acquisition','weights','stopped_physical','generated_original_physical'}
    if len(actual)!=1 or set(actual[0])!=keys:raise ArithmeticError('generated original acquisition proof shape differs')
    for key in keys:compare(proof[key],actual[0][key],check=check)
    sources=[];policies=[]
    def policy(scene,role,fitted):policies.append(dict(position_id=p['position_id'],scenario_id=scene['scenario_id'],role=role,policy=fitted))
    def native(source,scene,role,physical=False,original=False):
        check();r=deepcopy(source['raw_request']);stops=source['native_stops'];book=None
        if physical and not original:
            match=next(s for s in op if s['raw_request']['credit']['paths'][0]['path_key']==r['credit']['paths'][0]['path_key'])
            r['source']['original']=deepcopy(match['raw_request']['source']['original'])
            for draw,anchor in zip(r.get('performing_draw_tranches',[]),match['raw_request'].get('performing_draw_tranches',[]),strict=True):draw['source']['original']=deepcopy(anchor['source']['original'])
        projected=replay(r,check,stops=stops)
        if physical:
            book,_=groups(projected,check)
            factory = _ACCOUNTING_SPOOL.get()
            if factory is not None:
                book = factory(r, book)
        receipt=dict(schema='commercial-generated-source-1',source_binding='retained_source_request-1',native_stops=stops,credit_measure='physical' if physical else 'pricing')
        if physical and not original:receipt['book_points']=[dict(year=s['year'],principal=s['next']['principal'],interest_income_lines=[line['posting']['amount'] for line in s['postings'] if line['posting']['account']=='interest_income']) for s in book]
        wanted=dict(position_id=p['position_id'],scenario_id=scene['scenario_id'],run_id=scene['run_id'],path_id=r['credit']['paths'][0]['path_id'],path_key=r['credit']['paths'][0]['path_key'],weight=source['weight'],role=role,source_request=r,cash=_cash(projected['credit']),projection=receipt)
        if physical:wanted['groups']=book
        if fee_acquisition:
            from .commercial_generated_fee_reference import acquired_projection
            compact,full=acquired_projection(g['pricing_training']['nonintegral_fees'],fee_acquisition,r,projected,check,original=original,stops=stops)
            wanted['generated_fee_projection']=compact
            if physical and not original:
                factory=_ACCOUNTING_SPOOL.get()
                if factory is not None:full['bound_path']['source']['groups']=factory(r,full['bound_path']['source']['groups'])
                wanted['fee_projection']=full

        return wanted
    policy(raw['original_scenario'],'pricing',qpolicy)
    sources.extend(native(s,raw['original_scenario'],'pricing') for s in original)
    original_price=sum(s['weight']*_price(s['cash'],raw['original_scenario']['collateral_curve'],raw['as_of_ordinal'],raw['as_of_ordinal'],oas,0.) for s in sources)
    compare(float(p['original_signed_cash_quote']),float(original_price),check=check)
    if fee_acquisition:
        fee_quote=sum(source['weight']*fee_price(source['generated_fee_projection'],raw['original_scenario']['collateral_curve'],raw['as_of_ordinal'],raw['as_of_ordinal'],original=True)for source in sources)
        compare(float(fee_quote),float(fee_acquisition['original_signed_fee_quote']),check=check)
    expected_original=[native(s,raw['original_scenario'],'physical',physical=True,original=True) for s in op]
    if len(expected_original)!=len(result['original_physical_sources']):raise ArithmeticError('generated original Physical coverage differs')
    for expected,received in zip(expected_original,result['original_physical_sources'],strict=True):
        identity({k:v for k,v in expected.items() if k not in ('cash','groups','projection','fee_projection','generated_fee_projection')},{k:v for k,v in received.items() if k not in ('cash','projection','generated_fee_projection')})
        identity(expected['projection'],received['projection']);compare(expected['cash'],received['cash'],check=check)
        if fee_acquisition:compare(expected['generated_fee_projection'],received.get('generated_fee_projection'),check=check)
    fitted=[]
    for law in g['current_pricing']:fitted.append((law,fit_pricing_law(law,check)))
    for law in g['current_physical']:
        scene=next(s for s in raw['scenarios'] if s['scenario_id']==law['source']['source']['current']['scenario_id'])
        fitted_policy=next(f for q,f in fitted if q['parallel_bump']==0. and q['source']['source']['current']['scenario_id']==scene['scenario_id'])
        sources.extend(native(s,scene,'physical',physical=True) for s in generated_policy_sources(law,fitted_policy,check,physical_inputs=current_inputs(g,law)))
    for law,f in fitted:
        scene=next(s for s in raw['scenarios'] if s['scenario_id']==law['source']['source']['current']['scenario_id']);bump=law['parallel_bump'];role='pricing' if bump==0. else 'pricing_down' if bump<0. else 'pricing_up'
        policy(scene,role,f);sources.extend(native(s,scene,role) for s in generated_policy_sources(law,f,check))
    if len(sources)!=len(result['native_sources']):raise ArithmeticError('generated native source coverage differs')
    for expected,received in zip(sources,result['native_sources'],strict=True):
        identity({k:v for k,v in expected.items() if k not in ('cash','groups','projection','fee_projection','generated_fee_projection')},{k:v for k,v in received.items() if k not in ('cash','projection','generated_fee_projection')})
        identity({k:v for k,v in expected['projection'].items() if k!='book_points'},{k:v for k,v in received['projection'].items() if k!='book_points'})
        if 'book_points' in expected['projection']:compare(expected['projection']['book_points'],received['projection']['book_points'],check=check)
        compare(expected['cash'],received['cash'],check=check)
        if fee_acquisition:
            identity(expected['generated_fee_projection']['raw_fee_history'],received.get('generated_fee_projection',{}).get('raw_fee_history'))
            compare(expected['generated_fee_projection'],received.get('generated_fee_projection'),check=check)
        elif 'generated_fee_projection'in received:raise ArithmeticError('unselected generated fee source')
    compare(policies,result['exercise_policies'],check=check)
    anchors=[dict(position_id=p['position_id'],base_oas=float(oas),original_signed_cash_quote=float(p['original_signed_cash_quote']))];compare(anchors,result['original_oas'],check=check)
    metrics=[]
    current=sources[len(original):]
    for scene in raw['scenarios']:
        for ordinal in raw['reporting_ordinals']:
            values=[]
            for role,bump in [('pricing',0.),('pricing_down',-raw['bump_bp']*1e-4),('pricing_up',raw['bump_bp']*1e-4)]:
                values.append(sum(s['weight']*(_price(s['cash'],scene['collateral_curve'],raw['as_of_ordinal'],ordinal,oas,bump)+(fee_price(s['generated_fee_projection'],scene['collateral_curve'],raw['as_of_ordinal'],ordinal,bump)if fee_acquisition else 0.)) for s in current if s['scenario_id']==scene['scenario_id'] and s['role']==role))
            metrics.append(dict(position_id=p['position_id'],scenario_id=scene['scenario_id'],ordinal=ordinal,pv=values[0],dv01=(values[1]-values[2])/(2.*raw['bump_bp'])))
    compare(metrics,result['metrics'],check=check)
    if result.get('shared_draws_retained') is not True or result.get('physical_accounting_source_retained') is not False or result.get('generated_physical_recipes_retained') is not True or result.get('empirical_validation') is not False:raise ArithmeticError('generated source ownership/eligibility flags differ')
    if type(result.get('admitted_work_upper')) is not int or not 0<result['admitted_work_upper']<=4_000_000_000:raise ArithmeticError('generated graph work admission differs')
    return dict(verified=True,sources=sources,metrics=metrics,original_oas=anchors,physical_accounting_recomputed=True,pricing_cash_recomputed=True,shared_draws_retained=True,empirical_validation=False)
