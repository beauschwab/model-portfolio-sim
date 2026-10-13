"""Independent frozen-driver preparation verifier; never an execution backend.

No native, fitter or product engine calls. Reconstructed hazards are evidence for
publication only; financial execution and raw-spec admission remain native.
"""
from copy import deepcopy
import math

SCHEMA='driven-credit-ledger-1'
STATES=('performing','watch','delinquent','default','prepaid')
_EXACT_INTS={'path_key','seed','ordinal','seconds_of_day','as_of_ordinal','reporting_end_ordinal',
    'max_jumps_per_path','timeout_ms','horizon_days','start_day','maturity_day',
    'preparation_work_bound','projection_work_units','ledger_admitted_work','work_units'}


def _equal(actual,expected,path='',field=''):
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or actual.keys()!=expected.keys():
            raise ArithmeticError(f'driven credit preparation keys differ: {path}')
        for key in expected:_equal(actual[key],expected[key],path+'/'+key,key)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):
            raise ArithmeticError(f'driven credit preparation length differs: {path}')
        for i,(a,b) in enumerate(zip(actual,expected)):_equal(a,b,path+'/'+str(i),field)
    elif field in _EXACT_INTS:
        if type(actual) is not int or type(expected) is not int or actual!=expected:
            raise ArithmeticError(f'driven credit exact integer differs: {path}')
    elif isinstance(expected,(int,float)) and not isinstance(expected,bool):
        if (not isinstance(actual,(int,float)) or isinstance(actual,bool) or not math.isfinite(actual)
                or not math.isfinite(expected) or not math.isclose(actual,expected,rel_tol=1e-12,abs_tol=1e-9)):
            raise ArithmeticError(f'driven credit preparation amount differs: {path}')
    elif type(actual) is not type(expected) or actual!=expected:
        raise ArithmeticError(f'driven credit preparation identity differs: {path}')


def _keys(value,keys,where):
    if not isinstance(value,dict) or set(value)!=set(keys.split()):
        raise ArithmeticError(f'driven credit raw schema differs: {where}')


def _finite(value):
    if isinstance(value,bool) or not isinstance(value,(int,float)) or not math.isfinite(value):
        raise ArithmeticError('driven credit nonfinite financial input')
    return float(value)


def _integer(value,low,high):
    if type(value) is not int or not low<=value<=high:
        raise ArithmeticError('driven credit integer domain differs')
    return value


def _time(value):
    _keys(value,'ordinal seconds_of_day','observation time')
    return _integer(value['ordinal'],1,3652059)*86400+_integer(value['seconds_of_day'],0,86399)


def _checkpoint(check):
    if check():raise InterruptedError('driven credit verification cancelled')


def _parameter(value):
    _keys(value,'name value lower upper ridge_weight ridge_center','estimated parameter')
    x=_finite(value['value']);low=_finite(value['lower']);high=_finite(value['upper'])
    if not low<=x<=high or _finite(value['ridge_weight'])<0.:
        raise ArithmeticError('driven credit estimated parameter bounds differ')
    _finite(value['ridge_center'])
    return x


