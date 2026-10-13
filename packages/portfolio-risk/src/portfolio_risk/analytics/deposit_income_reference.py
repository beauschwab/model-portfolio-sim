"""Independent original-yield deposit source oracle, never a financial backend."""
from copy import deepcopy
import math
from .credit_path_verification import equal
from .deposit_schedule_reference import checkpoint, project

SCHEMA = 'deposit-graph-income-ledger-1'
CAPTURE = 'deposit-original-income-capture-1'

def verify_preparation(raw, prepared, receipt, check=lambda: False):
    checkpoint(check)
    if (raw['schema'] != SCHEMA or receipt['raw_request'] != raw or
            receipt['schema'] != 'deposit-graph-income-ledger-source-1' or
            receipt['normalization_policy'] != 'native_original_eir_deposit_opening_anchors-1'):
        raise ArithmeticError('original deposit immutable source identity differs')
    equal(receipt['raw_request'], raw, 'original_deposit/raw_typed_identity')
    policy = raw['capture_policy']; graph = raw['graph']; actual = receipt['capture']
    if (policy['recognition_model'] != 'deposit_daily_original_eir-1' or
            policy['book_basis'] != 'original_expected_cash_deposit-1' or policy['fees'] != 'reject' or
            actual['schema'] != CAPTURE):
        raise ArithmeticError('original deposit reviewed accounting policy differs')
    anchors = {a['position_id']: a for a in raw['income_anchors']}
    if len(anchors) != len(raw['income_anchors']) or set(anchors) != {p['position_id'] for p in graph['positions']}:
        raise ArithmeticError('original deposit complete unique anchors required')
    for anchor in anchors.values():
        values = [anchor['original_principal'], anchor['original_gross_carrying_value'],
            anchor['purchase_price_per100'], anchor['original_annual_effective_rate']]
        if (any(not math.isfinite(v) for v in values) or not 0. < values[0] <= 1e15 or
                not 0. < values[1] <= 1e15 or not 0. < values[2] <= 1e6 or not 0. <= values[3] <= 10. or
                anchor['purchase_ordinal'] != graph['as_of_ordinal'] or
                anchor['basis_revision_policy'] != 'reject_changed_cash_expectations-1' or
                anchor['terminal_basis_policy'] != 'reject_nonzero-1'):
            raise ArithmeticError('original deposit purchase/yield/basis policy unsupported')
        equal(values[1], values[0] * values[2] / 100., 'original_deposit/quote')
        for identity in [anchor['purchase_quote'], anchor['original_yield_anchor']]:
            if any(not isinstance(v,str) or not v.strip() or len(v.encode()) > 120 for v in identity.values()):
                raise ArithmeticError('original deposit quote/yield provenance invalid')

    # Independently verify the existing cash/earned-rights slice. Only explicit
    # generated book events are removed from this temporary oracle view; below
    # we reconstruct and compare the COMPLETE income capture, so extra events
    # cannot be hidden by removal here. Original queued anchors remain untouched.
    par_raw = deepcopy(raw); par_raw.pop('income_anchors'); par_raw['schema'] = 'deposit-graph-dated-ledger-1'
    par_raw['capture_policy'].update(recognition_model='deposit_daily_par-1', book_basis='par_deposit_simple_interest')
    for p in par_raw['specification']['positions']:
        if p['id'] in anchors and 'book_adjustment' in p: p['book_adjustment'] = 0.
    par_receipt = deepcopy(receipt); par_receipt.pop('income_sources')
    par_receipt.update(schema='deposit-graph-dated-ledger-source-1', raw_request=par_raw,
        normalization_policy='native_par_deposit_opening_anchors-1')
    par_capture = par_receipt['capture']; par_capture['schema'] = 'deposit-scheduled-capture-1'
    par_capture['policy'] = deepcopy(par_raw['capture_policy'])
    def book_event(event): return event['provenance'].get('model_id') == CAPTURE
    for s in par_capture['sources']: s['schedule']['events'] = [e for e in s['schedule']['events'] if not book_event(e)]
    par_capture['accrual_plans'] = [p for p in par_capture['accrual_plans'] if not p['event_id'].startswith('deposit-eir:')]
    for opening in par_capture['openings']: opening['gross_carrying_value'] = opening['principal']
    par_prepared = deepcopy(prepared)
    for p in par_prepared['specification']['positions']:
        if p['id'] in anchors: p['book_adjustment'] = 0.
    par_prepared['dated']['sources'] = deepcopy(par_capture['sources'])
    par_prepared['dated']['accrual_plans'] = [dict(p,segments=[{k:v for k,v in s.items() if k != 'origin_id'} for s in p['segments']]) for p in par_capture['accrual_plans']]
    from .deposit_schedule_reference import verify_preparation as verify_par
    verify_par(par_raw, par_prepared, par_receipt, check)
    expected_capture = deepcopy(par_capture); expected_capture['schema'] = CAPTURE
    expected_capture['policy'] = deepcopy(policy)
    expected_capture['scope'] = 'purchase-date original EIR on complete unchanged original physical expected cash; gross contractual expense/rights and EIR basis separated; no Q-yield inference, catchup, fees or terminal plug'
    owners = {o['position_id']: o for o in raw['owners']}; income_sources = []; work = 0
    for position in graph['positions']:
        checkpoint(check); anchor = anchors[position['position_id']]; owner = owners[position['position_id']]
        base = next(h for h in position['histories'] if h['scenario_id'] == graph['base_scenario_id'])
        original = project(base['physical'][0]['request'], check)['cashflows']
        opening = next(o for o in expected_capture['openings'] if o['position_id'] == position['position_id'])
        if opening['principal'] != anchor['original_principal']: raise ArithmeticError('original deposit face differs')
        opening['gross_carrying_value'] = anchor['original_gross_carrying_value']
        source = next(s for s in expected_capture['sources'] if s['source_id'] == owner['source_id'])
        for scenario in graph['scenarios']:
            checkpoint(check); history = next(h for h in position['histories'] if h['scenario_id'] == scenario['scenario_id'])['physical'][0]['request']
            output = project(history, check); asof = graph['as_of_ordinal']; end = asof + history['horizon_days']
            if output['cashflows'] != original: raise ArithmeticError('changed original deposit expected cash needs reviewed catchup')
            if history['horizon_days'] >= 4096: raise ArithmeticError('original deposit complete daily income tail exceeds admission')
            work += history['horizon_days'] + 1
            if work > 250000: raise ArithmeticError('original deposit income work exceeds admission')
            cash = {}; earned = {}
            for row in output['cashflows']:
                v = cash.setdefault(row['ordinal'], [0.,0.]); v[0] -= row['principal']; v[1] -= row['cash_interest']
            for origin in output['interest']:
                if origin['end_ordinal'] - origin['start_ordinal'] != 1: raise ArithmeticError('original deposit daily earned origin missing')
                earned[origin['end_ordinal']] = earned.get(origin['end_ordinal'], 0.) + origin['amount']
            rate = anchor['original_annual_effective_rate']; carrying = anchor['original_gross_carrying_value']
            equal(sum((p+i) / (1.+rate)**((d-asof)/365.) for d,(p,i) in cash.items()), carrying, 'original_deposit/complete_p_cash_yield')
            dates = ([asof] if asof in cash else []) + list(range(asof+1,end+1))
            events = [dict(event_id=f'deposit-eir:{d}', source=dict(id=owner['source_id'],revision=owner['source_revision']),
                payment_ordinal=d,contractual_interest_earned=earned.get(d,0.),principal_cash=cash.get(d,[0.,0.])[0],
                interest_cash=cash.get(d,[0.,0.])[1],integral_fee_cash=0.,nonintegral_fee_cash=0.,yield_change=None) for d in dates]
            request = dict(schema='dated-income-1',contract=dict(id=position['position_id'],revision=graph['identity']['input_revision']),
                original_yield_anchor=deepcopy(anchor['original_yield_anchor']),policy=dict(id=policy['policy_id'],revision=policy['policy_revision']),
                currency=graph['currency'],monetary_unit='currency_units',measure='physical',as_of_ordinal=asof,full_contract_end_ordinal=end,
                opening_principal=anchor['original_principal'],opening_accrued_interest=0.,opening_gross_carrying_value=carrying,
                original_annual_effective_rate=rate,opening_annual_effective_rate=rate,events=events)
            date = asof; principal = anchor['original_principal']; accrued = 0.; rows = []
            for event in events:
                checkpoint(check); opening_carrying = carrying; day = event['payment_ordinal']
                effective = carrying * ((1.+rate)**((day-date)/365.) - 1.)
                amort = effective - event['contractual_interest_earned']; payment = event['principal_cash'] + event['interest_cash']
                carrying += effective-payment; principal -= event['principal_cash']; accrued += event['contractual_interest_earned']-event['interest_cash']
                row = dict(event_id=event['event_id'],source=deepcopy(event['source']),payment_ordinal=day,opening_gross_carrying_value=opening_carrying,
                    annual_effective_rate=rate,effective_interest=effective,contractual_interest_earned=event['contractual_interest_earned'],
                    basis_amortization=amort,modification_gain_loss=0.,recognized_nonintegral_fee_income=0.,cash_received=payment,
                    closing_principal=principal,closing_accrued_interest=accrued,closing_gross_carrying_value=carrying,
                    closing_book_adjustment=carrying-principal-accrued,yield_review=None)
                rows.append(row); date = day
                if day == asof: continue
                selector = dict(kind='named',scenario_id=scenario['run_id']); amount = -amort
                expected_capture['accrual_plans'].append(dict(position_id=position['position_id'],event_id=event['event_id'],run=selector,
                    segments=[dict(origin_id=f"deposit-effective-interest:{anchor['original_yield_anchor']['id']}:{day}",start_ordinal=day-1,end_ordinal=day,accrued_interest=0.,book_amortization=amount)]))
                source['schedule']['events'].append(dict(position_id=position['position_id'],event_id=event['event_id'],scenario=selector,
                    payment_ordinal=day,day_offset=day-asof,accrual_period=dict(start_ordinal=day-1,end_ordinal=day),currency=graph['currency'],
                    components=dict(principal=0.,cash_interest=0.,accrued_interest=0.,book_amortization=amount,fees=0.),measure='physical',
                    provenance=dict(kind='modeled',model_id=CAPTURE,model_version=CAPTURE,input_revision=graph['identity']['input_revision']),adjustment=dict(kind='none')))
            for value in [carrying,principal,accrued,carrying-principal-accrued]: equal(value,0.,'original_deposit/terminal_closure')
            result = dict(schema='dated-income-1',contract=request['contract'],policy=request['policy'],original_yield_anchor=anchor['original_yield_anchor'],
                currency=graph['currency'],monetary_unit='currency_units',measure='physical',rows=rows,remaining_principal=principal,
                remaining_accrued_interest=accrued,remaining_gross_carrying_value=carrying)
            income_sources.append(dict(position_id=position['position_id'],scenario_id=scenario['scenario_id'],run_id=scenario['run_id'],
                anchor=deepcopy(anchor),request=request,result=result))
        source['schedule']['events'].sort(key=lambda e:e['payment_ordinal'])
    if sum(len(p['segments']) for p in expected_capture['accrual_plans']) > 250000: raise ArithmeticError('original deposit combined origins exceed admission')
    equal(receipt['income_sources'], income_sources, 'original_deposit/income_sources')
    equal(actual, expected_capture, 'original_deposit/complete_capture')
    expected = deepcopy(par_prepared)
    for p in expected['specification']['positions']:
        if p['id'] not in anchors: continue
        value = anchors[p['id']]['original_gross_carrying_value']-anchors[p['id']]['original_principal']
        original = next(q for q in raw['specification']['positions'] if q['id'] == p['id'])
        if 'book_adjustment' in original and original['book_adjustment'] != value: raise ArithmeticError('explicit original deposit book anchor conflicts')
        p['book_adjustment'] = value
    expected['dated']['sources'] = expected_capture['sources']
    expected['dated']['accrual_plans'] = [dict(p,segments=[{k:v for k,v in s.items() if k != 'origin_id'} for s in p['segments']]) for p in expected_capture['accrual_plans']]
    equal(prepared, expected, 'original_deposit/prepared_income')
    return dict(physical_deposit_source_recomputed=True,accrual_origins_retained=True,original_deposit_income_recomputed=True,
        original_yield_anchors_retained=True,pricing_graph_independently_verified=False,empirical_validation=False)
