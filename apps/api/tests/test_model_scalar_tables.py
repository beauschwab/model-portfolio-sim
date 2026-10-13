"""Shared scalar layouts must reopen identically in analyses and completed FTP."""
import polars as pl
import pytest
from app.model_tables import format_sources, restore_sources
from app.modeled_product_ftp_source_binding import _restore


@pytest.mark.parametrize('restore',[restore_sources,_restore])
def test_scalar_and_record_sources_roundtrip_without_losing_types(restore):
    raw=dict(vector=[1.,None,2.],matrix=[[1,2],[3,4]],labels=['a','b'],
             records=[dict(id='a',nested=dict(kind='supplied',amount=2.))])
    tables,layouts=format_sources(raw)
    assert restore(tables,layouts)==raw


@pytest.mark.parametrize('restore',[restore_sources,_restore])
@pytest.mark.parametrize('layout',[
    dict(kind='vector',width=1),dict(kind='vector',row_json=False),
    dict(kind='matrix',width=True),dict(kind='matrix',width=0),
    dict(kind='matrix',width=10**12),dict(kind='matrix',width=1,extra=0),
    dict(kind='unknown'),
])
def test_ambiguous_unknown_or_unbounded_scalar_layouts_fail(restore,layout):
    with pytest.raises(ValueError):
        restore(pl.DataFrame({'value':[1.]}),{'':layout})


@pytest.mark.parametrize('restore',[restore_sources,_restore])
@pytest.mark.parametrize('kind',['vector','matrix'])
def test_nested_scalar_cells_and_wrong_columns_cannot_be_reinterpreted(restore,kind):
    column='value'if kind=='vector'else'value_0'
    layout=dict(kind=kind)
    if kind=='matrix':layout['width']=1
    with pytest.raises(ValueError):
        restore(pl.DataFrame({column:[[1,2]]}),{'':layout})
    with pytest.raises(ValueError):
        restore(pl.DataFrame({'unexpected':[1.]}),{'':layout})


@pytest.mark.parametrize('restore',[restore_sources,_restore])
def test_heterogeneous_scalars_stay_lossless_json_and_column_types_are_retained(restore):
    from app.model_tables import canonical
    raw=dict(mixed=[1,2.0,True,'a'],mixed_matrix=[[1],[2.0]],
             typed_matrix=[[1,2.0],[3,4.0]],unsigned=[0,2**64-1],
             unsupported_range=[-1,2**64-1],nullable=[None,None])
    tables,layouts=format_sources(raw)
    assert set(layouts)=={'/typed_matrix','/unsigned','/nullable'}
    assert tables['unsigned'].schema['value']==pl.UInt64
    # Include the persisted Parquet boundary; UInt64 identities cannot round through f64.
    import io
    for name in ('typed_matrix','unsigned','nullable'):
        encoded=io.BytesIO();tables[name].write_parquet(encoded)
        tables[name]=pl.read_parquet(io.BytesIO(encoded.getvalue()))
    assert canonical(restore(tables,layouts))==canonical(raw)
