"""Independent selected cut receipt/scan census; no financial calculation.

Construction counters remain native metadata. The cut reference separately
rebuilds raw graph/fee owners and every ordered financial row from source.
"""
import json
import math

POLICY = 'encode_native_preparations_before_value_decode-1'
SOURCE_POLICY = 'counted_finite_transfer_contract_factor_work-1'
C = 4 << 20
V = 64 << 20
K = 32 << 20


def authority(raw):
    if raw.get('preparation_receipt_policy') != POLICY:
        raise ArithmeticError('explicit native cut receipt policy required')
    for key in ('completed_candidate', 'revised_candidate'):
        for unit in raw[key]['commercial']['units']:
            for p in unit['graph']['positions']:
                if p.get('generated_policy'):
                    raise ArithmeticError('selected cut receipts require finite owners')
                sources = [*p['original_pricing'], *p.get('original_physical', [])]
                for h in p['histories']:
                    for role in ('physical', 'pricing', 'pricing_down', 'pricing_up'):
                        sources.extend(h[role])
                for h in sources:
                    if h['source']['source'].get('transfer', {}).get('source_work_policy') != SOURCE_POLICY:
                        raise ArithmeticError('selected cut receipt genuine transfer authority absent')


def retention(raw, wire, capacity, sources):
    if any(type(n) is not int or not 0 <= n < 1 << 64 for n in (raw, wire, capacity, sources)):
        raise ArithmeticError('cut exact receipt phase dimension required')
    inputs = 20 * (raw + wire)
    phases = dict(owned_wire_parse_phase_bytes=capacity + 40 * wire + 20 * raw,
        source_owner_phase_bytes=sources + inputs + K + 6 * C,
        raw_validation_phase_bytes=inputs + V,
        decoded_tables_phase_bytes=inputs + V + 6 * C,
        final_value_phase_bytes=V + 2 * (V // 20))
    if any(n >= 1 << 64 for n in phases.values()):
        raise ArithmeticError('cut receipt phase overflow')
    return phases


def shape(value):
    """JSON cells/escaped-string census with32 bytes per numeric token."""
    def plus(a, b): return tuple(x+y for x,y in zip(a,b,strict=True))
    if value is None: return (4,1,0)
    if type(value) is bool: return (len(str(value).lower()),1,0)
    if type(value) in (int,float):
        if not math.isfinite(value): raise ArithmeticError('cut nonfinite structural scalar')
        return (32,1,0)
    if type(value) is str:
        n=len(json.dumps(value,ensure_ascii=False,separators=(',',':')).encode())
        return (n,1,n)
    if type(value) is list:
        result=(2+max(0,len(value)-1),1,0)
        for item in value: result=plus(result,shape(item))
        return result
    if type(value) is dict:
        result=(2+max(0,len(value)-1)+len(value),1,0)
        for key,item in value.items(): result=plus(plus(result,shape(key)),shape(item))
        return result
    raise ArithmeticError('cut structural JSON type differs')


def _shape(row):
    keys=('encoded_bytes_upper','cells_upper','string_bytes_upper')
    if type(row) is not dict or set(row)!=set(keys) or any(type(row[k]) is not int or not 0<=row[k]<1<<64 for k in keys):
        raise ArithmeticError('cut exact structural dimension required')
    return tuple(row[k] for k in keys)


def _scan(row):
    a,b,_=_shape(row)
    return a+b


def _table_total(tables):
    result=[2,1,0]
    for name,row in tables.items():
        array=list(_shape(row['shape_upper']))
        array[0]+=2+max(0,row['rows_upper']-1)
        array[1]+=1
        result=[a+b+c for a,b,c in zip(result,shape(name),array,strict=True)]
        result[0]+=2
    return tuple(result)



SCOPE = "one genuine native completed Sim prefix moves its complete live state into a distinct resumed Sim; original source clocks, gross rights, policy counters, original loan and fee anchors retained; new future commercial AC owners append zero state and acquire once; no deserialized monetary checkpoint, observed bank opening, native prior publication, arbitrary mixed owners or empirical/accounting approval"
RESULT_KEYS = "schema raw_source completed_preparation revised_preparation cut_tables producer_completion scope selected_admission origin_asof_ordinal cut_ordinal cut_day full_end_ordinal admitted_work_upper retained_bytes_upper cut_component_work_reservation cut_component_bytes_reservation component_output_limit_bytes appended_candidates completed_prefix_daily_sim_executed_once resumed_future_daily_sim_executed_once complete_origin_source_owners_retained cut_started_mixed_sim_adoption original_credit_clock_restarted original_loan_anchor_refitted original_fee_anchor_refitted surviving_second_acquisition_posted prior_publication_verified opening_identity_verified general_mixed_book_adoption empirical_validation".split()


def descriptor_work(meta):
    completion=dict(complete=True,protocol=8,cut_day=0,native_cut_state_moved=True,
        original_calendar_retained=True,new_sim_started_at_cut=True,prior_publication_verified=False)
    values=[meta,meta,'commercial-cut-mixed-journal-1',SCOPE,completion,*RESULT_KEYS]
    return sum(sum(shape(x)[:2]) for x in values)+len(RESULT_KEYS)+2

def verify_census(meta, *, admitted):
    """Arithmetic evidence only; a refusal is never positive publication proof."""
    from .modeled_json_identity import exact_json_identity
    names='''schema initial_source_work_upper final_joined_work_upper owned_wire_bytes
        owned_wire_capacity typed_raw_bytes source_owner_retained_bytes_upper retention_phases
        completed_preparation_work_upper revised_preparation_work_upper completed_receipt_bytes
        revised_receipt_bytes descriptor_retained_bytes_upper cut_ledger_census work_components
        final_result_shape output_inventory'''.split()
    if set(meta)!=set(names) or meta['schema']!='commercial-cut-encoded-admission-1':
        raise ArithmeticError('cut selected receipt census identity differs')
    for k in names[1:-4]:
        if k=='retention_phases': continue
        if type(meta[k]) is not int or not 0<meta[k]<1<<64:
            raise ArithmeticError('cut exact positive construction counter required')
    if not (meta['owned_wire_bytes']<=meta['owned_wire_capacity'] and max(meta['typed_raw_bytes'],meta['owned_wire_bytes'])<=2<<20):
        raise ArithmeticError('cut original wire/canonical bounds differ')
    expected=retention(meta['typed_raw_bytes'],meta['owned_wire_bytes'],meta['owned_wire_capacity'],meta['source_owner_retained_bytes_upper'])
    if meta['descriptor_retained_bytes_upper']!=20*(64<<10):
        raise ArithmeticError('cut descriptor component reservation differs')
    for key in ('source_owner_phase_bytes','decoded_tables_phase_bytes'):
        expected[key]+=meta['descriptor_retained_bytes_upper']
    ledger=meta['cut_ledger_census']
    keys='''work_upper retained_bytes_upper specification_bytes original_compiled_work_upper
        future_compiled_work_upper original_compiled_bytes_upper future_compiled_bytes_upper comparison_work_upper'''.split()
    if set(ledger)!=set(keys) or any(type(ledger[k]) is not int or not 0<=ledger[k]<1<<64 for k in keys):
        raise ArithmeticError('cut exact shared ledger census differs')
    if (ledger['comparison_work_upper']!=8*(ledger['original_compiled_bytes_upper']+ledger['future_compiled_bytes_upper']) or
        ledger['work_upper']!=ledger['comparison_work_upper']+ledger['original_compiled_work_upper']+ledger['future_compiled_work_upper']+8*ledger['specification_bytes']):
        raise ArithmeticError('cut unchanged shared ledger work formula differs')
    inv=meta['output_inventory']
    invkeys='schema prefix visible all_stream visible_stream decoded_tables dictionary_upper all_flushes_upper census_work_upper stream_batch_retained_bytes_upper'.split()
    if set(inv)!=set(invkeys) or inv['schema']!='commercial-cut-output-inventory-1':
        raise ArithmeticError('cut full output inventory identity differs')
    for section in ('prefix','visible'):
        for table,row in inv[section].items():
            if (type(table) is not str or type(row) is not dict or set(row)!=set('rows_upper shape_upper fields template_scan_work_upper'.split()) or
                type(row['rows_upper']) is not int or row['rows_upper']<0 or type(row['template_scan_work_upper']) is not int or row['template_scan_work_upper']<0 or
                type(row['fields']) is not list or len(row['fields'])!=len(set(row['fields'])) or any(type(k) is not str for k in row['fields'])):
                raise ArithmeticError('cut exact table inventory differs')
            _shape(row['shape_upper'])
    if type(inv['stream_batch_retained_bytes_upper']) is not int or inv['stream_batch_retained_bytes_upper']<0:
        raise ArithmeticError('cut live stream batch/flush clone reservation differs')
    if (_shape(inv['decoded_tables'])!=_table_total(inv['visible']) or
        inv['stream_batch_retained_bytes_upper']!=40*(_table_total(inv['prefix'])[0]+_shape(inv['decoded_tables'])[0])):
        raise ArithmeticError('cut full row batch/clone shape reservation differs')
    expected['source_owner_phase_bytes']+=inv['stream_batch_retained_bytes_upper']
    if not exact_json_identity(expected,meta['retention_phases']):
        raise ArithmeticError('cut simultaneous receipt/batch phase inventory differs')
    work=meta['work_components']
    expected_work=dict(source_preparation=meta['initial_source_work_upper']-8*max(meta['typed_raw_bytes'],meta['owned_wire_bytes']),
        raw_prefix=8*max(meta['typed_raw_bytes'],meta['owned_wire_bytes']),
        preparation_receipt_scans=2*(meta['completed_receipt_bytes']+meta['revised_receipt_bytes']),
        structural_census=inv['census_work_upper'],independent_cut_guard=ledger['work_upper'],
        repeated_specification_scans=8*ledger['specification_bytes'],
        all_native_stream_passes=4*_scan(inv['all_stream']),visible_stream_passes=2*_scan(inv['visible_stream']),
        decoded_table_passes=2*_scan(inv['decoded_tables']),final_result_passes=3*_scan(meta['final_result_shape']))
    for key in ('original_raw_census','descriptor_scans','prepared_specification_census'):
        if type(work.get(key)) is not int or work[key]<0: raise ArithmeticError('cut real scan dimension required')
        expected_work[key]=work[key]
    if not exact_json_identity(work,expected_work) or any(type(n) is not int or not 0<=n<1<<64 for n in work.values()):
        raise ArithmeticError('cut complete staged work census differs')
    if descriptor_work(meta)!=work['descriptor_scans']:
        raise ArithmeticError('cut complete descriptor sizing scan differs')
    if (sum(work.values())!=meta['final_joined_work_upper'] or meta['completed_preparation_work_upper']!=meta['initial_source_work_upper'] or
        meta['revised_preparation_work_upper']!=meta['initial_source_work_upper']+2*meta['completed_receipt_bytes']):
        raise ArithmeticError('cut source/receipt/final work stages differ')
    if admitted and (meta['final_joined_work_upper']>32_000_000 or max(expected.values())>128<<20 or ledger['work_upper']>8_000_000 or ledger['retained_bytes_upper']>32<<20 or
        _shape(meta['final_result_shape'])[0]>V//20 or _shape(inv['visible_stream'])[0]>C or _shape(inv['decoded_tables'])[0]>C):
        raise ArithmeticError('cut independent component/joined caps refuse')
    return expected


def verify(raw, actual):
    authority(raw)
    meta=actual['selected_admission'];phases=verify_census(meta,admitted=True)
    from ._deposit_continuation_income_reference import native_json_length
    if descriptor_work(meta)!=meta['work_components']['descriptor_scans']:
        raise ArithmeticError('cut descriptor sizing scan differs')
    if sum(shape(raw)[:2])!=meta['work_components']['original_raw_census']:
        raise ArithmeticError('cut exact raw structural scan differs')
    ledger=meta['cut_ledger_census']
    if ledger['retained_bytes_upper']!=2*(ledger['original_compiled_bytes_upper']+ledger['future_compiled_bytes_upper'])+4096*len(raw['revised_candidate']['specification']['positions'])+20*ledger['specification_bytes']:
        raise ArithmeticError('cut shared retained component formula differs')
    for key,work,receipt in (('completed_preparation',meta['completed_preparation_work_upper'],meta['completed_receipt_bytes']),
                             ('revised_preparation',meta['revised_preparation_work_upper'],meta['revised_receipt_bytes'])):
        prep=actual[key]
        if native_json_length(prep)!=receipt or prep['admitted_work_upper']!=work or prep['retained_bytes_upper']!=phases['source_owner_phase_bytes']-meta['output_inventory']['stream_batch_retained_bytes_upper']:
            raise ArithmeticError('cut complete native receipt scan/phase identity differs')
    if actual['retained_bytes_upper']!=max(phases.values()) or actual['admitted_work_upper']!=meta['final_joined_work_upper'] or actual['cut_component_work_reservation']!=meta['cut_ledger_census']['work_upper']:
        raise ArithmeticError('cut final admitted dimensions differ')
    inv=meta['output_inventory']
    for name,rows in actual['cut_tables'].items():
        if name not in inv['visible']:raise ArithmeticError('cut uninventoried emitted table')
        bound=inv['visible'][name]
        if len(rows)>bound['rows_upper'] or any(set(row)!=set(bound['fields']) for row in rows):
            raise ArithmeticError('cut emitted complete field/row inventory differs')
        total=tuple(sum(shape(row)[i] for row in rows) for i in range(3))
        if any(a>b for a,b in zip(total,_shape(bound['shape_upper']),strict=True)):
            raise ArithmeticError('cut complete emitted row shape exceeds inventory')
    final=(native_json_length(actual),*shape(actual)[1:])
    if any(a>b for a,b in zip(final,_shape(meta['final_result_shape']),strict=True)):
        raise ArithmeticError('cut complete final parsed receipt/cells exceed inventory')


PUBLICATION_GATE = ('selected native cut publication refused: independent raw-derived '
    'source/Compiled/output admission census is incomplete')


def require_raw_derived_admission():
    """Arithmetic/native metadata agreement cannot authorize publication."""
    raise ArithmeticError(PUBLICATION_GATE)
