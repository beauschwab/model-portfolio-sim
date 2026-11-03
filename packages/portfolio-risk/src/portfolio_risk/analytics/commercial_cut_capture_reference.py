"""Independent unpublished finite source-capture verification.

Only explicit validation calls this module. Captured money never supplies an
expected value. Reported preparation/retention bounds remain producer metadata;
these checks neither establish the complete resource proof nor permit publication.
"""
from dataclasses import dataclass
import hashlib
import json
import math

from .commercial_cut_receipts_reference import authority
from .commercial_dto import graph_identity
from .modeled_json_identity import exact_json_identity

MAX_STREAM = 4 << 20
MAX_RAW = 2 << 20
ROLES = ('physical', 'pricing', 'pricing_down', 'pricing_up')
COUNT_KEYS = ('original_pricing', 'original_physical', *ROLES)
RECORD_KEYS = {'schema', 'side', 'unit_index', 'unit_id', 'scope',
               'position_index', 'scenario_index', 'history_index', 'role', 'proof'}
PROOF_KEYS = {'position_id', 'scenario_id', 'run_id', 'path_id', 'path_key',
              'weight', 'role', 'source_request', 'cash', 'projection'}
FALSE_RECEIPT = ('retains_typed_original_economic_owners',
                 'original_quote_or_anchor_refitted', 'cut_sim_executed',
                 'cut_admission_granted', 'complete_source_sim_codec_bounds',
                 'independent_persisted_verification', 'publication_eligible')
INTEGER_RECEIPT = ('typed_request_bytes', 'source_preparation_work_upper',
                   'raw_prefix_work_upper', 'capture_work', 'admitted_work_upper',
                   'reported_retained_bytes_upper', 'encoded_bytes', 'source_records')


def _require(ok, message):
    if not ok:
        raise ArithmeticError(message)


def _uint(value, label, limit=(1 << 64) - 1):
    _require(type(value) is int and 0 <= value <= limit,
             f'exact unsigned capture {label} required')
    return value


def _finite_tree(value, depth=0):
    _require(depth <= 128, 'capture JSON depth exceeds 128')
    if type(value) is dict:
        for key, child in value.items():
            _require(type(key) is str, 'capture object key must be text')
            _require(not any(0xD800 <= ord(c) <= 0xDFFF for c in key),
                     'capture Unicode key scalar required')
            _finite_tree(child, depth + 1)
    elif type(value) is list:
        for child in value:
            _finite_tree(child, depth + 1)
    elif type(value) is float:
        _require(math.isfinite(value), 'capture nonfinite number')
    elif type(value) is int:
        _require(-(1 << 63) <= value < (1 << 64), 'capture integer outside native JSON domain')
    else:
        _require(value is None or type(value) in (str, bool), 'capture JSON scalar required')
        if type(value) is str:
            _require(not any(0xD800 <= ord(c) <= 0xDFFF for c in value), 'capture Unicode scalar required')


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        _require(key not in result, f'duplicate capture JSON key: {key}')
        result[key] = value
    return result


def strict_loads(data):
    """Reject duplicate keys, nonfinite tokens/overflow, depth and integer drift."""
    try:
        def invalid(value):
            raise ArithmeticError(f'capture nonfinite token: {value}')
        text = data.decode('utf8') if type(data) is bytes else data
        result = json.loads(text, object_pairs_hook=_pairs, parse_constant=invalid)
        _finite_tree(result)
        return result
    except (ValueError, UnicodeError, RecursionError) as error:
        raise ArithmeticError('invalid capture JSON') from error


@dataclass(frozen=True)
class Expected:
    metadata: dict
    proof_metadata: dict
    history: dict
    graph: dict
    position: dict
    original: bool


def expected_sources(raw):
    """Derive exact native observer order solely from the selected raw DTO."""
    _finite_tree(raw)
    _require(type(raw) is dict and raw.get('schema') == 'commercial-cut-mixed-journal-1',
             'explicit raw cut schema required')
    authority(raw)
    return _finite_source_order(raw)


def finite_source_order(raw):
    """Finite financial witness ordering only; grants no admission authority.

    The selected capture entrypoint above still requires its original resource
    authority. Financial component replay also covers ordinary origination.
    """
    _finite_tree(raw)
    _require(type(raw) is dict and raw.get('schema') == 'commercial-cut-mixed-journal-1',
             'explicit raw cut schema required')
    return _finite_source_order(raw)


