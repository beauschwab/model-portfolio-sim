"""Independent credit-enabled exercise values, policy error and sampling error."""
from copy import deepcopy
from dataclasses import asdict
import json
import math
import os
from pathlib import Path
from unittest.mock import patch

import pytest

from commercial_credit_exercise_benchmark import (
    CreditExerciseCell, FACE, LGD, LAWS, analytic, assess, benchmark_sources,
    matrix, native_binary, parameters, request, run_cell, run_matrix, validate_domain,
)
from portfolio_risk.core import model_native


@pytest.fixture(scope='module')
def receipt(tmp_path_factory):
    selected=os.environ.get('PORTFOLIO_CONVERGENCE_NATIVE')
    destination=Path(os.environ['PORTFOLIO_CREDIT_EXERCISE_RECEIPT']) if os.environ.get(
        'PORTFOLIO_CREDIT_EXERCISE_RECEIPT') else tmp_path_factory.mktemp('credit-exercise')/'receipt.json'
    def evidence():
        if destination.exists():
            value=json.loads(destination.read_text(encoding='utf-8'))
            assert value['benchmark_sources']==benchmark_sources()
            assert value['native_identity']==model_native.binary_identity()
            return value
        value=run_matrix();destination.parent.mkdir(parents=True,exist_ok=True)
        with destination.open('x',encoding='utf-8')as file:json.dump(value,file,indent=2,sort_keys=True,allow_nan=False)
        return value
    if selected:
        with native_binary(selected):return evidence()
    return evidence()


@pytest.mark.parametrize('cell',matrix(),ids=lambda c:c.key)
def test_native_declared_credit_exercise_cell_and_independent_analytic_oracle(cell,receipt):
    row=next(r for r in receipt['records']if r['key']==cell.key)
    assert row['status']=='passed',row.get('error',row.get('assessment'))
    assert row['raw']==request(cell) and row['independent_raw_source_replayed']
    assert row['assessment']==assess(cell,row['raw'],row['actual'])
    a=row['assessment']
    assert a['exact_interval_contains_learned'] and not a['gate_errors']
    assert a['clusters']==cell.paths//2 and a['policy_loss']>=-1e-9
    assert a['total_error']==pytest.approx(a['sampling_error']+a['learned']['value']-a['optimal']['value'],abs=1e-12)


def test_complete_predeclared_family_and_all_failure_records_are_retained(receipt):
    assert len(receipt['declared_cells'])==len(receipt['records'])==60
    assert len({r['key']for r in receipt['records']})==60 and receipt['passed']
    assert receipt['numerical_family_only'] and not receipt['observed_validation'] and not receipt['full_issue_complete']
    with patch.object(model_native,'run',side_effect=RuntimeError('retained execution failure')):
        failure=run_cell(matrix()[0])
    assert failure['status']=='failed' and failure['error']=='retained execution failure'
    assert failure['raw']==request(matrix()[0]) and failure['cell']==asdict(matrix()[0])


def test_hazard_survival_and_closed_form_option_values_are_independent():
    for cell in matrix():
        raw=request(cell);validate_domain(cell,raw)
        before,after,recovery,continuation,strike=parameters(cell)
        intervals=raw['source']['credit']['intervals']
        assert -math.expm1(-intervals[0]['hazards_per_year'][0][3]*15/365.)==pytest.approx(before,abs=1e-16)
        assert -math.expm1(-intervals[1]['hazards_per_year'][0][3]*20/365.)==pytest.approx(after,abs=1e-16)
        expected=recovery*before+(1.-before)*(min(strike,continuation)if cell.right=='call'else max(strike,continuation)if cell.right=='put'else continuation)
        assert analytic(cell)['value']==pytest.approx(expected,abs=1e-12)
        if cell.right!='none':
            assert abs(strike-continuation)==8.
            assert analytic(cell)['optimal_stop']==((cell.right=='call'and cell.strike_side=='below')or(cell.right=='put'and cell.strike_side=='above'))


def test_actual_learned_policy_oracle_is_separate_from_optimal_policy():
    cell=next(c for c in matrix()if c.law=='moderate'and c.right=='put'and c.strike_side=='above')
    optimal=analytic(cell);wrong_policy=analytic(cell,learned_stop=False)
    assert optimal['value']==pytest.approx(94.2,abs=1e-12)
    assert wrong_policy['value']==pytest.approx(87.,abs=1e-12)
    assert optimal['value']>wrong_policy['value']
    # Default before exercise is always recovered60, never redeemed at98.
    assert optimal['value']==pytest.approx(60.*.10+98.*.90,abs=1e-12)


