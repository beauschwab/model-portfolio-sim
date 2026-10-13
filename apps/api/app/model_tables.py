"""Lossless source tables and inverse persisted verification input."""
import json
import polars as pl

def canonical(value):return json.dumps(value,allow_nan=False,sort_keys=True,separators=(',',':'))

def scalar_type(values):
    """Select only lossless primitive columns; heterogeneous JSON stays JSON."""
    present=[value for value in values if value is not None]
    types={type(value)for value in present}
    if not types:return pl.Null
    if len(types)!=1:return None
    kind=next(iter(types))
    if kind is int:
        low,high=min(present),max(present)
        if -(2**63)<=low and high<2**63:return pl.Int64
        if low>=0 and high<2**64:return pl.UInt64
        return None
    return {float:pl.Float64,bool:pl.Boolean,str:pl.String}.get(kind)

def format_sources(result):
    layouts={}
    def visit(value,path):
        if isinstance(value,dict):return {k:visit(v,path+'/'+k) for k,v in value.items()}
        if isinstance(value,list) and value and all(isinstance(row,dict) for row in value):
            keys=set(value[0])
            if any(set(row)!=keys for row in value):
                layouts[path]=dict(row_json=True,json_columns=[])
                return pl.DataFrame({'source_json':[canonical(row) for row in value]})
            nested=[key for key in value[0] if any(isinstance(row[key],(dict,list)) for row in value)]
            rows=[{k:canonical(v) if k in nested else v for k,v in row.items()} for row in value]
            unsigned={key:pl.UInt64 for key in keys if any(isinstance(row[key],int) and not isinstance(row[key],bool) and row[key]>2**63-1 for row in value)}
            layouts[path]=dict(row_json=False,json_columns=nested)
            return pl.DataFrame(rows,infer_schema_length=None,schema_overrides=unsigned)
        if isinstance(value,list) and value and all(not isinstance(row,(dict,list,tuple)) for row in value):
            dtype=scalar_type(value)
            if dtype is None:return value
            layouts[path]=dict(kind='vector')
            return pl.DataFrame({'value':value},schema_overrides={'value':dtype},infer_schema_length=None)
        if isinstance(value,list) and value and all(isinstance(row,(list,tuple)) for row in value):
            width=len(value[0])
            if width and all(len(row)==width and all(not isinstance(cell,(dict,list,tuple)) for cell in row) for row in value):
                columns={f'value_{i}':[row[i]for row in value]for i in range(width)}
                dtypes={column:scalar_type(cells)for column,cells in columns.items()}
                if any(dtype is None for dtype in dtypes.values()):return value
                layouts[path]=dict(kind='matrix',width=width)
                return pl.DataFrame(columns,schema_overrides=dtypes,infer_schema_length=None)
        return value
    return visit(result,''),layouts

def restore_scalar_table(value,layout):
    """Restore an explicitly tagged flat vector/matrix without inferring shape."""
    if type(layout)is not dict:raise ValueError('persisted scalar table layout domain')
    kind=layout.get('kind')
    if kind=='vector':
        if set(layout)!={'kind'} or value.columns!=['value']:
            raise ValueError('persisted vector layout/columns differ')
        rows=value.to_dicts()
        if any(isinstance(row['value'],(dict,list,tuple))for row in rows):
            raise ValueError('persisted vector contains nested cells')
        return [row['value']for row in rows]
    if kind=='matrix':
        width=layout.get('width')
        if (set(layout)!={'kind','width'} or type(width)is not int
                or width<1 or width!=value.width):
            raise ValueError('persisted matrix width/layout differs')
        columns=[f'value_{i}'for i in range(width)]
        if value.columns!=columns:raise ValueError('persisted matrix columns differ')
        rows=value.to_dicts()
        if any(isinstance(cell,(dict,list,tuple))for row in rows for cell in row.values()):
            raise ValueError('persisted matrix contains nested cells')
        return [[row[column]for column in columns]for row in rows]
    raise ValueError('unsupported persisted scalar table kind')

def restore_sources(value,layouts,path=''):
    if isinstance(value,pl.DataFrame):
        if path not in layouts:raise ValueError('unconfigured persisted model table')
        layout=layouts[path]
        if 'kind'in layout:return restore_scalar_table(value,layout)
        rows=value.to_dicts()
        if layout['row_json']:
            if value.columns!=['source_json']:raise ValueError('persisted row-source columns differ')
            return [json.loads(row['source_json']) for row in rows]
        return [{k:json.loads(v) if k in layout['json_columns'] else v for k,v in row.items()} for row in rows]
    if isinstance(value,dict):return {k:restore_sources(v,layouts,path+'/'+k) for k,v in value.items()}
    return value
