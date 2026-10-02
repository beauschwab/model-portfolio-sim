"""Native grouping, immutable lineage, explicit pricing and attribution contracts."""
from copy import deepcopy
import json
import polars as pl
import pytest
from portfolio_risk.analytics.cohorts import build, presets, example, read_tape, position_books, allocate


@pytest.fixture
def tape():
    return read_tape(example().encode(), 'csv')


def test_product_separation_conservation_and_stable_identity(tape):
    a=build(tape,presets())
    b=build(tape | {'frame':tape['frame'].reverse()},presets())
    assert a['cohorts'].equals(b['cohorts'])
    assert a['lineage'].equals(b['lineage'])
    assert a['lineage']['loan_id'].n_unique()==20
    assert a['summary']['balance']==sum(100000+i*1000 for i in range(4))*5
    for cohort in a['cohorts'].to_dicts():
        members=a['lineage'].filter(pl.col('cohort_id')==cohort['cohort_id'])
        assert members['balance'].sum()==pytest.approx(cohort['balance'])
        assert members['weight'].sum()==pytest.approx(1.)
        assert members['product'].n_unique()==1
    assert a['pricing_support']['mortgage']['supported']
    assert a['pricing_support']['deposit']['supported']
    assert not a['pricing_support']['credit_card']['supported']


def test_rule_comparison_and_dispersion_are_auditable(tape):
    config=presets();old=build(tape,config)
    revised=deepcopy(config)
    revised['rules']['mortgage']['dimensions'][0]['edges']=[746,747]
    new=build(tape,revised,old)
    assert new['build_id']!=old['build_id']
    assert new['comparison']['cohorts_after']>new['comparison']['cohorts_before']
    assert new['comparison']['balance_difference']==0
    assert new['comparison']['changed_members']==4
    assert new['dispersion'].filter(pl.col('field')=='fico')['stddev'].max()>0
    with pytest.raises(ValueError,match='same immutable tape'):
        build(tape | {'sha256':'different'},revised,old)


@pytest.mark.parametrize('edit', ['duplicate','missing','negative','nan','bins'])
def test_invalid_tapes_or_rules_fail_before_publication(tape,edit):
    config=presets()
    if edit=='duplicate':tape['frame']=pl.concat([tape['frame'],tape['frame'].head(1)])
    if edit=='missing':tape['frame']=tape['frame'].drop('entity')
    if edit=='negative':tape['frame']=tape['frame'].with_columns(pl.lit('-1').alias('balance'))
    if edit=='nan':tape['frame']=tape['frame'].with_columns(pl.lit('nan').alias('balance'))
    if edit=='bins':config['rules']['mortgage']['dimensions'][0]['edges']=[750,700]
    with pytest.raises(ValueError):build(tape,config)


def test_mapping_preserves_ids_and_explicit_defaults():
    tape=read_tape(b'Account,Amount,segment,age_months,rate_paid,price,svc_cost\n000001,1000,SAV,12,.02,100,.001\n','csv')
    config=presets();config['column_map']={'loan_id':'Account','balance':'Amount'}
    config['defaults']={'product':'deposit','currency':'USD','entity':'bank','accounting_category':'amortized_cost','assumption_set':'base','insured_status':'insured','relationship':'primary'}
    out=build(tape,config)
    assert out['lineage']['loan_id'].to_list()==['000001']
    assert out['summary']['balance']==1000


def test_original_loan_terms_and_non_additive_metrics(tape):
    out=build(tape,presets())
    cohort=position_books(out,products=['mortgage'])['mbs']
    loan=position_books(out,products=['mortgage'],loan_ids=['mortgage-000'])['mbs']
    assert loan['current_face'][0]==100000
    assert loan['fico'][0]==745
    assert cohort['current_face'].sum()==406000
    assert loan['cusip'][0]=='HL-mortgage-000'
    with pytest.raises(ValueError,match='dedicated behavioral pricer'):
        position_books(out,products=['credit_card'])
    metrics=out['cohorts'].select('cohort_id').with_columns(pl.lit(100.).alias('market_value'),pl.lit(-2.).alias('dv01'))
    allocated=allocate(out,metrics,metrics=['market_value','dv01'])
    totals=allocated.group_by('cohort_id').agg(pl.col('market_value').sum(),pl.col('dv01').sum())
    assert totals['market_value'].to_list()==pytest.approx([100.]*len(metrics))
    assert totals['dv01'].to_list()==pytest.approx([-2.]*len(metrics))
    assert set(allocated['calculation_basis'])=={'allocated_cohort_result'}
    with pytest.raises(ValueError,match='additive'):
        allocate(out,metrics,metrics=['oas'])
    with pytest.raises(ValueError,match='duplicate'):
        allocate(out,pl.concat([metrics,metrics]),metrics=['market_value'])


def test_missing_dimension_is_explicit_and_boundaries_left_closed(tape):
    config=presets()
    config['rules']['mortgage']['dimensions'][0]['edges']=[745,746,747,748]
    out=build(tape,config)
    group=[json.loads(r['grouping']) for r in out['cohorts'].to_dicts() if r['product']=='mortgage']
    assert {r['fico']['bucket'] for r in group}=={1,2,3,4}
    tape['frame']=tape['frame'].drop('relationship')
    with pytest.raises(ValueError,match='relationship'):build(tape,config)
    config['rules']['deposit']['dimensions'][3]['separate_missing']=True
    assert build(tape,config)['summary']['loans']==20


def test_refresh_reconciles_additions_removals_and_contract_changes(tape):
    from portfolio_risk.analytics.cohorts import reconcile_refresh
    old=build(tape,presets())
    rows=tape['frame'].to_dicts()
    rows=rows[1:]+[rows[0] | {'loan_id':'new-loan','balance':'200000'}]
    rows[0]['balance']='99000'
    rows[1]['entity']='other-bank'
    new=build(tape | {'frame':pl.DataFrame(rows),'sha256':'new-vintage'},presets())
    report=reconcile_refresh(old,new)
    assert report['counts']=={'added':1,'removed':1,'changed':2,'unchanged':17}
    assert report['balance_before']+report['balance_change']==report['balance_after']
    assert report['balance_change']==98000
    assert report['rows'].filter(pl.col('classification_changed')).height==1
