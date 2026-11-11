"""Original selected HTTP bytes retain the declared bounded native lane."""
import asyncio
import json

from fastapi import HTTPException
import pytest
from starlette.requests import Request

from app.main import _selected_commercial_ledger_body
from app.schemas import ModeledBalanceStressRequest

SCHEMA = 'commercial-generated-candidate-journal-1'


def read(body):
    async def receive(): return dict(type='http.request', body=body, more_body=False)
    return asyncio.run(_selected_commercial_ledger_body(Request(
        dict(type='http', method='POST', path='/balance-stress/modeled', headers=[]), receive)))


@pytest.mark.parametrize('body', [
    b'{"specification":{"schema":"commercial-generated-candidate-journal-1","schema":"graph-dated-ledger-1"}}',
    b'{"specification":{"schema":"commercial-generated-candidate-journal-1","weight":1.0,"weight":1.0}}',
    b'{"specification":{"schema":"commercial-credit-ledger-1","schema":"graph-dated-ledger-1"}}',
])
def test_original_selected_schema_cannot_be_overwritten(body):
    with pytest.raises(HTTPException, match='duplicate'): read(body)


def test_generated_raw_bound_includes_unparsed_whitespace():
    body = json.dumps(dict(specification=dict(schema=SCHEMA))).encode() + b' ' * (1024 * 1024)
    with pytest.raises(HTTPException, match='raw bytes'): read(body)


def test_generated_original_body_preserves_scalar_categories():
    raw = dict(schema=SCHEMA, weight=1.0, path_key=1, review=True)
    result, started = read(json.dumps(dict(specification=raw)).encode())
    assert type(started) is float
    assert type(result['weight']) is float and type(result['path_key']) is int and result['review'] is True


def test_generated_pydantic_admission_uses_selected_byte_tier():
    with pytest.raises(ValueError, match='byte bound'):
        ModeledBalanceStressRequest(expected_revision=0,
            specification=dict(schema=SCHEMA, padding='x' * (1024 * 1024)))
