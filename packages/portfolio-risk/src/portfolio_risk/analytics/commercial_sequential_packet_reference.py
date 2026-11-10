"""Independent identity/financial reader for an unpublished native packet.

Original proofs and current preparation proofs are resolved by ordered raw
metadata. No money is inferred or checkpoint reconstructed. Producer resource
reports are not inputs, and this reader cannot grant publication authority.
"""
from .commercial_cut_capture_reference import _finite_tree, finite_source_order
from .commercial_dto import graph_typed
from .commercial_sequential_cut_reference import verify_parts
from .modeled_json_identity import exact_json_identity


def _require(ok, message):
    if not ok:
        raise ArithmeticError(message)


def financial_parts(raw, packet, check=lambda: False):
    check()
    _require(type(packet) is dict, 'complete direct packet required')
    stream = packet.get('schema') == 'commercial-sequential-packet-witness-2'
    keys = set('''schema raw_request_header completed_phase revised_phase component_receipt
        joined_raw_admission_proved publication_eligible'''.split())
    keys.update(('cut_stream',) if stream else ('cut_tables', 'producer_completion'))
    _require(set(packet) == keys, 'complete direct packet required')
    _finite_tree(packet)
    _require(packet['schema'] in ('commercial-sequential-packet-witness-1',
                                 'commercial-sequential-packet-witness-2')
             and packet['joined_raw_admission_proved'] is False
             and packet['publication_eligible'] is False, 'direct packet evidence boundary differs')
    canonical = graph_typed(raw)
    _require(set(canonical) == {'schema', 'cut_ordinal', 'completed_candidate', 'revised_candidate'},
             'direct packet explicit ordinary request required')
    _require(exact_json_identity(packet['raw_request_header'],
        {k: canonical[k] for k in ('schema', 'cut_ordinal')}), 'direct packet raw header differs')
    phases = {}
    for side in ('completed', 'revised'):
        phase = packet[side + '_phase']
        _require(type(phase) is dict and set(phase) == {'original_sources', 'preparation'}
                 and type(phase['original_sources']) is list, 'direct packet phase fields differ')
        _require(exact_json_identity(canonical[side + '_candidate'], phase['preparation']['raw_request']),
                 'direct packet retained complete candidate request differs')
        phases[side] = phase
    records = []
    originals = dict(completed=0, revised=0)
    current = {}
    expected = finite_source_order(canonical)
    for item in expected:
        metadata = item.metadata
        side = metadata['side']
        phase = phases[side]
        if metadata['scope'] != 'current':
            index = originals[side]
            _require(index < len(phase['original_sources']), 'direct packet original proof missing')
            record = phase['original_sources'][index]
            _require(type(record) is dict and 'proof' in record
                     and exact_json_identity(metadata, {k: v for k, v in record.items() if k != 'proof'}),
                     'direct packet original proof order/identity differs')
            originals[side] += 1
        else:
            unit = metadata['unit_index']
            graphs = phase['preparation']['native_preparation']['source']['native_graphs']
            _require(type(graphs) is list and unit < len(graphs), 'direct packet graph coverage differs')
            sources = graphs[unit]['native_sources']
            index = current.get((side, unit), 0)
            _require(type(sources) is list and index < len(sources), 'direct packet current proof missing')
            record = dict(metadata, proof=sources[index])
            current[side, unit] = index + 1
        records.append(record)
    for side, phase in phases.items():
        _require(originals[side] == len(phase['original_sources']), 'direct packet extra original proof')
        graphs = phase['preparation']['native_preparation']['source']['native_graphs']
        _require(len(graphs) == len(canonical[side + '_candidate']['commercial']['units']),
                 'direct packet complete unit graph coverage differs')
        for unit, graph in enumerate(graphs):
            _require(current.get((side, unit), 0) == len(graph['native_sources']),
                     'direct packet extra current proof')
    if stream:
        from .commercial_cut_stream_transport import decode
        tables, completion = decode(packet['cut_stream'], check)
    else:
        tables, completion = packet['cut_tables'], packet['producer_completion']
    return dict(raw_request=canonical, completed_preparation=phases['completed']['preparation'],
        revised_preparation=phases['revised']['preparation'], cut_tables=tables,
        producer_completion=completion, component_receipt=packet['component_receipt'],
        source_records=records, joined_raw_admission_proved=False, publication_eligible=False)


def verify(raw, packet, check=lambda: False):
    check()
    parts = financial_parts(raw, packet, check)
    return verify_parts(parts['raw_request'], parts, check)
