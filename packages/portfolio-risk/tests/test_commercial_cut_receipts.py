"""Selected receipt lifetimes and exact protocol8 authority; no supplied money."""
from copy import deepcopy
import pytest
from portfolio_risk.analytics.commercial_cut_mixed_reference import verify_completion


def completion():
    return dict(complete=True, protocol=8, cut_day=3, native_cut_state_moved=True,
        original_calendar_retained=True, new_sim_started_at_cut=True,
        prior_publication_verified=False)


def test_complete_native_cut_metadata_is_preserved_and_verified():
    verify_completion(completion(), 3)


@pytest.mark.parametrize('key,value', [
    ('complete', 1), ('protocol', 8.), ('cut_day', 3.), ('cut_day', 4),
    ('native_cut_state_moved', False), ('original_calendar_retained', False),
    ('new_sim_started_at_cut', False), ('prior_publication_verified', True),
])
def test_native_cut_completion_metadata_cannot_be_retyped_or_rebased(key, value):
    actual = completion(); actual[key] = value
    with pytest.raises(ArithmeticError, match='exact producer'):
        verify_completion(actual, 3)


def test_missing_or_extra_cut_completion_metadata_refuses():
    actual = completion(); del actual['native_cut_state_moved']
    with pytest.raises(ArithmeticError): verify_completion(actual, 3)
    actual = deepcopy(completion()); actual['opening_identity_verified'] = True
    with pytest.raises(ArithmeticError): verify_completion(actual, 3)

from pathlib import Path
import json
from portfolio_risk.analytics.commercial_cut_receipts_reference import authority,retention,shape,verify_census
from portfolio_risk.core.commercial_cut_mixed_native import run
ROOT=Path(__file__).resolve().parents[3]


def selected_raw():
    raw=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json').read_text())
    raw['preparation_receipt_policy']='encode_native_preparations_before_value_decode-1'
    return raw


@pytest.fixture(scope='module')
def actual_refusal():
    with pytest.raises(ValueError,match='selected cut pre-Sim structural admission refused:') as caught:
        run(selected_raw(),timeout=180.)
    return json.loads(str(caught.value).split('selected cut pre-Sim structural admission refused: ',1)[1])


def test_actual_full_native_source_census_refuses_and_independent_scan_formula_agrees(actual_refusal):
    phases=verify_census(actual_refusal,admitted=False)
    assert actual_refusal['initial_source_work_upper']==26_801_980
    assert actual_refusal['output_inventory']['schema']=='commercial-cut-output-inventory-2'
    # Native floating-point text can differ across platforms. Each embedded
    # RawValue byte bounds one encoded byte and one child cell: two preparation
    # scans plus three final-result scans charge eight visits per receipt byte.
    # Pin the remaining shape and work exactly, independent of decimal spelling.
    receipt_bytes=actual_refusal['completed_receipt_bytes']+actual_refusal['revised_receipt_bytes']
    assert actual_refusal['final_result_shape']==dict(
        encoded_bytes_upper=1_187_959+receipt_bytes,
        cells_upper=65_234+receipt_bytes,
        string_bytes_upper=612_247+receipt_bytes)
    assert actual_refusal['final_joined_work_upper']==51_339_011+8*receipt_bytes
    assert max(phases.values())==132_084_182
    assert actual_refusal['output_inventory']['stream_batch_retained_bytes_upper']==20_392_098
    assert actual_refusal['cut_ledger_census']['work_upper']==11_343_032
    assert actual_refusal['cut_ledger_census']['retained_bytes_upper']==2_876_652
    with pytest.raises(ArithmeticError,match='caps refuse'):
        verify_census(actual_refusal,admitted=True)
    assert 'cut_tables' not in actual_refusal and 'producer_completion' not in actual_refusal


