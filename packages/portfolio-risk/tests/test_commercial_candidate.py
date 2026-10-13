"""Independent raw purchased commercial graph and ordered journal gates."""
from copy import deepcopy
from pathlib import Path
import json
import pytest
from portfolio_risk.core.commercial_candidate_native import run
from portfolio_risk.analytics.commercial_candidate_reference import verify
ROOT=Path(__file__).resolve().parents[3]

def raw():return json.loads((ROOT/'examples/models/commercial-candidate-journal-1.json').read_text())


def test_purchased_native_commercial_candidate_replays_complete_graph_and_journal():
    request=raw();result,_=run(request);receipt=verify(request,result)
    assert receipt['purchased_journal_replayed']and receipt['complete_tail_replayed']
    assert result['full_end_ordinal']>result['reporting_end_ordinal']
    assert result['opening_identity_verified']is False


@pytest.mark.parametrize('target',['allocation','journal','source','origin','bool','completion'])
def test_purchased_candidate_nested_finance_dates_and_scalar_injection_fail(target):
    request=raw();result,_=run(request)
    if target=='allocation':result['preparation']['allocation_replay']['total_asset_purchase_cost']+=1.
    elif target=='journal':result['tables']['journal'][2]['value']+=1.
    elif target=='source':result['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources'][0]['projection']['accounting']['groups'][0]['next']['gross_carry']+=1.
    elif target=='origin':result['tables']['commercial_source_lineage'][0]['source_origin_day']+=1
    elif target=='bool':result['tables']['commercial_source_lineage'][0]['native_atomic_group']['next']['principal']=True
    else:result['producer_completion']['protocol']=True
    with pytest.raises((ArithmeticError,ValueError)):verify(request,result)


def issuer_request():
    request=raw()
    def select(value):
        if isinstance(value,dict):
            if 'credit'in value and 'source'in value:value['credit']['instrument_kind']='issuer_credit_debt'
            for child in value.values():select(child)
        elif isinstance(value,list):
            for child in value:select(child)
    select(request['commercial'])
    for unit in request['commercial']['units']:
        for position in unit['graph']['positions']:position['credit_identity']='issuer_credit_debt'
    request['specification']['positions'][0].update(kind='security',duration=0.,opening_market_price=1.)
    return request


def test_ac_issuer_candidate_retains_security_owner_in_every_lineage_and_book():
    request=issuer_request();result,_=run(request);assert verify(request,result)['purchased_journal_replayed']
    for table in ('commercial_source_lineage','commercial_daily','commercial_closing'):
        assert result['tables'][table]and all(r['source_class']=='issuer_ac_security-1'for r in result['tables'][table])
    changed=deepcopy(result);changed['tables']['commercial_source_lineage'][0]['source_class']='commercial_loan-1'
    with pytest.raises(ArithmeticError):verify(request,changed)


@pytest.mark.parametrize('guard',['loan','mark','duration','facility'])
def test_issuer_candidate_rejects_incompatible_asset_or_funding_owner(guard):
    request=issuer_request();position=request['specification']['positions'][0]
    if guard=='loan':position['kind']='loan'
    elif guard=='mark':position['opening_market_price']=.9
    elif guard=='duration':position['duration']=2.
    else:
        graph=request['commercial']['units'][0]['graph'];graph['positions'][0]['histories'][0]['physical'][0]['source']['credit']['opening_undrawn_commitment']=10.
    with pytest.raises((ValueError,RuntimeError)):run(request)
