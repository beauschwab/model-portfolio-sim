"""Durable explicit native model analysis; adapters contain no financial formulas."""
import time
import hashlib
import json
import math
from . import persistence as db
from .model_report_metadata import source_summary


def native_result(owner,request,stopping):
    from portfolio_risk.core import model_native
    from portfolio_risk.analytics.balance_stream import source_identity
    active=db.ACTIVE_JOB.get()
    if db.REPO is None or active is None:raise RuntimeError('model analyses require a durable worker')
    if source_identity()!=request['source_sha256']:raise RuntimeError('MODEL_VERSION_CHANGED: model source differs from queued identity')
    from .commercial_selection import is_commercial_cohort
    commercial=is_commercial_cohort(request['schema'],request['request']) or request['schema'] in {'commercial-product-graph-1','commercial-units-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1','commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1'}
    publication_deadline=request['publication_deadline_unix'] if commercial else time.time()+request['timeout']
    if type(publication_deadline) not in (float,int) or not math.isfinite(publication_deadline):raise ValueError('original model publication deadline required')
    remaining=publication_deadline-time.time()
    if remaining<.001:raise TimeoutError('original model publication deadline expired')
    last=[0.,False]
    def cancelled():
        if stopping():return True
        now=time.monotonic()
        if now-last[0]>=.1:
            row=db.REPO.job(active['id'])
            last[:]=[now,row['status']!='running' or row['token']!=active['token'] or
                row['owner']!=active['owner'] or not db.REPO.owns(active['owner'])]
        return last[1]
    owner.report('Running explicit native model contract',.1)
    if request['schema'] in {'commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'}:
        if request['schema']=='commercial-fee-journal-1':from portfolio_risk.core.commercial_fee_native import run
        elif request['schema']=='commercial-treasury-1':from portfolio_risk.core.commercial_treasury_native import run
        elif request['schema']=='commercial-holding-journal-1':from portfolio_risk.core.commercial_holding_native import run
        else:from portfolio_risk.core.commercial_candidate_native import run
        result,execution=run(request['request'],threads=request['threads'],expected_binary=request['binary_sha256'],cancelled=cancelled,timeout=min(request['timeout'],remaining))
    elif request['schema'] in {'strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'}:
        if request['schema']=='product-mixed-product-credit-1':
            from portfolio_risk.core import product_credit_mixed_native as runtime
        elif request['schema']=='product-mixed-consumer-credit-1':
            from portfolio_risk.core import consumer_mixed_native as runtime
        elif request['schema']=='product-mixed-dated-1':
            from portfolio_risk.core import product_mixed_native as runtime
        elif request['schema']=='product-candidate-journal-1':
            from portfolio_risk.core import product_candidate_native as runtime
        else:
            from portfolio_risk.core import product_strategy_native as runtime
        result,execution=runtime.run(request['request'],threads=request['threads'],
            expected_binary=request['binary_sha256'],cancelled=cancelled,timeout=request['timeout'])
    else:
        result,execution=model_native.run(request['schema'],request['request'],threads=request['threads'],
            expected_binary=request['binary_sha256'],cancelled=cancelled,timeout=min(request['timeout'],remaining) if commercial else request['timeout'])
    if cancelled():raise InterruptedError('model analysis publication cancelled')
    if source_identity()!=request['source_sha256']:raise RuntimeError('MODEL_VERSION_CHANGED: model source changed during execution')
    execution.update(request_manifest=active['request'],source_sha256=request['source_sha256'],publication_deadline_unix=publication_deadline)
    return result,execution


def publication_check(deadline,job,stopping):
    """Bound independent replay by the original deadline and durable ownership."""
    last=[0.]
    def check():
        if time.time()>=deadline:raise TimeoutError('model source publication deadline exceeded')
        if stopping():raise InterruptedError('model source publication cancelled')
        now=time.monotonic()
        if job is not None and now-last[0]>=.1:
            row=db.REPO.job(job['id']);last[0]=now
            if (row['status']!='running' or row['token']!=job['token'] or
                    row['owner']!=job['owner'] or not db.REPO.owns(job['owner'])):
                raise InterruptedError('model source publication ownership lost')
        return False
    return check


def verify_final_execution(request,*,deadline,job,stopping):
    """Fresh artifact and source fence at the final durable publication boundary."""
    if request.get('schema') in {'term-product-graph-2','term-product-reprice-2'}:
        from .cd_sequence_jobs import _fence,_budget
        return _fence(request,_budget(request,job,stopping,deadline))
    if request.get('schema')=='market-indexed-books-1':
        from .indexed_book_jobs import _budget,final_identity
        return final_identity(request,_budget(request,job,stopping,deadline=deadline))
    from portfolio_risk.analytics.balance_stream import source_identity, _hash
    check=publication_check(deadline,job,stopping)
    check()
    if 'specification' in request:
        from portfolio_risk.analytics import model_ledger, ledger_native
        for path,expected in [(model_ledger.binary_path(),request['binary_sha256']),
                              (ledger_native.library_path(),request['validation_library_sha256'])]:
            if not path.is_file() or _hash(path)!=expected:
                raise RuntimeError('MODEL_VERSION_CHANGED: modeled artifacts changed at final publication')
    else:
        if request['schema'] in {'strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'}:
            from portfolio_risk.core.product_strategy_native import binary_identity
        else:
            from portfolio_risk.core.model_native import binary_identity
        if binary_identity()!=request['binary_sha256']:
            raise RuntimeError('MODEL_VERSION_CHANGED: model artifact changed at final publication')
    if source_identity()!=request['source_sha256']:
        raise RuntimeError('MODEL_VERSION_CHANGED: model source changed at final publication')
    # Hashing is included in the original budget; use a fresh ownership probe.
    publication_check(deadline,job,stopping)()


