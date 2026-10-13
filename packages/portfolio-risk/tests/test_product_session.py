"""Native staged product economics against independent raw-source replay."""
from copy import deepcopy
import json
from pathlib import Path
import pytest

from portfolio_risk.analytics.product_strategy_reference import verify
from portfolio_risk.core.runtime import RunConfig,run_context
from portfolio_risk.strategy import product,optimizer

ROOT=Path(__file__).resolve().parents[3]

def raw():
    return json.loads((ROOT/'examples/models/strategy-product-1.json').read_text())

def session(r):
    return product.ProductSession(r['product'],r['bases'],r['constraints'],face_limits=r['face_limits'])

def test_constraint_candidate_keeps_original_sources_until_verified_publication():
    r=raw()
    with session(r) as s:
        initial=s.status()['snapshot']
        revised=deepcopy(r);revised['constraints']['max_total_assets']=50.
        staged=s.stage(constraints=revised['constraints'])
        assert staged['work']==dict(full_rebuild=False,units_rebuilt=0,selective_rebuild=False,coefficient_only=True)
        assert staged['product_preparation']==initial['preparation']
        assert verify(revised,staged)['verified']
        assert s.status()['snapshot']==initial and s.version==0
        assert [a['face'] for a in staged['allocation']]==pytest.approx([20.,30.])
        with pytest.raises(ValueError,match='token'):s.publish(staged['token']+1)
        with pytest.raises(ValueError,match='pending'):s.stage()
        s.abort(staged['token']);assert not s.status()['pending']
        staged=s.stage(constraints=revised['constraints']);s.publish(staged['token'])
        assert s.version==1
        answer=s.solve()
        assert verify(revised,dict(answer,product_preparation=s.status()['snapshot']['preparation']))['verified']
        assert [a['face'] for a in answer['allocation']]==pytest.approx([20.,30.])

def test_dirty_unit_reuses_unchanged_native_sources_and_abort_preserves_yield_anchors():
    r=raw();revised=deepcopy(r);revised['product']['units'][0]['weights']['rwa']=.25
    with session(r) as s:
        initial=s.status()['snapshot']
        candidate=s.stage(product=revised['product'])
        assert candidate['work']==dict(full_rebuild=False,units_rebuilt=1,selective_rebuild=True,coefficient_only=False)
        assert verify(revised,candidate)['verified']
        assert candidate['product_preparation']['preparation']['units'][1]==initial['preparation']['preparation']['units'][1]
        complete=product.optimize(revised['product'],revised['bases'],revised['constraints'],face_limits=revised['face_limits'])
        assert candidate['product_preparation']==complete['product_preparation']
        assert candidate['worst_case_nii_$']==complete['worst_case_nii_$']
        s.abort(candidate['token']);assert s.status()['snapshot']==initial
        bad=deepcopy(revised['product']);bad['units'][0]['quote_price_per_face_unit']*=1.01
        with pytest.raises(ValueError,match='reconcile'):s.stage(product=bad)
        assert not s.status()['pending'] and s.status()['snapshot']==initial

def test_retained_allocation_evaluation_never_solves_or_reprices(monkeypatch):
    r=raw()
    with session(r) as s:
        answer=s.solve();allocation=[dict(unit_id=a['unit']['unit_id'],face=a['face']) for a in answer['allocation']]
        published=s.status()['snapshot']['preparation']
        original=s.native.call
        def only_eval(**fields):
            assert fields['op']=='eval'
            return original(**fields)
        with monkeypatch.context() as patch:
            patch.setattr(s.native,'call',only_eval)
            selected=s.evaluate(allocation)
            assert selected['work']['coefficient_only'] and selected['work']['units_rebuilt']==0
            assert selected['worst_case_nii_$']==pytest.approx(answer['worst_case_nii_$'],abs=1e-10)
            assert verify(r,dict(selected,product_preparation=published))['verified']
            with pytest.raises(ValueError,match='identity|ordered|unit'):
                s.evaluate(list(reversed(allocation)))

def test_public_optimizer_dispatches_exact_product_sources_and_reviewed_face_limits():
    r=raw()
    with run_context(RunConfig(compute_backend='rust')):
        lib=product.build_library(r['product'])
        answer=optimizer.optimize_balance_sheet([(lib,r['bases'][0]['base'])],
            **{k:v for k,v in r['constraints'].items() if k not in {'capital_limits','commercial'}},face_limits=r['face_limits'])
        assert answer['execution']['schema']=='strategy-product-1'
        assert [a['face'] for a in answer['allocation']]==pytest.approx([20.,80.])
        assert verify(r,answer)['verified']
        with pytest.raises(ValueError,match='cannot be mixed'):
            optimizer.optimize_balance_sheet([(lib,r['bases'][0]['base']),({},r['bases'][0]['base'])])
        with pytest.raises(ValueError,match='typed face_limits'):
            optimizer.optimize_balance_sheet([(lib,r['bases'][0]['base'])],commercial=[dict(template='ALL_ASSET',rhs=1,sense='<=')])