def _project(projection,research,check):
    _keys(projection,'schema measure model model_revision review use_policy position_id scenario_id origin_time end_time timeout_ms intervals','projection')
    model=projection['model']
    _keys(model,'schema measure data_kind population model_policy training_source training_split columns term_levels rows training_digest_sha256 numerical_eligible adoption_eligible coefficient_identity_sha256','fitted model')
    if projection['schema']!='credit-covariate-1' or model['schema']!='credit-covariate-1' or projection['measure']!='physical' or model['measure']!='physical':
        raise ArithmeticError('driven credit physical frozen model required')
    if projection['use_policy']!=('analytical_only' if research else 'adoption'):
        raise ArithmeticError('driven credit execution/projection use differs')
    if not research and (model['data_kind']!='observed' or model['adoption_eligible'] is not True or model['numerical_eligible'] is not True):
        raise ArithmeticError('driven credit ineligible adoption model')
    for key in ('training_digest_sha256','coefficient_identity_sha256'):
        digest=model[key]
        if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):
            raise ArithmeticError('driven credit model digest identity differs')
    if len(model['rows'])!=3 or not model['term_levels'] or len(set(model['term_levels']))!=len(model['term_levels']) or len(model['columns'])>8:
        raise ArithmeticError('driven credit model dimensions differ')
    columns=model['columns'];terms=model['term_levels'];origin=_time(projection['origin_time']);end=_time(projection['end_time'])
    _integer(projection['timeout_ms'],1,3600000)
    if not 0<end-origin<=100*365*86400 or not 0<len(projection['intervals'])<=4096:
        raise ArithmeticError('driven credit driver horizon admission differs')
    intervals=[];prior=origin;ids=set()
    for driver in projection['intervals']:
        _checkpoint(check)
        _keys(driver,'interval_id start end available_time kind source covariates term_level','driver interval')
        start=_time(driver['start']);stop=_time(driver['end'])
        if start!=prior or not start<stop<=end or _time(driver['available_time'])>start or driver['interval_id'] in ids:
            raise ArithmeticError('driven credit ordered available driver coverage differs')
        if driver['kind'] not in ('supplied_observed','supplied_projected') or len(driver['covariates'])!=len(columns) or driver['term_level'] not in terms:
            raise ArithmeticError('driven credit feature/term coverage differs')
        ids.add(driver['interval_id']);scaled=[]
        for column,x in zip(columns,driver['covariates']):
            _keys(column,'name role unit offset scale raw_lower raw_upper scaling_review','feature column')
            x=_finite(x);scale=_finite(column['scale']);low=_finite(column['raw_lower']);high=_finite(column['raw_upper'])
            if scale<=0. or not low<=x<=high:raise ArithmeticError('driven credit feature domain differs')
            scaled.append((x-_finite(column['offset']))/scale)
        hazards=[];term=terms.index(driver['term_level'])
        for origin_index,row in enumerate(model['rows']):
            if row['kind']=='fixed_row':
                _keys(row,'kind hazards_per_year review rationale','fixed origin')
                rates=[_finite(v) for v in row['hazards_per_year']]
            elif row['kind']=='log_linear':
                _keys(row,'kind edges','fitted origin');rates=[0.]*5;destinations=set()
                if not 1<=len(row['edges'])<=4:raise ArithmeticError('driven credit edge admission differs')
                for edge in row['edges']:
                    _keys(edge,'destination intercept covariate_coefficients term_coefficients','fitted edge')
                    destination=STATES.index(edge['destination'])
                    if destination==origin_index or destination in destinations or len(edge['covariate_coefficients'])!=len(columns) or len(edge['term_coefficients'])!=len(terms)-1:
                        raise ArithmeticError('driven credit competing edge coverage differs')
                    destinations.add(destination)
                    beta=[_parameter(p) for p in edge['covariate_coefficients']]
                    gamma=[_parameter(p) for p in edge['term_coefficients']]
                    # Explicit reference category has all zero term indicators.
                    log_rate=_parameter(edge['intercept'])+sum(b*z for b,z in zip(beta,scaled))
                    if term:log_rate+=gamma[term-1]
                    rates[destination]=math.exp(log_rate)
            else:raise ArithmeticError('driven credit unknown origin model')
            if len(rates)!=5 or rates[origin_index]!=0. or any(not math.isfinite(v) or v<0. for v in rates) or sum(rates)>100.:
                raise ArithmeticError('driven credit competing hazard domain differs')
            hazards.append(rates)
        intervals.append(dict(interval_id=driver['interval_id'],start=deepcopy(driver['start']),end=deepcopy(driver['end']),
            end_year=(stop-origin)/(365.*86400.),source=deepcopy(driver['source']),kind=driver['kind'],hazards_per_year=hazards))
        prior=stop
    if prior!=end:raise ArithmeticError('driven credit incomplete driver path')
    return dict(schema='credit-covariate-1',measure='physical',position_id=projection['position_id'],scenario_id=projection['scenario_id'],
        model_revision=deepcopy(projection['model_revision']),review=deepcopy(projection['review']),
        coefficient_identity_sha256=model['coefficient_identity_sha256'],adoption_output=not research,analytical_only=research,intervals=intervals)