def verify_source(request,result,check=lambda:False):
    from .commercial_selection import is_commercial_cohort
    if is_commercial_cohort(request['schema'],request['request']):
        from portfolio_risk.analytics.commercial_cohort_reference import verify
        return verify(request['request'],result,check)
    check()
    if request["schema"]=="commercial-fee-journal-1":
        from portfolio_risk.analytics.commercial_fee_reference import verify
        return verify(request["request"],result,check)
    if request["schema"]=="nonintegral-fee-accounting-1":
        from portfolio_risk.analytics.nonintegral_fee_reference import verify
        return verify(request["request"],result,check)
    if request["schema"]=="hedge-accounting-1":
        from portfolio_risk.analytics.hedge_accounting_reference import verify
        return verify(request["request"],result,check)
    if request['schema']=='commercial-holding-journal-1':
        from portfolio_risk.analytics.commercial_holding_reference import verify
        return verify(request['request'],result,check)
    if request['schema']=='commercial-holding-continuation-1':
        from portfolio_risk.analytics.commercial_continuation_reference import verify
        return verify(request['request'],result,check)
    if request['schema']=='commercial-candidate-journal-1':
        from portfolio_risk.analytics.commercial_candidate_reference import verify
        return verify(request['request'],result,check)
    if request['schema']=='commercial-generated-exercise-1':
        from portfolio_risk.analytics.commercial_stochastic_reference import verify
        verify(request['request'],result,check)
        return dict(verified=True,raw_source_recomputed=True,stopped_cash_replayed=True,training_evaluation_separated=True,physical_journal_eligible=False,empirical_validation=False,convergence_established=False)
    if request['schema']=='commercial-treasury-1':
        from portfolio_risk.analytics.commercial_treasury_reference import verify
        return verify(request['request'],result,check)
    if request['schema'] in {'commercial-product-graph-1','commercial-units-1'}:
        from portfolio_risk.analytics.commercial_graph_reference import verify_graph,verify_units
        verified=(verify_graph if request['schema']=='commercial-product-graph-1' else verify_units)(request['request'],result,check)
        return {key:value for key,value in verified.items() if type(value) is bool}
    if request['schema']=='conditional-exercise-1':
        from portfolio_risk.analytics.term_exercise_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema'] in {'term-product-graph-2','term-product-reprice-2'}:
        from portfolio_risk.analytics.cd_sequence_graph_reference import verify_request
        return verify_request(request['schema'],request['request'],result,check=check)
    if request['schema']=='product-mixed-product-credit-1':
        raw=request['request']
        if any(path['request']['kind']=='card_v2' for owner in raw['product_owners']
               for run in owner['runs'] for path in run['paths']):
            from portfolio_risk.analytics.product_rights_reference import verify
        else:
            from portfolio_risk.analytics.product_event_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='product-mixed-consumer-credit-1':
        from portfolio_risk.analytics.consumer_mixed_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='consumer-credit-accounting-1':
        from portfolio_risk.analytics.consumer_credit_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='card-credit-accounting-1':
        from portfolio_risk.analytics.card_credit_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='card-credit-accounting-2':
        from portfolio_risk.analytics.card_credit_v2_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='market-indexed-adapter-1':
        from portfolio_risk.analytics.market_indexed_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='mortgage-credit-accounting-1':
        from portfolio_risk.analytics.mortgage_credit_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='product-mixed-dated-1':
        from portfolio_risk.analytics.product_mixed_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='product-candidate-journal-1':
        from portfolio_risk.analytics.product_candidate_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='product-units-1':
        from portfolio_risk.analytics.product_strategy_reference import verify_preparation
        return verify_preparation(request['request'],result,check=check)
    if request['schema']=='strategy-product-1':
        from portfolio_risk.analytics.product_strategy_reference import verify
        return verify(request['request'],result,check=check)
    if request['schema']=='derivative-accounting-1':
        from portfolio_risk.analytics.derivative_verification import verify_derivative_accounting
        return verify_derivative_accounting(request['request'],result)
    if request['schema']=='credit-path-accounting-1':
        from portfolio_risk.analytics.credit_path_reference import account_path
        from portfolio_risk.analytics.credit_path_verification import equal
        raw=request['request'];bindings={b['path_key']:b for b in raw['bindings']}
        expected=[account_path(raw,p,bindings[p['path_key']]) for p in raw['path']['paths']]
        equal(result['paths'],expected)
        metadata=dict(schema='credit-path-accounting-1',day_mapping=raw['day_mapping'],measure=raw['path']['measure'],monetary_unit=raw['path']['monetary_unit'],reporting_end_ordinal=raw['reporting_end_ordinal'],source_id=raw['path']['source_id'],source_revision=raw['path']['source_revision'])
        equal({k:v for k,v in result.items() if k not in {'paths','work_units'}},metadata)
        if type(result['work_units']) is not int or not 0<=result['work_units']<=2000000:raise ValueError('invalid native accounting work receipt')
        return dict(verified=True,physical_paths_recomputed=True,persisted_journal_verified=False)
    if request['schema']=='joint-state-1':
        return _verify_joint_state(request['request'], result)
    if request['schema']=='joint-market-credit-1':
        return _verify_joint_market_credit(request['request'], result)
    if request['schema']=='market-driver-calibration-1':
        return _verify_market_driver_calibration(request['request'], result)
    if request['schema']=='market-quote-bootstrap-1':
        return _verify_market_quote_bootstrap(request['request'], result, check=check)
    if request['schema']=='observed-deposit-calibration-1':
        return _verify_observed_deposit_calibration(request['request'], result, check=check)
    return None



def _verify_joint_state(raw, result):
    """Independent replay of the bounded joint-state coordinator.

    This is a publication oracle only. It never supplies production model
    outputs or changes the Rust-selected assumptions.
    """
    import math
    if result.get('schema')!='joint-state-1' or result.get('identity')!=raw['identity'] or result.get('source_status')!='supplied_joint_market_behavior_credit_drivers':
        raise ValueError('joint-state result identity mismatch')
    periods=raw['periods']; paths=raw['paths']
    if result.get('work_units')!=len(periods)*len(paths):
        raise ValueError('joint-state work receipt mismatch')
    if len(result.get('paths',[]))!=len(paths):raise ValueError('joint-state path count mismatch')
    def close(a,b,name):
        if not math.isclose(float(a),float(b),rel_tol=2e-12,abs_tol=2e-12):
            raise ValueError(f'joint-state {name} differs from independent replay')
    for raw_path,out_path in zip(paths,result['paths']):
        if out_path.get('position_id')!=raw_path['position_id'] or out_path.get('path_key')!=raw_path['path_key']:
            raise ValueError('joint-state path identity mismatch')
        if len(out_path.get('events',()))!=len(periods):raise ValueError('joint-state event count mismatch')
        balance=float(raw_path['principal']); totals=[0.,0.,0.,0.,0.]; discounted=0.
        for period,pair,event in zip(periods,raw_path['random_pairs'],out_path.get('events',())):
            years=(period['end_ordinal']-period['start_ordinal'])/365.
            hazards=[period['behavioral_exit_hazard_per_year'],period['credit_default_hazard_per_year'],period['prepayment_hazard_per_year']]
            total=sum(hazards); mass=0. if total==0. else -math.expm1(-total*years)
            probs=[0.,0.,0.] if total==0. else [mass*h/total for h in hazards]
            before=balance; kind='continue'; after=before; recovery=loss=0.
            if before>0.:
                u=pair['waiting_uniform']
                if u<probs[0]: kind='behavioral_exit';after=0.;totals[0]+=before
                elif u<probs[0]+probs[1]: kind='credit_default';after=0.;recovery=before*period['recovery_fraction'];loss=before-recovery;totals[1]+=before;totals[3]+=recovery;totals[4]+=loss
                elif u<sum(probs): kind='prepayment';after=0.;totals[2]+=before
                else: discounted+=before*period['market_discount_factor']
            if event['kind']!=kind or event['start_ordinal']!=period['start_ordinal'] or event['end_ordinal']!=period['end_ordinal']:
                raise ValueError('joint-state event differs from independent replay')
            close(event['principal_before'],before,'principal_before');close(event['principal_after'],after,'principal_after')
            close(event['recovery_cash'],recovery,'recovery_cash');close(event['economic_loss'],loss,'economic_loss')
            close(event['market_discount_factor'],period['market_discount_factor'],'market_discount_factor')
            close(event['recovery_lag_years'],period['recovery_lag_years'],'recovery_lag_years')
            for key,expected in zip(('behavioral_exit_probability','credit_default_probability','prepayment_probability'),probs):close(event[key],expected,key)
            balance=after
        close(out_path['closing_principal'],balance,'closing_principal')
        for key,expected in zip(('total_behavioral_exit','total_credit_default','total_prepayment','total_recovery_cash','total_economic_loss'),totals):close(out_path[key],expected,key)
        close(out_path['discounted_live_balance'],discounted,'discounted_live_balance')
    return dict(verified=True,joint_paths_recomputed=len(paths),market_drivers_retained=True,
                common_random_numbers_id=raw['identity']['common_random_numbers_id'])