def _finite_source_order(raw):
    expected = []
    for side, key in (('completed', 'completed_candidate'), ('revised', 'revised_candidate')):
        candidate = raw[key]
        for u, unit in enumerate(candidate['commercial']['units']):
            graph = unit['graph']
            for p, position in enumerate(graph['positions']):
                _require(not position.get('generated_policy') and not position.get('exercise')
                         and not position.get('original_physical'),
                         'capture requires finite nonoptional owners without original Physical')
                def append(scope, scenario_index, index, role, history, scenario, original):
                    weight = history['weight']
                    _require(type(weight) in (int, float) and math.isfinite(weight)
                             and 0. <= weight <= 1., 'finite source probability required')
                    paths = history['source']['credit']['paths']
                    _require(len(paths) == 1, 'one finite source path required')
                    path_key = _uint(paths[0]['path_key'], 'path_key')
                    metadata = dict(schema='commercial-cut-source-record-1', side=side,
                        unit_index=u, unit_id=unit['unit_id'], scope=scope,
                        position_index=p, scenario_index=scenario_index,
                        history_index=index, role=role)
                    proof = dict(position_id=position['position_id'],
                        scenario_id=scenario['scenario_id'], run_id=scenario['run_id'],
                        path_id=history['path_id'], path_key=path_key,
                        weight=float(weight), role=role)
                    expected.append(Expected(metadata, proof, history, graph, position, original))
                for h, history in enumerate(position['original_pricing']):
                    append('original_pricing', None, h, 'pricing', history,
                           graph['original_scenario'], True)
                for s, scenario in enumerate(graph['scenarios']):
                    matches = [h for h in position['histories']
                               if h['scenario_id'] == scenario['scenario_id']]
                    _require(len(matches) == 1, 'exact raw scenario coverage required')
                    index = 0
                    for role in ROLES:
                        for history in matches[0][role]:
                            append('current', s, index, role, history, scenario, False)
                            index += 1
    _require(0 < len(expected) <= 65536, 'bounded nonempty source coverage required')
    return expected


def _parse(data):
    _require(type(data) is bytes and 0 < len(data) <= MAX_STREAM,
             'capture bytes exceed 4MiB component or are empty')
    _require(data.endswith(b'\n'), 'capture must end with complete newline')
    lines = data.split(b'\n')[:-1]
    _require(2 <= len(lines) <= 65537 and all(lines), 'capture empty/incomplete record')
    rows = [strict_loads(line) for line in lines]
    _require(all(type(row) is dict for row in rows), 'capture object records required')
    return rows


def source_traversal_work(value):
    """Independent JSON cell/string/numeric upper for prepaid built-in records."""
    if value is None or type(value) is bool:
        return 1
    if type(value) in (int, float):
        return 33
    if type(value) is str:
        return 1 + len(value.encode('utf8'))
    if type(value) is list:
        return 1 + sum(map(source_traversal_work, value))
    if type(value) is dict:
        return 1 + sum(1 + len(key.encode('utf8')) + source_traversal_work(child)
                       for key, child in value.items())
    raise ArithmeticError('prepaid capture JSON value required')


def validate_layout(raw, data, receipt, check=lambda: False):
    """Verify bytes/metadata only; financial replay remains explicitly false."""
    check()
    expected = expected_sources(raw)
    rows = _parse(data)
    _finite_tree(receipt)
    prepaid = receipt.get('schema') == 'commercial-cut-source-capture-receipt-2'
    fields = {'schema', 'counts', 'all_selected_source_records_captured',
              *FALSE_RECEIPT, *INTEGER_RECEIPT}
    if prepaid:
        fields.add('source_traversal_work')
    _require(set(receipt) == fields, 'capture receipt fields differ')
    _require(receipt['schema'] in ('commercial-cut-source-capture-receipt-1',
                                   'commercial-cut-source-capture-receipt-2'),
             'capture receipt schema differs')
    for key in INTEGER_RECEIPT:
        _uint(receipt[key], key)
    _require(receipt['all_selected_source_records_captured'] is True
             and all(receipt[key] is False for key in FALSE_RECEIPT),
             'capture completion/publication eligibility differs')
    counts = {key: 0 for key in COUNT_KEYS}
    for item in expected:
        key = item.metadata['scope'] if item.original else item.metadata['role']
        counts[key] += 1
    for value in (receipt['counts'], rows[-1].get('counts')):
        _require(type(value) is dict and set(value) == set(COUNT_KEYS),
                 'capture count fields differ')
        for key in COUNT_KEYS:
            _uint(value[key], key)
        _require(value == counts, 'capture raw role counts differ')
    footer = dict(schema='commercial-cut-source-capture-complete-1',
                  source_records=len(expected), counts=counts, capture_complete=True,
                  cut_sim_executed=False, publication_eligible=False)
    _require(exact_json_identity(footer, rows[-1]), 'exact capture footer differs')
    _require(len(rows) == len(expected) + 1 and receipt['source_records'] == len(expected),
             'complete raw/native capture coverage differs')
    for item, row in zip(expected, rows[:-1], strict=True):
        check()
        _require(type(row) is dict and set(row) == RECORD_KEYS, 'capture record fields differ')
        metadata = {key: value for key, value in row.items() if key != 'proof'}
        _require(exact_json_identity(item.metadata, metadata), 'exact capture order/role metadata differs')
        proof = row['proof']
        keys = PROOF_KEYS | ({'fee_projection'} if item.position.get('fees') else set())
        _require(type(proof) is dict and set(proof) == keys, 'full source proof fields differ')
        actual = {key: proof[key] for key in item.proof_metadata}
        _require(exact_json_identity(item.proof_metadata, actual), 'exact capture source metadata differs')
        graph_identity(item.history['source'], proof['source_request'])
        _require(type(proof['cash']) is list and type(proof['projection']) is dict,
                 'full source cash/projection required')
    _require(receipt['encoded_bytes'] == len(data), 'capture actual byte count differs')
    work = 2 * len(data) + 16 * len(expected) + 2048
    if prepaid:
        _uint(receipt['source_traversal_work'], 'source_traversal_work')
        traversal = sum(map(source_traversal_work, rows[:-1]))
        _require(receipt['source_traversal_work'] == traversal,
                 'prepaid capture source traversal differs')
        work += traversal
    _require(receipt['capture_work'] == work, 'capture actual logical work differs')
    _require(0 < receipt['typed_request_bytes'] <= MAX_RAW
             and receipt['raw_prefix_work_upper'] == 8 * receipt['typed_request_bytes'],
             'capture raw-prefix component counter differs')
    _require(receipt['admitted_work_upper'] == work + receipt['raw_prefix_work_upper']
             + receipt['source_preparation_work_upper'] <= 32_000_000,
             'capture joined work counter/cap differs')
    _require(0 < receipt['reported_retained_bytes_upper'] <= 128 << 20,
             'capture reported retention cap differs')
    return dict(source_records=len(expected), counts=counts, encoded_bytes=len(data),
                capture_work=work, layout_verified=True, financial_replay=False,
                complete_resource_proof=False, publication_eligible=False)


