"""Generated source partitions preserve exact cells and enforce phase bounds."""
import json

import pytest

from portfolio_risk.analytics import commercial_generated_transport as transport


def writer(tmp_path):
    return transport.GeneratedWriter(tmp_path, 2, check=lambda: False)


def test_metadata_is_partitioned_one_owner_at_a_time(tmp_path):
    output = writer(tmp_path)
    name = 'commercial_candidate_native_sources'
    for i in range(5):
        native = {'weight': .125, 'path_key': i, 'nested': {'signed': -0.0}}
        output.add(name, [{'unit_index': 0, 'source_index': i, 'native_json': native}])
        assert not output.buffers[name] if name in output.buffers else True
    output.finish()
    output.admit_persisted()
    assert len(output.parts[name]) == 5
    for i, frame in enumerate(output.frames(name)):
        row = frame.to_dicts()[0]
        assert row['source_index'] == i
        assert row['native_json'] == json.dumps({'weight': .125, 'path_key': i,
            'nested': {'signed': -0.0}}, separators=(',', ':'))


@pytest.mark.parametrize('row', [
    {'unit_index': True, 'source_index': 0, 'native_json': {}},
    {'unit_index': 0, 'source_index': -1, 'native_json': {}},
    {'unit_index': 0, 'source_index': 0, 'native_json': []},
    {'unit_index': 0, 'source_index': 0, 'native_json': {}, 'extra': 1},
    {'unit_index': 0, 'source_index': 0, 'native_json': {'cash': float('nan')}},
])
def test_native_metadata_shape_and_scalar_refusals(tmp_path, row):
    with pytest.raises(ValueError):
        writer(tmp_path).add('commercial_candidate_native_sources', [row])


def test_partition_tampering_is_refused_before_replay(tmp_path):
    output = writer(tmp_path)
    name = 'commercial_candidate_units'
    output.add(name, [{'unit_index': 0, 'source_index': 0, 'native_json': {'price': 100.}}])
    part = output.parts[name][0]
    path = tmp_path/part['path']
    data = bytearray(path.read_bytes()); data[len(data)//2] ^= 1; path.write_bytes(data)
    with pytest.raises(ValueError, match='checksum'):
        output.admit_persisted()


def test_generated_serialized_phase_refuses_limit(tmp_path, monkeypatch):
    monkeypatch.setattr(transport, 'SERIALIZED_BYTES', 1)
    with pytest.raises(ValueError, match='phase byte/object'):
        writer(tmp_path).add('commercial_candidate_units',
            [{'unit_index': 0, 'source_index': 0, 'native_json': {}}])


@pytest.mark.parametrize('model,protocol,record', [
    (transport.SCHEMA, 6, 'commercial_generated_complete'),
    (transport.FEE_SCHEMA, 7, 'commercial_generated_fee_complete'),
])
def test_native_trailer_cannot_claim_verification_or_different_paths(model, protocol, record):
    header = {'model_schema': model, 'source_receipt': {'physical_paths': 256,
        'maximum_live_full_physical_owners': 1, 'full_end_ordinal': 740000}}
    if model == transport.FEE_SCHEMA:
        header['source_receipt']['maximum_live_full_fee_owners'] = 1
    trailer = dict(record=record, complete=True,
        schema=model, protocol=protocol, **header['source_receipt'],
        independent_persisted_verification=False, opening_identity_verified=False,
        mixed_ledger_adoption=False, empirical_validation=False)
    transport.validate_trailer(header, trailer)
    for changed in (dict(trailer, physical_paths=255), dict(trailer, protocol=True),
                    dict(trailer, independent_persisted_verification=True),
                    dict(trailer, extra=False)):
        with pytest.raises(ValueError): transport.validate_trailer(header, changed)

    for invalid_header in ({'source_receipt': header['source_receipt']},
                           dict(header, model_schema='unsupported-generated-model')):
        with pytest.raises(ValueError): transport.validate_trailer(invalid_header, trailer)
    if model == transport.FEE_SCHEMA:
        with pytest.raises(ValueError):
            transport.validate_trailer(header, dict(trailer, maximum_live_full_fee_owners=2))