def _verify_joint_market_credit(raw, result):
    """Replay market-linked hazards and competing exits independently.

    The market covariance implementation is verified by the native/reference
    model-market suites. This publication oracle independently consumes the
    persisted market tape, recomputes hazard elasticities and replays every
    path event before publication.
    """
    import datetime
    import math
    if (result.get('schema') != 'joint-market-credit-1'
            or result.get('model_version') != 'joint-market-credit-1'
            or result.get('identity') != raw['identity']
            or result.get('source_status') != 'generated_market_drivers_with_supplied_hazard_elasticities'
            or result.get('adoption_eligible') is not False):
        raise ValueError('joint market-credit result identity/status mismatch')
    market = result.get('market', {})
    source_market = raw['market']
    if (market.get('source_revision') != source_market['source_revision']
            or market.get('shared_draw_id') != source_market['shared_draw_id']
            or market.get('drivers') != source_market['drivers']
            or market.get('durations_years') != source_market['durations_years']):
        raise ValueError('joint market-credit market identity mismatch')
    periods, positions = raw['periods'], raw['positions']
    drivers = len(source_market['drivers']); steps = len(periods); paths = len(positions)
    shape = market.get('shape')
    if shape != [paths, steps, drivers]:
        raise ValueError('joint market-credit market shape mismatch')
    increments = market.get('increments', [])
    if len(increments) != paths * steps * drivers:
        raise ValueError('joint market-credit increment count mismatch')
    if result.get('work_units') != paths * steps or len(result.get('paths', [])) != paths:
        raise ValueError('joint market-credit work/path receipt mismatch')

    def years(start, end, convention):
        if convention == 'act360': return (end - start) / 360.0
        if convention == 'act365_fixed': return (end - start) / 365.0
        if convention != 'act_act_isda': raise ValueError('unsupported day-count in verifier')
        cursor = datetime.date.fromordinal(start); finish = datetime.date.fromordinal(end)
        total = 0.0
        while cursor < finish:
            next_year = datetime.date(cursor.year + 1, 1, 1)
            boundary = min(next_year, finish)
            total += (boundary - cursor).days / (366.0 if cursor.year % 4 == 0 and (cursor.year % 100 != 0 or cursor.year % 400 == 0) else 365.0)
            cursor = boundary
        return total

    def close(a, b, name):
        if not math.isclose(float(a), float(b), rel_tol=3e-12, abs_tol=3e-12):
            raise ValueError(f'joint market-credit {name} differs from independent replay')

    def hazard(base, beta, annual, name):
        if base == 0.0:
            if any(value != 0.0 for value in beta):
                raise ValueError(f'{name} zero baseline has nonzero beta')
            return 0.0
        value = math.exp(math.log(base) + sum(coefficient * shock for coefficient, shock in zip(beta, annual)))
        if not math.isfinite(value) or value > 100.0:
            raise ValueError(f'{name} outside admitted domain')
        return value

    annual_samples = []
    terminal = [0.0] * (paths * drivers)
    for path_index in range(paths):
        for step, dt in enumerate(source_market['durations_years']):
            offset = (path_index * steps + step) * drivers
            annual = [float(increments[offset + j]) / math.sqrt(float(dt)) for j in range(drivers)]
            annual_samples.append(annual)
            for driver in range(drivers):
                terminal[path_index * drivers + driver] += float(increments[offset + driver])
    for actual, expected in zip(market.get('terminal_changes', []), terminal):
        close(actual, expected, 'terminal_changes')
    if len(market.get('terminal_changes', [])) != len(terminal):
        raise ValueError('joint market-credit terminal shape mismatch')
    samples = float(len(annual_samples))
    sample_mean = [sum(row[j] for row in annual_samples) / samples for j in range(drivers)]
    sample_cov = []
    for j in range(drivers):
        for k in range(drivers):
            sample_cov.append(sum((row[j] - sample_mean[j]) * (row[k] - sample_mean[k]) for row in annual_samples) / samples)
    for actual, expected in zip(market.get('sample_mean_annualized_innovation', []), sample_mean):
        close(actual, expected, 'sample_mean_annualized_innovation')
    for actual, expected in zip(market.get('sample_annual_covariance', []), sample_cov):
        close(actual, expected, 'sample_annual_covariance')
    if len(market.get('sample_mean_annualized_innovation', [])) != drivers or len(market.get('sample_annual_covariance', [])) != drivers * drivers:
        raise ValueError('joint market-credit sample diagnostic shape mismatch')
    diagnostics = market.get('diagnostics', {})
    eigenvalues = diagnostics.get('correlation_eigenvalues_descending', [])
    if len(eigenvalues) != drivers or any(not math.isfinite(float(value)) or float(value) < -1e-10 for value in eigenvalues):
        raise ValueError('joint market-credit covariance eigenvalue diagnostics are invalid')
    close(sum(float(value) for value in eigenvalues), float(drivers), 'correlation_eigenvalue_trace')
    retained = diagnostics.get('retained_annual_covariance', [])
    if len(retained) != drivers * drivers or any(not math.isfinite(float(value)) for value in retained):
        raise ValueError('joint market-credit retained covariance diagnostics are invalid')
    for key in ('roundoff_negative_eigenvalue', 'roundoff_correlation_bound_adjustment', 'discarded_covariance_frobenius'):
        if not math.isfinite(float(diagnostics.get(key, float('nan')))):
            raise ValueError(f'joint market-credit diagnostic {key} is invalid')

    for path_index, (position, output) in enumerate(zip(positions, result['paths'])):
        if output.get('position_id') != position['position_id'] or output.get('path_key') != position['path_key']:
            raise ValueError('joint market-credit path identity mismatch')
        if len(output.get('events', [])) != steps:
            raise ValueError('joint market-credit event count mismatch')
        balance = float(position['principal'])
        totals = [0.0, 0.0, 0.0, 0.0, 0.0]; discounted = 0.0
        for step, (period, event) in enumerate(zip(periods, output['events'])):
            check_dt = years(period['start_ordinal'], period['end_ordinal'], raw['day_count'])
            dt = float(source_market['durations_years'][step])
            close(dt, check_dt, f'duration[{step}]')
            offset = (path_index * steps + step) * drivers
            annual = [float(increments[offset + j]) / math.sqrt(dt) for j in range(drivers)]
            hb = hazard(period['behavioral_baseline_hazard_per_year'], period['behavioral_beta'], annual, 'behavioral')
            hc = hazard(period['credit_baseline_hazard_per_year'], period['credit_beta'], annual, 'credit')
            hp = hazard(period['prepayment_baseline_hazard_per_year'], period['prepayment_beta'], annual, 'prepayment')
            total = hb + hc + hp
            mass = 0.0 if total == 0.0 else -math.expm1(-total * check_dt)
            probabilities = [0.0, 0.0, 0.0] if total == 0.0 else [mass * value / total for value in (hb, hc, hp)]
            before = balance; after = before; recovery = 0.0; loss = 0.0; kind = 'continue'
            if before > 0.0:
                u = position['random_pairs'][step]['waiting_uniform']
                if u < probabilities[0]: kind = 'behavioral_exit'; after = 0.0; totals[0] += before
                elif u < probabilities[0] + probabilities[1]:
                    kind = 'credit_default'; after = 0.0; recovery = before * period['recovery_fraction']; loss = before - recovery
                    totals[1] += before; totals[3] += recovery; totals[4] += loss
                elif u < sum(probabilities): kind = 'prepayment'; after = 0.0; totals[2] += before
                else: discounted += before * period['market_discount_factor']
            if event.get('kind') != kind or event.get('start_ordinal') != period['start_ordinal'] or event.get('end_ordinal') != period['end_ordinal']:
                raise ValueError('joint market-credit event differs from independent replay')
            for key, expected in [('principal_before', before), ('principal_after', after), ('recovery_cash', recovery), ('economic_loss', loss), ('market_discount_factor', period['market_discount_factor']), ('recovery_lag_years', period['recovery_lag_years'])]:
                close(event[key], expected, key)
            for key, expected in zip(('behavioral_exit_probability', 'credit_default_probability', 'prepayment_probability'), probabilities):
                close(event[key], expected, key)
            balance = after
        close(output['closing_principal'], balance, 'closing_principal')
        for key, expected in zip(('total_behavioral_exit', 'total_credit_default', 'total_prepayment', 'total_recovery_cash', 'total_economic_loss'), totals):
            close(output[key], expected, key)
        close(output['discounted_live_balance'], discounted, 'discounted_live_balance')
    return dict(verified=True, market_tape_replayed=True, hazard_elasticities_recomputed=True,
                competing_paths_recomputed=paths, adoption_eligible=False)


