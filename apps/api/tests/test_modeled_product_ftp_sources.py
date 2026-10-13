"""Default completed native sources -> selected FTP -> immutable guarded publication.

Authored for the applied successor/official gate; never executed by the artifact
suite. No alternate executable or artifact imports are used here.
"""
from copy import deepcopy
from pathlib import Path
import json
import pytest
from app import main,model_routes,treasury_routes,worker,persistence as db
from app.schemas import ModeledBalanceStressRequest
from test_persistence import durable

FIXTURES=Path(__file__).resolve().parents[3]/'packages/portfolio-risk/tests/fixtures/modeled_product_ftp'

def execute(jid,owner):
    db.WORKER=owner;worker.STOP.clear();assert db.REPO.acquire(owner,300.)
    job=db.REPO.claim(owner);assert job['id']==jid
    worker.execute(job);row=db.REPO.job(jid)
    assert row['status']=='done',row['detail']
    db.REPO.release(owner);return row

def completed(kind,fixture):
    template=json.loads((FIXTURES/(fixture+'.json')).read_text());source=template['source']['sources'][0];raw=source['input']['request']
    if kind=='card_v2':
        from portfolio_risk.core import product_credit_mixed_native
        assert product_credit_mixed_native.binary_path().parent.name=='release'
        status=model_routes.submit(model_routes.ModelRunRequest(expected_revision=0,schema_name=raw['schema'],request=raw,timeout=300.));jid=status.id
    else:
        from portfolio_risk.analytics import model_ledger
        assert model_ledger.binary_path().parent.name=='release'
        jid=main.modeled_balance_stress(ModeledBalanceStressRequest(expected_revision=0,specification=raw))['id']
    execute(jid,'source-ftp-owner');template['source'].pop('sources')
    selection=dict(schema='modeled-product-ftp-completed-selection-1',sources=[dict(source_id=source['source_id'],source_job=jid,kind=kind,scope=source['scope'],scope_review=source['scope_review'])],specification=template)
    return jid,selection

@pytest.mark.parametrize('kind,fixture',[('card_v2','card-reviewed-gross-funding'),('deposit_performing','deposit-continuation'),('deposit_portfolio','deposit-portfolio-two40'),('deposit_portfolio','deposit-portfolio-one90')])
def test_default_completed_source_selection_persisted_whole_gl(durable,kind,fixture):
    source,selection=completed(kind,fixture)
    request=treasury_routes.ModeledProductFTPRequest(expected_revision=0,specification=selection,timeout=300.)
    queued=treasury_routes.modeled_product_ftp_sources(request,dict(expected_revision=0,specification=selection,timeout=300.))
    row=execute(queued.id,'selected-ftp-owner');saved=db.CODEC.load(row['result']);e=saved['execution']
    bound,=e['completed_source_bindings'];assert bound['provenance']['binding']['id']==source
    assert saved['tables']['source_journal'].height>0
    assert saved['summary']['capital_ftp_eligible']is False
    assert e['validation']['raw_source_and_ordered_journal_verified']is True

@pytest.mark.parametrize('kind,fixture',[('card_v2','card-reviewed-gross-funding'),('deposit_performing','deposit-continuation'),('deposit_portfolio','deposit-portfolio-two40'),('deposit_portfolio','deposit-portfolio-one90')])
def test_default_bound_source_output_replacement_during_dump_never_done(durable,monkeypatch,kind,fixture):
    source,selection=completed(kind,fixture)
    request=treasury_routes.ModeledProductFTPRequest(expected_revision=0,specification=selection,timeout=300.)
    queued=treasury_routes.modeled_product_ftp_sources(request,dict(expected_revision=0,specification=selection,timeout=300.))
    ref=db.REPO.job(source)['result'];put=db.CODEC.objects.put
    def upload(*a,**kw):
        out=put(*a,**kw);path=db.CODEC.objects.root/ref['key'];path.write_bytes(path.read_bytes()+b'late-source-corruption');return out
    monkeypatch.setattr(db.CODEC.objects,'put',upload)
    db.WORKER='selected-ftp-owner';worker.STOP.clear();assert db.REPO.acquire(db.WORKER,300.);job=db.REPO.claim(db.WORKER)
    worker.execute(job);row=db.REPO.job(queued.id)
    assert row['status']!='done' and row['result']is None
