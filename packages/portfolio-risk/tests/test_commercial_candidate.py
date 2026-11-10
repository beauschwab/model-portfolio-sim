"""Independent raw purchased commercial graph and ordered journal gates."""
from copy import deepcopy
from pathlib import Path
import json
import math
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


@pytest.mark.parametrize('face',[0.,100.])
def test_cached_schedule_preserves_realized_default_recovery_and_zero_weight_sources(face):
    request=raw()
    for allocation in request['allocation']:allocation['face']=face
    for unit in request['commercial']['units']:
        for position in unit['graph']['positions']:
            originals=[*position['original_pricing'],*position.get('original_physical',[])]
            current=[source for histories in position['histories']
                     for role in ('physical','pricing','pricing_down','pricing_up')
                     for source in histories[role]]
            for history in [*originals,*current]:
                credit=history['source']['credit']
                credit['draw_stream']=dict(kind='supplied',stream_id='schedule-default-control',
                    stream_revision='1',paths=[dict(path_key=path['path_key'],
                        pairs=[dict(waiting_uniform=math.exp(-100.*2./365.),destination_uniform=.5)])
                        for path in credit['paths']])
            for history in current:
                interval=history['source']['credit']['intervals'][0]
                interval['hazards_per_year'][0][3]=100.
                interval['loss_given_default']=[.4]*3
                for tail in interval['complete_recovery_tails']:tail[0]['lag_years']=40./365.
    result,_=run(request)
    receipt=verify(request,result)
    assert receipt['purchased_journal_replayed']and receipt['complete_tail_replayed']
    source=result['preparation']['native_preparation']['source']
    physical=[s for graph in source['native_graphs']for s in graph['native_sources']if s['role']=='physical']
    assert physical
    for history in physical:
        paths=history['projection']['carry']['credit']['result']['paths']
        assert all(p['default']is not None and p['default']['economic_principal_loss']>0. for p in paths)
        assert any(c['kind']=='recovery'for c in history['cash'])
    lineage=result['tables']['commercial_source_lineage']
    assert lineage and any(row['native_atomic_group']['next']['in_default']for row in lineage)
    assert any(row['native_atomic_group']['year']>=40./365. for row in lineage)


@pytest.mark.parametrize('basis,floor,status', [
    ('unconfigured', 0., 'unconfigured'),
    ('not_applicable', 0., 'not_applicable'),
])
def test_commercial_journal_verifies_explicit_liquidity_policy(basis, floor, status):
    request = raw()
    for account in request['specification']['accounts']:
        for metric in ('lcr', 'nsfr'):
            account[metric + '_applicability'] = basis
            account[metric + '_floor'] = floor
    result, _ = run(request)
    assert verify(request, result)['purchased_journal_replayed']
    for row in result['tables']['path']:
        assert row['lcr_status'] == row['nsfr_status'] == status
        if basis != 'internal_limit':
            assert row['lcr_headroom'] is None and row['nsfr_headroom'] is None
    altered = deepcopy(result)
    altered['tables']['path'][0]['lcr_status'] = 'within_limit'
    with pytest.raises(ArithmeticError):
        verify(request, altered)
    altered = deepcopy(result)
    altered['preparation']['resolved_specification']['accounts'][0]['lcr_applicability'] = 'regulatory'
    with pytest.raises(ArithmeticError):
        verify(request, altered)


def test_commercial_journal_keeps_configured_liquidity_outside_adopted_scope():
    request = raw()
    request['specification']['accounts'][0].update(lcr_applicability='internal_limit', lcr_floor=1.)
    with pytest.raises(ValueError, match='eligibility is not adopted'):
        run(request)
    from portfolio_risk.analytics.commercial_reports_reference import normalize_specification, scope
    from types import SimpleNamespace
    normalized = normalize_specification(request['specification'])
    spec = {key: [SimpleNamespace(**row) for row in values] if key in ('accounts', 'positions', 'scenarios') else values
            for key, values in normalized.items()}
    with pytest.raises(ArithmeticError, match='capital/liquidity'):
        scope(spec, [owner['position_id'] for owner in request['owners']])


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