def _verify_market_driver_calibration(raw, result):
    """Independently replay training moments and chronological diagnostics."""
    import math
    if (result.get('schema') != 'market-driver-calibration-1'
            or result.get('source_status') != 'observed_rows_training_moments_chronological_holdout'):
        raise ValueError('market-driver calibration schema mismatch')
    for key in ('model_id','model_version','input_revision','source_id','source_revision',
                'training_split_id','holdout_split_id','training_end'):
        if result.get(key) != raw.get(key):
            raise ValueError(f'market-driver {key} identity mismatch')
    dates, values, cutoff = raw['dates'], raw['values'], raw['training_end']
    n = sum(d <= cutoff for d in dates)
    held = len(values) - n
    if n < 30 or held < 10 or any(dates[i] >= dates[i + 1] for i in range(len(dates) - 1)):
        raise ValueError('market-driver chronological split is invalid')
    train, hold = values[:n], values[n:]
    mean = [sum(row[j] for row in train) / n for j in range(4)]
    cov = [[sum((row[j] - mean[j]) * (row[k] - mean[k]) for row in train) / (n - 1)
            for k in range(4)] for j in range(4)]
    bias = [sum(row[j] for row in hold) / held - mean[j] for j in range(4)]
    rmse = [math.sqrt(sum((row[j] - mean[j]) ** 2 for row in hold) / held) for j in range(4)]
    outside = [sum(abs(row[j] - mean[j]) / math.sqrt(max(cov[j][j], 1e-16)) > 3.
                   for row in hold) / held for j in range(4)]
    diag = result.get('diagnostics', {})
    if diag.get('training_rows') != n or diag.get('holdout_rows') != held:
        raise ValueError('market-driver calibration split counts differ from source')
    lag = raw.get('uncertainty_lag') or 0
    if type(lag) is not int or lag < 0 or lag >= n or lag > 64:
        raise ValueError('market-driver uncertainty lag is invalid')
    uncertainty = diag.get('uncertainty', {})
    if (uncertainty.get('method') != 'bartlett_hac_mean_standard_error'
            or uncertainty.get('lag') != lag
            or uncertainty.get('training_rows') != n
            or uncertainty.get('serial_dependence_adjusted') != (lag > 0)):
        raise ValueError('market-driver uncertainty metadata differs from source')
    standard_error = []
    for j in range(4):
        gamma0 = sum((row[j] - mean[j]) ** 2 for row in train) / n
        long_run = gamma0
        for distance in range(1, lag + 1):
            gamma = sum((left[j] - mean[j]) * (right[j] - mean[j])
                        for left, right in zip(train[distance:], train[:n - distance])) / n
            long_run += 2.0 * (lag + 1 - distance) / (lag + 1) * gamma
        if not math.isfinite(long_run) or long_run < -1e-12 * max(gamma0, 1.0):
            raise ValueError('market-driver HAC variance is invalid')
        standard_error.append(math.sqrt(max(long_run, 0.0) / n))
    def close(a, b, name):
        if not math.isclose(float(a), float(b), rel_tol=3e-12, abs_tol=3e-12):
            raise ValueError(f'market-driver {name} differs from independent replay')
    for j in range(4):
        close(diag['mean'][j], mean[j], f'mean[{j}]')
        close(diag['holdout_bias'][j], bias[j], f'bias[{j}]')
        close(diag['holdout_rmse'][j], rmse[j], f'rmse[{j}]')
        close(diag['holdout_outside_three_sigma'][j], outside[j], f'outside[{j}]')
        close(uncertainty['mean_standard_error'][j], standard_error[j], f'mean_standard_error[{j}]')
        for k in range(4):
            close(diag['covariance'][j][k], cov[j][k], f'covariance[{j},{k}]')
    selected = diag.get('selected', [])
    if (not isinstance(selected, list) or len(selected) > 8
            or any(type(i) is not int or i < 0 or i >= n for i in selected)):
        raise ValueError('market-driver selected scenarios are invalid')
    if result.get('adoption_eligible') is not False:
        raise ValueError('market-driver calibration cannot be adoption eligible')
    return dict(verified=True, training_moments_recomputed=True,
                chronological_holdout_recomputed=True, selected_count=len(selected),
                adoption_eligible=False)