def verify_preparation(raw,prepared_request,source_receipt,check=lambda:False):
    """Rebuild projected contracts from immutable raw inputs, without native calls.

    Work counters are bounded operational evidence; their Rust JSON/SHA accounting
    is not reused as a financial oracle. Every financial/source value is rebuilt.
    """
    _checkpoint(check)
    _keys(raw,'schema use_policy monetary_unit as_of_ordinal specification timeout_ms runs','request')
    if raw['schema']!=SCHEMA or raw['monetary_unit']!='currency_units':raise ArithmeticError('driven credit schema/unit differs')
    _integer(raw['as_of_ordinal'],1,3652059);_integer(raw['timeout_ms'],1,3600000)
    policy=raw['use_policy'];research=policy['kind']=='research_analytical_only'
    _keys(policy,'kind review rationale' if research else 'kind','execution policy')
    if policy['kind'] not in ('adoption','research_analytical_only'):raise ArithmeticError('driven credit execution policy differs')
    if research and (not policy['rationale'] or not policy['review']['id'] or not policy['review']['revision']):
        raise ArithmeticError('driven credit research review missing')
    _keys(source_receipt,'schema raw_source projections research_only adoption_output empirical_validation regulatory_compliance preparation_work_bound projection_work_units ledger_admitted_work','source receipt')
    _equal(source_receipt['raw_source'],raw,'raw_source')
    if not 1<=len(raw['runs'])<=4096:raise ArithmeticError('driven credit run admission differs')
    expected_runs=[];expected_projections=[];crn={};batch_count=0;interval_count=0;work=0
    if not isinstance(source_receipt['projections'],list):raise ArithmeticError('driven credit projection receipt missing')
    for run in raw['runs']:
        _keys(run,'run_id scenario_id severity batches','run')
        prepared_batches=[]
        if not run['batches']:raise ArithmeticError('driven credit batch coverage missing')
        batch_count+=len(run['batches'])
        if batch_count>4096:raise ArithmeticError('driven credit batch admission differs')
        for batch in run['batches']:
            _checkpoint(check)
            _keys(batch,'batch_id account_id projection template','batch')
            projection=batch['projection'];template=batch['template'];path=template['path']
            _keys(template,'schema path day_mapping reporting_end_ordinal timestamp_tolerance_seconds bindings','accounting template')
            _keys(path,'schema measure monetary_unit source_id source_revision model_policy_id origin_time max_jumps_per_path intervals boundaries draw_stream paths','path template')
            if template['schema']!='credit-path-accounting-1' or path['schema']!='credit-path-1' or path['measure']!='physical' or path['monetary_unit']!='currency_units' or len(path['paths'])!=1 or len(template['bindings'])!=1:
                raise ArithmeticError('driven credit physical singleton template differs')
            opening=path['paths'][0];binding=template['bindings'][0]
            if (opening['path_id']!=projection['position_id'] or binding['position_id']!=opening['path_id'] or
                    binding['path_key']!=opening['path_key'] or projection['scenario_id']!=run['scenario_id'] or
                    path['model_policy_id']!=projection['model']['model_policy']['id']):
                raise ArithmeticError('driven credit position/scenario/model mapping differs')
            _integer(opening['path_key'],0,2**64-1)
            _equal(path['origin_time'],projection['origin_time'],'origin_time')
            _equal(path['origin_time'],dict(ordinal=raw['as_of_ordinal'],seconds_of_day=0),'ledger_origin')
            _equal(projection['end_time'],dict(ordinal=template['reporting_end_ordinal'],seconds_of_day=0),'ledger_end')
            frozen=[path['draw_stream'],opening]
            if opening['path_id'] in crn:_equal(frozen,crn[opening['path_id']],'scenario_crn')
            else:crn[opening['path_id']]=deepcopy(frozen)
            projected=_project(projection,research,check)
            interval_count+=len(projected['intervals'])
            if interval_count>50000 or len(path['intervals'])!=len(projected['intervals']):
                raise ArithmeticError('driven credit policy interval coverage differs')
            resolved=[]
            for policy_interval,hazards in zip(path['intervals'],projected['intervals']):
                _keys(policy_interval,'end_time accrual_rates loss_given_default credit_conversion_factor interest_recovery_fraction complete_recovery_tails','policy interval')
                _equal(policy_interval['end_time'],hazards['end'],'policy_end')
                resolved.append({k:deepcopy(v) for k,v in policy_interval.items() if k!='end_time'}|
                    dict(end_year=hazards['end_year'],hazards_per_year=hazards['hazards_per_year']))
            expected_path=deepcopy(path);expected_path['intervals']=resolved
            expected_template=deepcopy(template);expected_template['path']=expected_path
            prepared_batches.append(dict(batch_id=batch['batch_id'],account_by_position=[dict(position_id=opening['path_id'],account_id=batch['account_id'])],request=expected_template))
            index=len(expected_projections)
            if index>=len(source_receipt['projections']):raise ArithmeticError('missing driven credit projection receipt')
            actual=source_receipt['projections'][index]
            _keys(actual,'run_id scenario_id severity batch_id account_id position_id path_key projection','projection receipt')
            units=_integer(actual['projection'].get('work_units'),1,20000000)
            work+=units
            projected['work_units']=units
            expected_projections.append(dict(run_id=run['run_id'],scenario_id=run['scenario_id'],severity=run['severity'],
                batch_id=batch['batch_id'],account_id=batch['account_id'],position_id=opening['path_id'],path_key=opening['path_key'],projection=projected))
        expected_runs.append({k:run[k] for k in ('run_id','scenario_id','severity')}|dict(batches=prepared_batches))
    _equal(prepared_request,dict(schema='credit-path-ledger-1',monetary_unit='currency_units',as_of_ordinal=raw['as_of_ordinal'],
        specification=deepcopy(raw['specification']),runs=expected_runs),'prepared_request')
    bounds=_integer(source_receipt['preparation_work_bound'],1,20000000)
    admitted=_integer(source_receipt['ledger_admitted_work'],1,90000000)
    if bounds<256*interval_count or work>20000000:raise ArithmeticError('driven credit operational work admission differs')
    expected_receipt=dict(schema=SCHEMA,raw_source=raw,projections=expected_projections,research_only=research,adoption_output=not research,
        empirical_validation=False,regulatory_compliance=False,preparation_work_bound=bounds,projection_work_units=work,ledger_admitted_work=admitted)
    _equal(source_receipt,expected_receipt,'source_receipt')
    _checkpoint(check)
    return dict(driven_credit_source_recomputed=True,driver_hazards_recomputed=True,prepared_credit_source_reconciled=True,physical_driver_measure=True,
        research_only=research,adoption_output=not research,empirical_validation=False,regulatory_compliance=False,
        projection_count=batch_count,driver_interval_count=interval_count,
        reference='Independent frozen-feature loglinear hazard and exact calendar preparation replay')