@pytest.mark.parametrize('target', ['work','comparison','raw','receipt','phase','bool','overflow','extra','table','sink','sink_bool','sink_total','inventory_version','dictionary'])
def test_independent_census_rejects_mutated_metadata_including_retyped_dimensions(actual_refusal,target):
    x=deepcopy(actual_refusal)
    if target=='work': x['final_joined_work_upper']-=1
    elif target=='comparison': x['cut_ledger_census']['comparison_work_upper']-=1
    elif target=='raw': x['owned_wire_capacity']=1
    elif target=='receipt': x['completed_receipt_bytes']-=1
    elif target=='phase': x['retention_phases']['source_owner_phase_bytes']-=1
    elif target=='bool': x['work_components']['original_raw_census']=True
    elif target=='overflow': x['typed_raw_bytes']=1<<64
    elif target=='extra': x['native_cut_state_moved']=True
    elif target=='table': x['output_inventory']['visible']['journal']['fields'].append('foreign')
    elif target=='sink': x['output_inventory']['stream_batch_memory']['row_vector_bytes_upper']-=1
    elif target=='sink_bool': x['output_inventory']['stream_batch_memory']['journal_vector_bytes_upper']=True
    elif target=='sink_total': x['output_inventory']['stream_batch_retained_bytes_upper']-=1
    elif target=='inventory_version': x['output_inventory']['schema']='commercial-cut-output-inventory-3'
    else:x['output_inventory']['dictionary_upper']['cells_upper']+=1
    with pytest.raises(ArithmeticError):verify_census(x,admitted=False)


def test_selected_source_authority_is_required_even_at_zero_allocation():
    raw=selected_raw();authority(raw)
    for key in ('completed_candidate','revised_candidate'):
        for a in raw[key]['allocation']:a['face']=0.
    authority(raw)
    del raw['completed_candidate']['commercial']['units'][0]['graph']['positions'][0]['original_pricing'][0]['source']['source']['transfer']['source_work_policy']
    with pytest.raises(ArithmeticError,match='genuine transfer authority'):
        authority(raw)


def test_structural_work_preserves_cells_and_escaped_strings_without_money_based_pruning():
    a=shape({'lots':[0.,-0.,1e-20],'source':'非\\"\n'})
    b=shape({'lots':[1e200,-1e200,1e30],'source':'非\\"\n'})
    assert a==b and a[1]==8 and a[2]>10
    assert shape(True)!=(32,1,0)
    for args in [(1<<64,0,0,0),(0,1<<64,0,0),(0,0,1<<64,0),(0,0,0,1<<64)]:
        with pytest.raises(ArithmeticError):retention(*args)


def test_producer_arithmetic_agreement_never_grants_selected_publication(actual_refusal):
    from portfolio_risk.analytics.commercial_cut_receipts_reference import require_raw_derived_admission
    verify_census(actual_refusal,admitted=False)
    # This is a refusal/census control. It fabricates no accepted finance and
    # makes no claim that the native source has passed its independent gates.
    with pytest.raises(ArithmeticError,match='independent raw-derived source/Compiled/output admission census is incomplete'):
        require_raw_derived_admission()


def test_historical_inventory_retains_its_original_clone_reservation_and_refusal():
    frozen=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/commercial-cut-inventory-1-refusal.json').read_text(encoding='utf-8'))
    assert frozen['output_inventory']['schema']=='commercial-cut-output-inventory-1'
    assert frozen['output_inventory']['stream_batch_retained_bytes_upper']==38_840_800
    assert max(verify_census(frozen,admitted=False).values())==150_532_884
    with pytest.raises(ArithmeticError,match='caps refuse'):
        verify_census(frozen,admitted=True)


def test_borrowed_sink_capacity_uses_every_table_and_repeated_zero_label_upper():
    from portfolio_risk.analytics.commercial_cut_receipts_reference import _borrowed_sink_memory
    frozen=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/commercial-cut-inventory-1-refusal.json').read_text(encoding='utf-8'))
    inv=frozen['output_inventory']; memory=_borrowed_sink_memory(inv)
    assert memory['row_tree_bytes_upper']==19_420_400
    assert memory['journal_vector_bytes_upper']==99_584
    assert sum(n for k,n in memory.items() if k!='schema')<22_000_000
    inv['prefix']['journal']['rows_upper']=1<<32
    assert _borrowed_sink_memory(inv)['journal_vector_bytes_upper']==64*(3*16_384+8)
    inv['dictionary_upper']['cells_upper']=(1<<64)-1
    with pytest.raises(ArithmeticError,match='overflow'):
        _borrowed_sink_memory(inv)
    inv['dictionary_upper']['cells_upper']=((1<<64)-1-2240)//584
    with pytest.raises(ArithmeticError,match='overflow'):
        _borrowed_sink_memory(inv)