def _verify_market_quote_bootstrap(raw, result, check=lambda: False):
    """Independently replay explicit-schedule par bootstrap and quote residuals."""
    import math
    check()
    if (result.get('schema') != 'market-quote-bootstrap-1'
            or result.get('model_version') != 'market-quote-bootstrap-1'
            or result.get('source_status') != 'source_linked_explicit_schedule_par_bootstrap'
            or result.get('adoption_eligible') is not False):
        raise ValueError('market quote bootstrap schema/status mismatch')
    for key in ('source_id', 'source_revision', 'curve_id'):
        if result.get(key) != raw.get(key):
            raise ValueError(f'market quote {key} identity mismatch')
    bootstrap = raw['bootstrap_quotes']; validation = raw['validation_quotes']
    if not bootstrap or len(bootstrap) > 256 or len(validation) > 256:
        raise ValueError('market quote bootstrap admission mismatch')
    ordered = sorted(bootstrap, key=lambda quote: (quote['payments'][-1]['time_years'], quote['quote_id']))
    times, discounts = [0.0], [1.0]
    expected = []
    def close(a, b, name):
        if (type(a) not in (int, float) or not math.isfinite(a)
                or not math.isclose(a, b, rel_tol=3e-12, abs_tol=3e-12)):
            raise ValueError(f'market quote {name} differs from independent replay')
    def lookup(time):
        for pillar, discount in zip(times, discounts):
            if abs(pillar - time) <= 1e-12 * max(abs(pillar), abs(time), 1.0):
                return discount
        raise ValueError('market quote payment is not an existing pillar')
    def model(quote):
        annuity = 0.0; final = None
        for payment in quote['payments']:
            discount = lookup(payment['time_years'])
            annuity += payment['accrual_fraction'] * discount
            final = discount
        rate = (1.0 - final) / annuity
        return rate, rate * annuity + final
    for quote in ordered:
        check()
        known = sum(payment['accrual_fraction'] * lookup(payment['time_years'])
                    for payment in quote['payments'][:-1])
        last = quote['payments'][-1]
        discount = (1.0 - quote['par_rate'] * known) / (1.0 + quote['par_rate'] * last['accrual_fraction'])
        times.append(last['time_years']); discounts.append(discount)
        rate, unit_pv = model(quote)
        expected.append(dict(quote_id=quote['quote_id'], kind='bootstrap',
                             observed_par_rate=quote['par_rate'], model_par_rate=rate,
                             residual=rate - quote['par_rate'], unit_par_pv=unit_pv))
    for quote in validation:
        check()
        rate, unit_pv = model(quote)
        expected.append(dict(quote_id=quote['quote_id'], kind='validation',
                             observed_par_rate=quote['par_rate'], model_par_rate=rate,
                             residual=rate - quote['par_rate'], unit_par_pv=unit_pv))
    if result.get('pillar_times_years') != times or len(result.get('discount_factors', [])) != len(discounts):
        raise ValueError('market quote curve pillars differ from independent replay')
    for actual, expected_discount in zip(result['discount_factors'], discounts):
        close(actual, expected_discount, 'discount factor')
    actual_quotes = result.get('quote_diagnostics', [])
    if len(actual_quotes) != len(expected):
        raise ValueError('market quote diagnostic count differs from source')
    for actual, expected_quote in zip(actual_quotes, expected):
        check()
        for key in ('quote_id', 'kind'):
            if actual.get(key) != expected_quote[key]:
                raise ValueError('market quote diagnostic identity mismatch')
        for key in ('observed_par_rate', 'model_par_rate', 'residual', 'unit_par_pv'):
            close(actual[key], expected_quote[key], key)
    bootstrap_residual = max(abs(row['residual']) for row in expected if row['kind'] == 'bootstrap')
    diagnostics = result.get('diagnostics', {})
    if (diagnostics.get('method') != 'sequential_closed_form_par_bootstrap'
            or diagnostics.get('bootstrap_quote_count') != len(bootstrap)
            or diagnostics.get('converged') is not True):
        raise ValueError('market quote convergence receipt mismatch')
    close(diagnostics.get('max_bootstrap_abs_residual'), bootstrap_residual, 'max bootstrap residual')
    if (type(diagnostics.get('bootstrap_quote_count')) is not int
            or diagnostics.get('residual_tolerance') != 2e-12
            or bootstrap_residual > 2e-12):
        raise ValueError('market quote residual gate differs from independent replay')
    work = sum(len(quote['payments']) for quote in bootstrap + validation)
    if type(result.get('work_units')) is not int or result['work_units'] != work:
        raise ValueError('market quote work count differs from source')
    check()
    return dict(verified=True, explicit_schedule_replayed=True,
                quote_residuals_recomputed=len(expected), adoption_eligible=False)


def _verify_observed_deposit_calibration(raw, result, check=lambda: False):
    from portfolio_risk.analytics.deposit_calibration_reference import verify
    return verify(raw, result, check=check)



