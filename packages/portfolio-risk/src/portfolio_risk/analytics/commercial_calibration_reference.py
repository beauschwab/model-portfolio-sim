"""Independent retained-Q fit, native-quote residual and bracket receipt gate."""
from copy import deepcopy
import math
import struct
from . import commercial_stochastic_reference as reference
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity

def _close(a, b):
    return abs(a-b) <= 1e-9 + 1e-12*abs(b)

def _bits(a, b):
    return type(a) is float and type(b) is float and struct.pack('!d', a) == struct.pack('!d', b)

def verify(law, policy, target, receipt, check=lambda: False):
    if set(policy) != {'policy', 'lower_oas', 'upper_oas', 'max_trials'}:
        raise ArithmeticError('generated calibration controls differ')
    low, high, limit = policy['lower_oas'], policy['upper_oas'], policy['max_trials']
    if (policy['policy'] != 'native_retained_q_refit_bracket-1'
        or type(low) not in (int,float) or type(high) not in (int,float)
        or not math.isfinite(low) or not math.isfinite(high) or not -5.<=low<high<=5.
        or type(limit) is not int or not 3<=limit<=128):
        raise ArithmeticError('bounded original generated calibration required')
    expected_keys = {'policy','target_cash_price','trials','selected_base_oas',
        'stopped_native_cash_value','quote_residual','pricing_training_samples_generated',
        'pricing_evaluation_samples_generated','continuation_refitted_each_trial',
        'physical_outcomes_used_for_calibration'}
    if set(receipt) != expected_keys or not exact_json_identity(dict(policy=policy['policy'],lower_oas=float(low),upper_oas=float(high),max_trials=limit),receipt['policy']):
        raise ArithmeticError('generated calibration receipt shape/policy differs')
    if (receipt['continuation_refitted_each_trial'] is not True
        or receipt['physical_outcomes_used_for_calibration'] is not False
        or type(receipt['pricing_training_samples_generated']) is not int
        or type(receipt['pricing_evaluation_samples_generated']) is not int
        or receipt['pricing_training_samples_generated'] != law['training']['paths']
        or receipt['pricing_evaluation_samples_generated'] != law['evaluation']['paths']):
        raise ArithmeticError('generated calibration sample/measure ownership differs')
    if not _bits(float(target),receipt['target_cash_price']):
        raise ArithmeticError('generated calibration target identity differs')
    raw = reference._typed(law)
    reference._admit(raw)
    train = reference._paths(raw,raw['training'],reference._tape(raw,raw['training'],check),check)
    levels = reference._tape(raw,raw['evaluation'],check)
    retained = dict(training=train,evaluation_levels=levels,
        evaluation=reference._paths(raw,raw['evaluation'],levels,check))
    trials = receipt['trials']
    if type(trials) is not list or not 2<=len(trials)<=limit:
        raise ArithmeticError('bounded complete calibration trials required')
    low, high = float(low), float(high)
    low_value = high_value = None
    endpoint = None
    selected = None
    for index, trial in enumerate(trials):
        check()
        if set(trial) != {'base_oas','evaluation_value'}:
            raise ArithmeticError('calibration trial shape differs')
        oas = low if index==0 else high if index==1 else low+(high-low)*.5
        if not _bits(oas,trial['base_oas']):
            raise ArithmeticError('calibration bracket path differs')
        r = deepcopy(raw);r['fixed_base_oas'] = oas
        evaluated = reference.evaluate(r,check,_retained_samples=retained)
        value = evaluated['evaluation_value']
        compare(float(value),trial['evaluation_value'],check=check)
        if index==0:
            low_value=value
            if _close(value,target):endpoint=oas
            continue
        if index==1 and value>low_value+1e-9:
            raise ArithmeticError('nonmonotone calibration endpoints')
        if _close(value,target):
            if endpoint is not None:raise ArithmeticError('ambiguous calibration endpoints')
            selected=oas
        elif index==1 and endpoint is not None:
            selected=endpoint
        elif index==1:
            high_value=value
            if low_value<=target or value>=target:
                raise ArithmeticError('unbracketed original generated quote')
        else:
            if value>low_value+1e-9 or value<high_value-1e-9:
                raise ArithmeticError('nonmonotone generated calibration sample')
            if value>target:low,low_value=oas,value
            else:high,high_value=oas,value
        if selected is not None and index!=len(trials)-1:
            raise ArithmeticError('calibration continues after accepted quote')
    if selected is None or not _bits(selected,receipt['selected_base_oas']):
        raise ArithmeticError('generated original quote residual unresolved')
    r=deepcopy(raw);r['fixed_base_oas']=selected
    actual=reference.evaluate(r,check,_retained_samples=retained)['evaluation_value']
    if not _close(actual,target):raise ArithmeticError('independent stopped cash quote residual unresolved')
    compare(float(actual),receipt['stopped_native_cash_value'],check=check)
    compare(float(actual-target),receipt['quote_residual'],check=check)
    return selected
