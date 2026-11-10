"""Original selected cut mixed bytes cannot hide routing or size changes."""
import asyncio
import json
import math

from fastapi import HTTPException
import pytest
from starlette.requests import Request
from app.model_routes import _selected_cd_body

SCHEMA='commercial-cut-mixed-journal-1'


def read(body):
    async def receive():return dict(type='http.request',body=body,more_body=False)
    return asyncio.run(_selected_cd_body(Request(
        dict(type='http',method='POST',path='/models/runs',headers=[]),receive)))


@pytest.mark.parametrize('body',[
    b'{"schema_name":"commercial-cut-mixed-journal-1","schema_name":"consumer-installment-1","request":{}}',
    b'{"schema_name":"commercial-cut-mixed-journal-1","request":{"fee_cost":1.0,"fee_cost":1.0}}',
])
def test_original_selected_cut_mixed_duplicate_cannot_be_overwritten(body):
    with pytest.raises(HTTPException,match='duplicate'):read(body)


def test_cut_mixed_raw_bound_includes_unparsed_whitespace():
    body=json.dumps(dict(schema_name=SCHEMA,request={})).encode()+b' '*(2*1024*1024)
    with pytest.raises(HTTPException,match='raw bytes'):read(body)


def test_original_cut_mixed_transport_preserves_money_categories_and_signed_zero():
    raw=dict(path_key=1,amount=1.,zero=-0.,reviewed=True)
    actual,started=read(json.dumps(dict(schema_name=SCHEMA,request=raw)).encode())
    assert type(started) is float
    assert type(actual['path_key']) is int and type(actual['amount']) is float
    assert actual['reviewed'] is True and math.copysign(1.,actual['zero']) == -1.