def verify_persisted(request,result,*,expected_deadline,request_manifest=None,job=None,stopping=lambda:False):
    if request['schema'] in {'term-product-graph-2','term-product-reprice-2'}:
        from .cd_sequence_jobs import verify_persisted as cd_verify
        return cd_verify(request,result,expected_deadline=expected_deadline,request_manifest=request_manifest,job=job,stopping=stopping)
    if request['schema']=='market-indexed-books-1':
        from .indexed_book_jobs import verify_persisted as indexed_verify
        return indexed_verify(request,result,expected_deadline=expected_deadline,request_manifest=request_manifest,job=job,stopping=stopping)
    from .model_tables import restore_sources
    from .commercial_selection import is_commercial_cohort
    commercial=is_commercial_cohort(request['schema'],request['request']) or request['schema'] in {'commercial-product-graph-1','commercial-units-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1','commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1'}
    execution=result['execution'];deadline=execution.get('publication_deadline_unix')
    if 'prior_opening_verification'in execution and 'prior_publication'not in request:
        raise ValueError('unbound prior-publication evidence')
    if isinstance(deadline,bool) or not isinstance(deadline,(int,float)) or not math.isfinite(deadline):
        raise ValueError('invalid model source publication deadline')
    if deadline!=expected_deadline:raise ValueError('persisted model publication deadline differs from original budget')
    envelope=dict(schema=request['schema'],threads=request['threads'],request=request['request'])
    digest=hashlib.sha256(json.dumps(envelope,allow_nan=False,separators=(',',':')).encode()).hexdigest()
    expected=dict(schema=request['schema'],backend='rust',input_sha256=digest,
        binary_sha256=request['binary_sha256'],source_sha256=request['source_sha256'])
    if request['schema'] in {'strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'}:
        executed=request['request']
        if request['schema']=='product-mixed-product-credit-1':
            from portfolio_risk.core.product_credit_mixed_native import command
            executed=command(executed,timeout=request['timeout'])
        elif request['schema']=='product-mixed-consumer-credit-1':
            from portfolio_risk.core.consumer_mixed_native import command
            executed=command(executed,timeout=request['timeout'])
        elif request['schema']=='product-mixed-dated-1':
            from portfolio_risk.core.product_mixed_native import command
            executed=command(executed,timeout=request['timeout'])
        elif request['schema']=='product-candidate-journal-1':
            from portfolio_risk.core.product_candidate_native import command
            executed=command(executed,timeout=request['timeout'])
        expected['executed_input_sha256']=hashlib.sha256(json.dumps(executed,allow_nan=False,separators=(',',':')).encode()).hexdigest()
    if request['schema'] in {'commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'}:
        from .commercial_execution_envelope import retained_command
        milliseconds=execution.get('native_timeout_ms')
        executed=retained_command(request['schema'],request['request'],request['timeout'],milliseconds)
        expected['executed_input_sha256']=hashlib.sha256(json.dumps(executed,allow_nan=False,separators=(',',':')).encode()).hexdigest()
    if request['schema'] in {'commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1'}:
        milliseconds=execution.get('native_timeout_ms')
        if type(milliseconds)is not int or not 1<=milliseconds<=int(request['timeout']*1000):raise ValueError('generated original native budget identity differs')
        executed=dict(envelope,execution_timeout_ms=milliseconds)
        expected['executed_input_sha256']=hashlib.sha256(json.dumps(executed,allow_nan=False,separators=(',',':')).encode()).hexdigest()
    if any(execution.get(key)!=value for key,value in expected.items()):
        raise ValueError('persisted model execution identity differs from queued raw envelope')
    if request_manifest is not None and (not _mortgage_json_exact(execution.get('request_manifest'),request_manifest) if commercial else execution.get('request_manifest')!=request_manifest):
        raise ValueError('persisted model queued request manifest differs')
    check=publication_check(deadline,job,stopping);check()
    commercial_cohort=is_commercial_cohort(request['schema'],request['request'])
    if commercial_cohort or request['schema'] in {'commercial-product-graph-1','commercial-units-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1','commercial-generated-exercise-1','commercial-holding-continuation-1','nonintegral-fee-accounting-1','hedge-accounting-1','conditional-exercise-1','market-indexed-adapter-1','market-driver-calibration-1','market-quote-bootstrap-1','observed-deposit-calibration-1','joint-market-credit-1','derivative-accounting-1','credit-path-accounting-1','joint-state-1','consumer-credit-accounting-1','card-credit-accounting-1','card-credit-accounting-2','mortgage-credit-accounting-1','strategy-product-1','product-units-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'}:
        if not result['execution'].get('source_verification'):raise ValueError('missing independent source verification receipt')
        restored=restore_sources(result['model_result'],result['execution']['table_layouts'])
        if commercial:
            from .model_tables import format_sources
            _,layouts=format_sources(restored)
            if not _mortgage_json_exact(layouts,execution['table_layouts']):raise ValueError('commercial persisted source layouts differ')
        canonical=lambda value:json.dumps(value,allow_nan=False,sort_keys=True,separators=(',',':'))
        if canonical(verify_source(request,restored,check=check))!=canonical(execution['source_verification']):
            raise ValueError('persisted independent model source receipt differs')
        if canonical(execution['summary'])!=canonical(source_summary(restored)):
            raise ValueError('persisted model summary differs from verified source')
    if time.time()>=deadline:raise TimeoutError('model source publication deadline exceeded')
    if request['schema'] in {'strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1','commercial-treasury-1','commercial-candidate-journal-1','commercial-holding-journal-1','commercial-fee-journal-1'}:
        from portfolio_risk.core.product_strategy_native import binary_identity
    else:
        from portfolio_risk.core.model_native import binary_identity
    from portfolio_risk.analytics.balance_stream import source_identity
    if binary_identity()!=request['binary_sha256'] or source_identity()!=request['source_sha256']:
        raise RuntimeError('MODEL_VERSION_CHANGED: model source changed before publication')
    if time.time()>=deadline:raise TimeoutError('model source publication deadline exceeded')


def run(owner,request,stopping):
    if request['schema'] in {'term-product-graph-2','term-product-reprice-2'}:
        from .cd_sequence_jobs import run as cd_run
        return cd_run(owner,request,stopping)
    if request['schema']=='market-indexed-books-1':
        from .indexed_book_jobs import run as indexed_run
        return indexed_run(owner,request,stopping)
    prior=None
    if 'prior_publication'in request:
        if request['schema']!='commercial-holding-journal-1':raise ValueError('private prior binding requires native holding contract')
        from . import commercial_prior_source as source
        check=publication_check(request['publication_deadline_unix'],db.ACTIVE_JOB.get(),stopping)
        expected=source.materialize(request['prior_publication'],request['request']['revised_sources'],request['request']['cut_ordinal'])
        from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
        if not exact_json_identity(expected,request['request']):raise ValueError('server-bound prior holding input changed')
        prior=source.reopen(db.REPO,db.CODEC,request['prior_publication'],check)
    result,execution=native_result(owner,request,stopping)
    from .model_tables import format_sources
    check=publication_check(execution['publication_deadline_unix'],db.ACTIVE_JOB.get(),stopping)
    receipt=verify_source(request,result,check=check)
    if prior is not None:
        prior.match(result,check);execution['prior_opening_verification']=prior.receipt()
    check()
    formatted,layouts=format_sources(result)
    execution['table_layouts']=layouts
    if receipt:execution['source_verification']=receipt
    execution.update(empirical_validation=False,
        summary=source_summary(result),
        scope='Explicit analytical contract; saved-book, cohort, strategy and daily adoption require their own acceptance gates')
    owner.report('Native model source report complete',1.)
    return dict(model_result=formatted,execution=execution,revision=owner.current_state()['revision'])


