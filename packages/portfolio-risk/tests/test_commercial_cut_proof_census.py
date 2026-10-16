"""Independent raw-only compact-cut diagnostic; no native or producer inputs."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import commercial_cut_proof_census as oracle
FIXTURE=Path(__file__).with_name('fixtures')/'commercial-short-secondary-cut-refusal.json'

@pytest.fixture(scope='module')
def raw():return json.loads(FIXTURE.read_text())

@pytest.fixture(scope='module')
def receipt(raw):return oracle.census(raw)

def test_complete_q_p_and_origin_use_occurrences_remain_distinct(receipt):
    uses=sum(receipt['source_uses'].values(),[])
    assert len(uses)==27 and sum(u['original']for u in uses)==3
    assert sum(u['role']=='physical'for u in uses)==6
    assert sum(u['role']=='pricing_down'for u in uses)==6
    assert sum(u['role']=='pricing_up'for u in uses)==6
    assert len({u['raw_locator']for u in uses})==27
    assert all('secondary_acquisition'in u['families']and'fee_source'in u['families']for u in uses)
    assert 'original_q_native_live_capture_not_retained_in_final_graph'in receipt['unproved_gates']

def test_raw_compiled_dimensions_match_hand_row_inventory(receipt):
    old=receipt['compiled']['completed_candidate'];new=receipt['compiled']['revised_candidate']
    assert (old['loan_stage_rows'],old['fee_stage_rows'],old['work_upper'],old['retained_component_formula_bytes'])==(92,32,61504,507904)
    assert (new['loan_stage_rows'],new['fee_stage_rows'],new['work_upper'],new['retained_component_formula_bytes'])==(152,64,107136,884736)
    assert (old['loan_bindings'],old['fee_bindings'],new['loan_bindings'],new['fee_bindings'])==(2,2,4,4)

def test_exact_interleaved_prefix_retains_native_clock_bits(receipt):
    p=receipt['prefix']
    assert (p['events'],p['loans'],p['fees'])==(8,4,4)
    assert [e['kind']for e in p['ordered_keys']]==['loan','fee','fee','loan']*2
    assert p['ordered_keys'][3]['native_year_bits']=='3f66719f3601671a'
    assert p['comparison_work_upper']==19876 and p['left_shape']==p['right_shape']
    assert p['left_shape']['declared_field_visits']>p['left_shape']['emitted_fields']

def test_all16families_dictionary_flushes_and_caps_remain_explicit(receipt):
    out=receipt['output16']
    assert set(out['prefix'])==set(out['visible'])==set(oracle.TABLES)
    assert out['per_table_headers_upper']==16*out['all_flushes_upper']
    assert receipt['dictionary']['old_sink_to_global_index_mapping_required']is True
    assert receipt['caps']==dict(root_work=32_000_000,root_bytes=128<<20,cut_work=8_000_000,cut_bytes=32<<20,complete_stream_bytes=4<<20)
    assert receipt['known_source_proof_shape']['cells']==34861
    assert receipt['known_four_serialization_shaped_passes_upper']>4_541_816

def test_zero_allocation_never_prunes_unweighted_source_policy(raw,receipt):
    changed=deepcopy(raw)
    for side in ('completed_candidate','revised_candidate'):
        for allocation in changed[side]['allocation']:allocation['face']=0.
    actual=oracle.census(changed)
    assert actual['source_histories']==27
    for side in ('completed_candidate','revised_candidate'):
        assert actual['compiled'][side]['all_groups_shape']==receipt['compiled'][side]['all_groups_shape']
        assert actual['compiled'][side]['loan_stage_rows']==receipt['compiled'][side]['loan_stage_rows']
    assert actual['prefix']==receipt['prefix']and actual['publication_allowed']is False

def test_full_origin_rows_and_span_increase_reservation_with_buyer_unchanged(raw):
    source=raw['completed_candidate']['commercial']['units'][0]['graph']['positions'][0]['original_pricing'][0]['source']
    before=oracle._source_dimensions(source,oracle.Ticket());changed=deepcopy(source)
    origin=changed['source']['transfer']['contract_origin'];p=deepcopy(origin['periods'][0])
    p.update(period_id='earlier-settled-coupon',start_ordinal=738884,end_ordinal=738885,payment_ordinal=738885)
    origin['asof_ordinal']=738884;origin['periods'].insert(0,p);origin['principal_plan']['principal_payments'].insert(0,0.)
    after=oracle._source_dimensions(changed,oracle.Ticket())
    assert after['periods']==before['periods']==1
    for k in ('contract_periods','complete_calendar_span','reference_rows_upper','reference_work_upper','reference_bytes_upper'):assert after[k]>before[k]

@pytest.mark.parametrize('mutation',['hint','unknown_nested','wrong_version','missing_selector','indexed','exercise'])
def test_unmapped_or_counterfeit_inputs_refuse_before_financial_projection(raw,monkeypatch,mutation):
    from portfolio_risk.analytics import commercial_credit_reference
    changed=deepcopy(raw);p=changed['completed_candidate']['commercial']['units'][0]['graph']['positions'][0];s=p['original_pricing'][0]['source']
    if mutation=='hint':changed['admitted_work_upper']=1
    elif mutation=='unknown_nested':s['source']['transfer']['unmapped_money']=1.
    elif mutation=='wrong_version':s['schema']='unreviewed-source-2'
    elif mutation=='missing_selector':s['source']['transfer']['source_work_policy']='caller_allowance-1'
    elif mutation=='indexed':s['source']['current']['periods'][0]['coupon']['kind']='indexed'
    else:p['exercise']={}
    def forbidden(*a,**k):raise AssertionError('financial projection began before raw admission')
    monkeypatch.setattr(commercial_credit_reference,'replay',forbidden)
    with pytest.raises(ArithmeticError):oracle.census(changed)

def test_ticket_and_scalar_failures_happen_before_growth():
    t=oracle.Ticket(work=oracle.MAX_WORK)
    with pytest.raises(ArithmeticError,match='before scan/growth'):oracle.shape({'a':1},t)
    assert t.retained==0
    with pytest.raises(ArithmeticError):oracle.shape(float('nan'))
    with pytest.raises(ArithmeticError):oracle.shape([0]*100001)
    with pytest.raises(ArithmeticError):oracle._add(oracle.U64,1)

def test_visible_signed_zero_and_integer_categories_are_exact():
    assert not oracle._exact(0.,-0.,oracle.Ticket())
    assert not oracle._exact(0,0.,oracle.Ticket())
    assert not oracle._exact(1,True,oracle.Ticket())
    assert oracle._fee_state(dict.fromkeys(oracle._STATE_ALWAYS,0.),oracle.Ticket())==oracle._fee_state(dict.fromkeys(oracle._STATE_ALWAYS,0.)|{'acquired_fee_asset':-0.},oracle.Ticket())

def test_unmapped_native_snapshot_lot_fields_fail_closed():
    state=dict(path_key=1,year=0.,principal=100.,recognized_interest=0.,legal_interest=0.,native_legal_reduction_difference=0.,book_adjustment=0.,gross_carry=100.,in_default=False,lots=[])
    with pytest.raises(ArithmeticError):oracle._snapshot(state|{'new_native_claim':1.},oracle.Ticket())
    with pytest.raises(ArithmeticError):oracle._snapshot(state|{'lots':[{'new_lot_claim':1.}]},oracle.Ticket())

def test_phase1_cannot_grant_publication_even_with_forged_reported_bounds(receipt):
    fake=deepcopy(receipt);fake.update(publication_allowed=True,native_root_work_upper=1,native_retained_bytes_upper=1,compact_stream_bytes_upper=1,unproved_gates=[])
    with pytest.raises(ArithmeticError,match='publication refused'):oracle.require_complete(fake)
    assert receipt['source_work_upper']is None and receipt['native_receipts_read']is False and receipt['producer_admission_hints_read']is False

def test_reference_census_never_calls_native_finance(raw,monkeypatch):
    from portfolio_risk.core import quant_native
    def forbidden(*a,**k):raise AssertionError('native finance callback in independent census')
    monkeypatch.setattr(quant_native,'call',forbidden)
    assert oracle.census(raw)['source_histories']==27

def test_raw_stock_search_and_ordering_dimensions_match_hand_census(receipt):
    a=receipt['compiled']['completed_candidate'];b=receipt['compiled']['revised_candidate']
    names=('loan_postings','lot_visits','loan_two_stock_search_steps_upper','loan_reconcile_stock_search_steps_upper','loan_stock_clone_entries_upper','loan_legal_clone_entries_upper')
    assert tuple(a[k]for k in names)==(36,22,1296,1656,216,132)
    assert tuple(b[k]for k in names)==(60,36,1872,2376,312,188)
    assert receipt['prefix']['ordering_comparison_steps_upper']==401408
    assert receipt['prefix']['ordered_event_workspace_bytes_upper']==8192
    assert receipt['prefix']['fee_clock_comparison_work_upper']==32
    assert set(receipt['dictionary']['zero_gross_fee_labels'])==set(oracle.FEE_GLS)


def test_zero_hazards_do_not_prune_complete_recovery_tail_reservation(raw):
    source=deepcopy(raw['completed_candidate']['commercial']['units'][0]['graph']['positions'][0]['original_pricing'][0]['source'])
    before=oracle._source_dimensions(source,oracle.Ticket())
    for interval in source['credit']['intervals']:
        interval['complete_recovery_tails']=[[dict(lag_years=0.,weight=.5),dict(lag_years=1./365.,weight=.5)]for _ in range(3)]
    after=oracle._source_dimensions(source,oracle.Ticket())
    assert before['maximum_recoveries']==1 and after['maximum_recoveries']==2
    assert after['reference_work_upper']>before['reference_work_upper']
    assert after['reference_bytes_upper']>before['reference_bytes_upper']


def test_original_q_expected_use_is_not_claimed_native_retained(receipt):
    assert receipt['source_coverage']==dict(expected_uses=27,original_q_live_uses=3,expected_current_native_source_uses=24,original_q_native_capture_verified=False)


def test_cancellation_during_raw_census_never_enters_financial_projection(raw,monkeypatch):
    from portfolio_risk.analytics import commercial_credit_reference
    calls=0
    def check():
        nonlocal calls
        calls+=1
        if calls==31:raise RuntimeError('original census cancellation')
    def forbidden(*a,**k):raise AssertionError('cancelled census entered oracle')
    monkeypatch.setattr(commercial_credit_reference,'replay',forbidden)
    with pytest.raises(RuntimeError,match='original census cancellation'):oracle.census(raw,check)
    assert calls==31


def test_binding_and_accessor_dimensions_are_independently_derived(receipt):
    p=receipt['prefix']
    assert p['binding_identity_pair_shape']==dict(encoded_bytes_upper=1632,cells=48,declared_field_visits=40,emitted_fields=40,string_utf8_bytes=664)
    assert p['binding_comparison_work_upper']==1016
    assert p['future_binding_identity_shape']==dict(encoded_bytes_upper=832,cells=24,declared_field_visits=20,emitted_fields=20,string_utf8_bytes=348)
    assert p['accessor_lookup_work_upper']==16416
    assert p['scenario_clone_bytes_upper']==112
    assert p['accessor_string_workspace_bytes_upper']==80
    assert receipt['compiled']['completed_candidate']['fee_native_year_shape']['cells']==8
    assert receipt['compiled']['revised_candidate']['fee_native_year_shape']['cells']==16


def test_deterministic_interner_never_uses_membership_or_hash_probes():
    class NoMembershipList(list):
        def __contains__(self,item):raise AssertionError('unbounded string membership')
    t=oracle.Ticket(strings=NoMembershipList())
    values=['a','aa','ab','a','é','😀','é','\\','"','\n']
    for value in values:oracle.shape(value,t)
    assert t.strings==['a','aa','ab','é','😀','\\','"','\n']
    assert t.work>sum(len(v)for v in values)
    assert oracle.shape('😀').string_utf8_bytes==4
    assert oracle.shape('\n').encoded_bytes_upper==4


def test_trie_capacity_and_closed_schema_capacity_fail_before_allocation():
    import sys
    assert sys.getsizeof([None]*257)<=2304
    assert sys.getsizeof([None]*129)<=1152
    t=oracle.Ticket(retained=oracle.MAX_BYTES-64)
    with pytest.raises(ArithmeticError,match='before growth'):oracle.shape('new',t)
    assert t.string_trie is None and t.strings==[]
    t=oracle.Ticket(retained=oracle.MAX_BYTES-oracle._FIELD_BYTES+1)
    with pytest.raises(ArithmeticError,match='before growth'):oracle.shape({'kind':'supplied','principal_payments':[]},t,strict_raw=True)
    assert t.field_grammar_charged is False


def test_closed_field_grammar_and_exact_dictionary_comparison_ignore_order():
    t=oracle.Ticket()
    assert oracle.shape(dict(kind='supplied',principal_payments=[]),t,strict_raw=True)==oracle.shape(dict(principal_payments=[],kind='supplied'),t,strict_raw=True)
    assert oracle._exact({'long-key':1.,'a':[0.,True]},{'a':[0.,True],'long-key':1.},t)
    assert not oracle._exact({'long-key':1.,'a':[0.,True]},{'a':[-0.,True],'long-key':1.},t)
    with pytest.raises(ArithmeticError,match='field layout'):oracle.shape({'kind':'supplied','principal_payments':[],'unknown':1},t,strict_raw=True)
    with pytest.raises(ArithmeticError,match='field layout'):oracle.shape({'kínd':'supplied','principal_payments':[]},t,strict_raw=True)
    assert t.field_grammar_charged