def test_fixed_oas_shared_crn_and_defaulted_rights_across_all_rights(receipt):
    for law in LAWS:
      for paths in (128,256):
       for train,evaluation in ((7,11),(19,23)):
        rows=[r for r in receipt['records']if r['cell']['law']==law and r['cell']['paths']==paths and r['cell']['training_seed']==train and r['cell']['evaluation_seed']==evaluation]
        free=next(r for r in rows if r['cell']['right']=='none')
        baseline=[r['value']for r in free['actual']['evaluation_paths']]
        fits=[r['actual']['regressions']for r in rows if r['cell']['right']!='none']
        assert all(fit==fits[0]for fit in fits)
        masks=[]
        for row in rows:
            assert 'assessment' in row,row.get('error')
            raw=row['raw'];assert raw['fixed_base_oas']==raw['parallel_bump']==0.
            assert raw['training']==free['raw']['training'] and raw['evaluation']==free['raw']['evaluation']
            if row['assessment']['learned_stop']:
                mask=[math.isclose(p['value'],60.,abs_tol=1e-9)for p in row['actual']['evaluation_paths']];masks.append(mask)
                assert all(not default or baseline[i]==60. for i,default in enumerate(mask))
                assert all(p['first_exercise_ordinal']is None for p,default in zip(row['actual']['evaluation_paths'],mask)if default)
            else:assert [p['value']for p in row['actual']['evaluation_paths']]==baseline
        assert all(mask==masks[0]for mask in masks)


@pytest.mark.parametrize('target',['cash','mean','se','cluster_count','default_right','fit_domain','raw_law'])
def test_assessment_rejects_altered_raw_cash_policy_and_sampling_evidence(target,receipt):
    row=next(r for r in receipt['records']if r['cell']['law']=='front_loaded'and r['cell']['right']=='put'and r['cell']['strike_side']=='above')
    cell=CreditExerciseCell(**row['cell']);raw=deepcopy(row['raw']);actual=deepcopy(row['actual'])
    if target=='cash':actual['evaluation_paths'][0]['value']+=1.
    elif target=='mean':actual['evaluation_value']+=1.
    elif target=='se':actual['evaluation_standard_error']+=1.
    elif target=='cluster_count':actual['evaluation_clusters']+=1
    elif target=='default_right':
        path=next(p for p in actual['evaluation_paths']if p['value']==60.)
        path['first_exercise_decision']='put';path['first_exercise_ordinal']=raw['policy']['exercise_dates'][0]['ordinal']
    elif target=='fit_domain':actual['regressions'][0]['active_features']=[0]
    else:raw['source']['credit']['intervals'][0]['complete_recovery_tails'][0][0]['lag_years']=.01
    with pytest.raises(ArithmeticError):assess(cell,raw,actual)


def test_credit_off_sampling_is_exact_and_normal_intervals_are_descriptive(receipt):
    rows=[r for r in receipt['records']if r['cell']['law']=='credit_off']
    assert len(rows)==20
    for row in rows:
        a=row['assessment'];assert a['cluster_standard_error']==0. and a['defaults']==0
        assert a['sample_value']==pytest.approx(a['optimal']['value'],abs=1e-9)
    # A deterministic zero-event sample for a genuinely positive default law
    # has a zero-width normal interval. Exact binomial uncertainty stays nonzero.
    cell=next(c for c in matrix()if c.law=='moderate'and c.right=='none')
    actual=dict(regressions=[],evaluation_paths=[dict(path_key=i+1,value=100.,first_exercise_decision='continue',first_exercise_ordinal=None,full_native_cash_replayed=True)for i in range(cell.paths)],evaluation_value=100.,evaluation_standard_error=0.,evaluation_clusters=cell.paths//2)
    a=assess(cell,request(cell),actual)
    assert a['zero_event_sample'] and not a['descriptive_normal95_contains_learned']
    assert a['default_probability_interval'][1]>0.0 and a['gate_errors']


def test_native_binary_changes_and_source_pin_changes_invalidate_evidence(tmp_path):
    binary=tmp_path/'lifecycle.exe';binary.write_bytes(b'original')
    with pytest.raises(RuntimeError,match='changed'):
        with native_binary(binary):binary.write_bytes(b'changed')
    with patch('commercial_credit_exercise_benchmark.benchmark_sources',side_effect=[{},dict(changed=True)]),patch('commercial_credit_exercise_benchmark.run_cell',return_value=dict(key='retained-source-change-control',status='failed',elapsed_seconds=0.0)),patch.object(model_native,'binary_identity',return_value='pinned'):
        with pytest.raises(RuntimeError,match='invalidate entire matrix'):run_matrix()