def build_aged(owner,request,stopping):
    """Explicit dated library adoption; no monthly backbook baseline is mixed in."""
    if request['schema']=='product-units-1':
        from portfolio_risk.strategy.product import library_from_result
    elif request['schema']=='commercial-units-1':
        from portfolio_risk.strategy.commercial import library_from_result
    elif request['schema']=='aged-units-1':
        from portfolio_risk.strategy.aged import library_from_result
    else:raise ValueError('dated native unit contract required')
    raw=request['request']
    result,execution=native_result(owner,request,stopping)
    source_report={}
    if request['schema'] in {'product-units-1','commercial-units-1'}:
        check=publication_check(execution['publication_deadline_unix'],db.ACTIVE_JOB.get(),stopping)
        execution['source_verification']=verify_source(request,result,check=check)
        execution['summary']=source_summary(result)
        from .model_tables import format_sources
        formatted,execution['table_layouts']=format_sources(result)
        source_report['model_result']=formatted
        check()
    library=library_from_result(raw,result)
    state=owner.current_state()
    with owner._LOCK:
        db.assert_current(state['revision'])
        if owner.STATE_META['revision']!=state['revision']:
            raise RuntimeError('inputs changed while building the dated library')
        owner.CACHE.update(revision=state['revision'],library=library,base=None,compute_backend='rust')
        owner.UNITLIB,owner.BASE_KPIS=library,None
    return dict(units=len(library['units']),templates=list(library['templates']),grid_m=library['grid_m'],
        horizon_months=library['horizon'],reporting_ordinals=library['reporting_ordinals'],
        preparation_schema=request['schema'],revision=state['revision'],execution=execution,
        scope='Actual-date incremental allocation coefficients; saved-book KPI baseline and daily acceptance require separate adoption',
        **source_report)


def verify_product_library(request,result,saved,*,deadline,job,stopping):
    """Compare persisted adapter cells with the independently checked native source."""
    import numpy as np
    from .model_tables import restore_sources
    if request['schema']=='commercial-units-1':
        from portfolio_risk.strategy.commercial import library_from_result
    else:
        from portfolio_risk.strategy.product import library_from_result
    check=publication_check(deadline,job,stopping)
    raw=restore_sources(result['model_result'],result['execution']['table_layouts'])
    expected=library_from_result(request['request'],raw)
    def equal(actual,reference):
        check()
        if isinstance(reference,np.ndarray):
            if not isinstance(actual,np.ndarray) or actual.dtype!=reference.dtype or not np.array_equal(actual,reference):
                raise ValueError('persisted product library coefficient cells differ from verified source')
        elif isinstance(reference,dict):
            if not isinstance(actual,dict) or actual.keys()!=reference.keys():raise ValueError('persisted product library fields differ')
            for key,value in reference.items():equal(actual[key],value)
        elif isinstance(reference,list):
            if not isinstance(actual,list) or len(actual)!=len(reference):raise ValueError('persisted product library sequence differs')
            for a,b in zip(actual,reference,strict=True):equal(a,b)
        elif type(actual) is not type(reference) or actual!=reference:
            raise ValueError('persisted product library identity/source cells differ')
    equal(saved['cache'],dict(revision=result['revision'],library=expected,base=None,compute_backend='rust'))
    if saved['identity']!=db.identity():raise RuntimeError('MODEL_VERSION_CHANGED: persisted product library identity differs')
    verify_persisted(request,result,expected_deadline=deadline,request_manifest=job['request'],job=job,stopping=stopping)


from portfolio_risk.analytics.modeled_json_identity import exact_json_identity as _mortgage_json_exact