def verify_capture(raw, data, receipt, check=lambda: False):
    """Independently replay every selected original/current loan and fee tail."""
    result = validate_layout(raw, data, receipt, check)
    from .commercial_graph_reference import _economic
    from .commercial_ledger_reference import verify_compiler
    from .commercial_reference_math import _cash, _price
    from .commercial_source_reference import compare
    from . import commercial_fee_adoption_reference as fees
    rows = _parse(data)[:-1]
    original_quotes = {}
    fee_validated = set()
    physical = pricing = 0
    for item, row in zip(expected_sources(raw), rows, strict=True):
        check()
        source = item.history['source']
        proof = row['proof']
        role = item.metadata['role']
        if role == 'physical':
            reference = verify_compiler(source, proof['projection'], check)['replay']
            physical += 1
        else:
            reference = _economic(source, proof['projection'], check)
            pricing += 1
        cash = _cash(reference['credit'])
        compare(cash, proof['cash'], '$.capture.cash', check)
        selection = item.position.get('fees')
        fee_projection = None
        if selection:
            position_key = (item.metadata['side'], item.metadata['unit_index'],
                            item.metadata['position_index'])
            if position_key not in fee_validated:
                fees.validate(selection, item.graph, item.position, check)
                fee_validated.add(position_key)
            fee_projection = fees.project(selection, item.proof_metadata['scenario_id'],
                role, item.history, reference, proof['fee_projection'], check,
                original=item.original)
        if item.original:
            key = (item.metadata['side'], item.metadata['unit_index'], item.metadata['position_index'])
            quote = original_quotes.setdefault(key, [[], [], item])
            target = item.position['oas_target']
            _require(target.get('kind') == 'locked', 'capture original quote needs retained locked OAS')
            curve = item.graph['original_scenario']['collateral_curve']
            asof = item.graph['as_of_ordinal']
            quote[0].append(item.history['weight'] * _price(cash, curve, asof, asof,
                                                          target['base_oas'], 0.))
            if fee_projection:
                quote[1].append(item.history['weight'] * fees.price(fee_projection, curve,
                                                                    asof, asof, original=True))
    for loan_parts, fee_parts, item in original_quotes.values():
        compare(float(item.position['original_signed_cash_quote']), float(math.fsum(loan_parts)),
                '$.capture.original_loan_quote', check)
        if item.position.get('fees'):
            compare(float(item.position['fees']['original_signed_fee_quote']),
                    float(math.fsum(fee_parts)), '$.capture.original_fee_quote', check)
    result.update(financial_replay=True, physical_sources_verified=physical,
                  pricing_sources_verified=pricing, original_quote_positions=len(original_quotes),
                  stream_sha256=hashlib.sha256(data).hexdigest(),
                  source_preparation_work_upper_reported=receipt['source_preparation_work_upper'],
                  reported_retained_bytes_upper=receipt['reported_retained_bytes_upper'])
    return result