def verify_modeled_persisted(request, result, *, job, stopping, expected_deadline):
    """Recheck immutable stored bytes against the queued raw source before SQL commit.

    Existing independent scalar verifiers consume one persisted partition at a
    time. This never calls a product pricer or hydrates a complete journal.
    """
    import io
    import json
    import math
    import polars as pl
    from . import persistence as db
    from portfolio_risk.analytics import model_ledger
    from portfolio_risk.analytics.balance_stream import _hash, source_identity, _finalize
    from portfolio_risk.analytics.ledger_native import library_path, validate_spec
    from portfolio_risk.analytics.dated_ledger import verify_dated_sources
    from portfolio_risk.analytics.credit_path_verification import verify_sources

    execution = result['execution']; raw = request['specification']; model = raw['schema']
    mortgage = model == 'mortgage-mixed-accounting-1'
    revised = model == 'deposit-revised-income-ledger-1'
    continuation = model in {'deposit-performing-income-ledger-1','deposit-performing-portfolio-ledger-2'}
    commercial = model == 'commercial-credit-ledger-1'
    strict_modeled = mortgage or revised or continuation or commercial
    if model not in model_ledger.SCHEMAS or execution.get('model_version') != model:
        raise ValueError('persisted modeled ledger schema differs from queued source')
    deadline = execution.get('publication_deadline_unix')
    if isinstance(deadline, bool) or not isinstance(deadline, (int, float)) or not math.isfinite(deadline):
        raise ValueError('missing modeled ledger publication deadline')
    if (not _mortgage_json_exact(deadline,expected_deadline) if strict_modeled else deadline!=expected_deadline):raise ValueError('persisted modeled publication deadline differs from original budget')
    if job is None or (not _mortgage_json_exact(execution.get('request_manifest'),job['request']) if continuation or commercial else execution.get('request_manifest') != job['request']):
        raise ValueError('modeled publication requires its immutable queued request manifest')
    last_owner_read = [float('-inf')]
    def runtime_probe():
        if time.time() >= deadline:raise TimeoutError('modeled ledger publication deadline exceeded')
        if stopping():raise InterruptedError('modeled ledger publication cancelled')
    def check(force=False):
        runtime_probe()
        now = time.monotonic()
        # Mortgage replay probes every scalar. Querying SQLite twice for every
        # value can consume the worker lease before a small verified journal
        # closes. Keep runtime probes immediate and force ownership at every
        # retained-object/oracle/publication boundary.
        if job is not None and (not strict_modeled or force or now - last_owner_read[0] >= .1):
            row = db.REPO.job(job['id'])
            if (row['status'] != 'running' or row['token'] != job['token'] or
                    row['owner'] != job['owner'] or not db.REPO.owns(job['owner'])):
                raise InterruptedError('modeled ledger publication ownership lost')
            last_owner_read[0] = now
        runtime_probe()
        return False
    def identities():
        check(True)
        for path, expected in [(model_ledger.binary_path(), request['binary_sha256']),
                               (library_path(), request['validation_library_sha256'])]:
            if not path.is_file() or _hash(path) != expected:
                raise RuntimeError('MODEL_VERSION_CHANGED: modeled artifacts changed before publication')
        if (execution.get('binary_sha256') != request['binary_sha256'] or
                execution.get('validation_library_sha256') != request['validation_library_sha256']):
            raise RuntimeError('MODEL_VERSION_CHANGED: persisted modeled artifact identity differs')
        if source_identity() != request['source_sha256'] or execution.get('source_sha256') != request['source_sha256']:
            raise RuntimeError('MODEL_VERSION_CHANGED: modeled source changed before publication')
        check(True)
    identities()
    for key,limit in [('input',1024*1024 if strict_modeled else 32*1024*1024),('modeled_source',32*1024*1024 if strict_modeled else 128*1024*1024)]:
        ref=execution[key]
        if ref.get('format')!='bin' or type(ref.get('bytes')) is not int or not 0<ref['bytes']<=limit:
            raise ValueError('invalid bounded modeled source object descriptor')
    get_object=(lambda ref,cap:db.CODEC.objects.get_bounded(ref,cap)) if continuation or commercial else (lambda ref,cap:db.CODEC.objects.get(ref))
    envelope = json.loads(get_object(execution['input'],1024*1024))
    if (set(envelope) != {'request','execution_timeout_ms'} or (not _mortgage_json_exact(envelope['request'],raw) if strict_modeled else envelope['request'] != raw) or
            type(envelope['execution_timeout_ms']) is not int or not 1 <= envelope['execution_timeout_ms'] <= 1800000):
        raise ValueError('persisted modeled input differs from queued raw source')
    check(True)
    header = json.loads(get_object(execution['modeled_source'],32*1024*1024))
    check(True)
    if (set(header) != {'schema','model_schema','prepared_request','source_receipt'} or
            header['schema'] != ('commercial-ledger-preparation-1' if commercial else 'deposit-performing-portfolio-workflow-2' if model=='deposit-performing-portfolio-ledger-2' else 'model-ledger-preparation-1') or header['model_schema'] != model):
        raise ValueError('persisted modeled source header identity mismatch')
    mortgage = model == 'mortgage-mixed-accounting-1'
    revised = model == 'deposit-revised-income-ledger-1'
    continuation = model in {'deposit-performing-income-ledger-1','deposit-performing-portfolio-ledger-2'}
    strict_modeled = mortgage or revised or continuation or commercial
    if commercial:
        from portfolio_risk.analytics.commercial_ledger_reference import verify_preparation,verify_stream
    elif model=='deposit-performing-portfolio-ledger-2':
        from portfolio_risk.analytics.deposit_portfolio_reference import verify_preparation,verify_stream
    elif continuation:
        from portfolio_risk.analytics.deposit_continuation_reference import verify_preparation, verify_stream
    elif revised:
        from portfolio_risk.analytics.deposit_revised_income_reference import verify_preparation, verify_stream
    elif mortgage:
        from portfolio_risk.analytics.mortgage_mixed_reference import verify_preparation, verify_stream
    elif model == 'graph-dated-ledger-1':
        from portfolio_risk.analytics.consumer_schedule_reference import verify_preparation
    elif model == 'deposit-graph-dated-ledger-1':
        from portfolio_risk.analytics.deposit_schedule_reference import verify_preparation
    elif model == 'deposit-graph-income-ledger-1':
        from portfolio_risk.analytics.deposit_income_reference import verify_preparation
    else:
        from portfolio_risk.analytics.driven_credit_reference import verify_preparation
    source_validation = verify_preparation(raw, header['prepared_request'], header['source_receipt'], check)
    check(True)
    spec = None if strict_modeled else validate_spec(header['prepared_request'], max_positions=60000 if request['large_book'] else 2000,
                         max_work=90000000 if request['large_book'] else 3000000)
    schemas = model_ledger.SCHEMAS[model]
    if set(result) != set(schemas) | {'execution','revision'}:
        raise ValueError('persisted modeled table coverage mismatch')
    partition_rows = execution['partition_rows']
    if type(partition_rows) is not int or not 1 <= partition_rows <= 65536:
        raise ValueError('invalid persisted modeled partition bound')
    class StoredFrames:
        def __init__(self):
            self.schemas=schemas
            if strict_modeled:
                descriptor=execution.get('producer_completion',{})
                if descriptor.get('format')!='bin' or type(descriptor.get('bytes')) is not int or not 0<descriptor['bytes']<=128:raise ValueError('invalid mortgage completion descriptor')
                check(True)
                self.completion=json.loads(get_object(descriptor,128))
                check(True)
                if not _mortgage_json_exact(self.completion,{'complete':True,'protocol':6 if commercial else 3}):raise ValueError('persisted mortgage producer did not complete')
            self.parts = {}
            serialized_bytes=sum(execution[key]['bytes'] for key in ('input','modeled_source','producer_completion')) if continuation or commercial else 0;part_count=0
            self.decoded=0;self.logical=0
            if (continuation or commercial) and serialized_bytes>32*1024*1024:raise ValueError('modeled persisted aggregate metadata byte cap')
            for name, schema in schemas.items():
                table = result[name]
                if (table.get('format') != 'partitioned-parquet' or
                        table['schema'] != {k:str(v) for k,v in schema.items()} or
                        type(table['rows']) is not int or table['rows'] < 0 or
                        any(type(p['rows']) is not int or not 1 <= p['rows'] <= partition_rows for p in table['parts']) or
                        table['rows'] != sum(p['rows'] for p in table['parts'])):
                    raise ValueError('invalid persisted modeled partition descriptor')
                if continuation or commercial:
                    for part in table['parts']:
                        descriptor=part['ref'];size=descriptor.get('bytes')
                        if type(size) is not int or not 0<size<=32*1024*1024:raise ValueError('deposit persisted partition byte cap')
                        serialized_bytes+=size;part_count+=1
                        if serialized_bytes>32*1024*1024 or part_count>65536:raise ValueError('deposit persisted aggregate byte/count cap')
                self.parts[name] = table['parts']
        def finish(self):check(True)
        def frames(self, name):
            for part in self.parts[name]:
                check(True)
                if part['ref']['format'] != 'parquet':raise ValueError('invalid modeled partition object type')
                data=get_object(part['ref'],32*1024*1024)
                if continuation or commercial:
                    from .deposit_partition_bounds import parquet_metadata
                    self.decoded,self.logical=parquet_metadata(data,part['rows'],{k:str(v) for k,v in schemas[name].items()},decoded=self.decoded,logical=self.logical,check=lambda:check(True))
                    frame=pl.read_parquet(io.BytesIO(data),n_rows=part['rows']+1)
                else:
                    frame = pl.read_parquet(io.BytesIO(data))
                if frame.height != part['rows'] or frame.schema != schemas[name]:
                    raise ValueError('persisted modeled partition schema or rows differ')
                check(True)
                yield frame
    frames = StoredFrames()
    if commercial or revised or continuation:
        from portfolio_risk.analytics.deposit_revision_transport import VerificationFrames
        validation = verify_stream(raw, source_validation, VerificationFrames(frames,
            lambda:json.loads(get_object(execution['producer_completion'],128))), check)
    elif mortgage:
        validation = verify_stream(raw, source_validation, frames, check)
    elif model == 'driven-credit-ledger-1':
        validation = verify_sources(header['prepared_request'], spec, frames, check)
    else:
        validation = _finalize(spec, frames, check, native_reports=True)
        validation.update(verify_dated_sources(header['prepared_request'], spec, frames, check))
    # Both the source gate and journal evidence must reproduce the saved receipt.
    if not strict_modeled:validation.update(source_validation)
    if (not _mortgage_json_exact(execution['validation'],validation) if strict_modeled else execution['validation'] != validation):
        raise ValueError('persisted modeled verification receipt differs')
    identities()
