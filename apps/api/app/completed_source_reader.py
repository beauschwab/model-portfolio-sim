"""Shared bounded completed-source transport; no financial calculation."""
from copy import deepcopy
import io,json,math
import polars as pl
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
LIMIT=32*1024*1024
MAX_OBJECTS=4096+4 # Existing completed-source partition plus manifest allowance.

def probe(check):
    if check():raise InterruptedError('completed source cancelled')

def exact(actual,expected,message):
    if not exact_json_identity(actual,expected):raise ValueError(message)

def encoded(value):return json.dumps(value,allow_nan=False,sort_keys=True,separators=(',',':')).encode()


def loads(data):
    def pairs(items):
        out={}
        for k,v in items:
            if k in out:raise ValueError('duplicate source JSON field')
            out[k]=v
        return out
    def number(s):
        v=float(s)
        if not math.isfinite(v):raise ValueError('nonfinite source JSON')
        return v
    def constant(s):raise ValueError('nonfinite source JSON')
    return json.loads(data,object_pairs_hook=pairs,parse_float=number,parse_constant=constant)

def descriptor(ref,form,limit=LIMIT):
    if type(ref)is not dict or set(ref)!={'key','sha256','bytes','format'} or ref.get('format')!=form or type(ref.get('bytes'))is not int or not 0<ref['bytes']<=limit or type(ref.get('sha256'))is not str or len(ref['sha256'])!=64 or any(c not in '0123456789abcdef'for c in ref['sha256']):raise ValueError('bounded immutable source descriptor required')

class Reader:
    """Strict bounded Codec reader with cumulative compressed and logical admission."""
    def __init__(self,codec,check):self.codec,self.check=codec,check;self.refs=[];self.bytes=0;self.decoded=0;self.logical=0;self._identities=set()
    def body(self,ref,form='bin',limit=LIMIT):
        descriptor(ref,form,limit);probe(self.check)
        identity=encoded(ref)
        if identity not in self._identities and len(self._identities)>=MAX_OBJECTS:raise ValueError('completed source object count admission')
        self._identities.add(identity)
        data=self.codec.objects.get_bounded(ref,limit);probe(self.check)
        self.refs.append(deepcopy(ref));self.bytes+=len(data)
        if self.bytes>2*LIMIT:raise ValueError('completed source cumulative read admission')
        return data
    def manifest(self,ref):
        m=loads(self.body(ref,'json'))
        if type(m)is not dict or set(m)!={'schema_version','value'}or type(m['schema_version'])is not int or m['schema_version']!=1:raise ValueError('source Codec manifest version')
        return m['value']
    def select(self,node,names):
        if type(node)is not dict or set(node)!={'type','items'}or node['type']!='dict':raise ValueError('source top-level Codec dict required')
        seen=set();out={}
        for pair in node['items']:
            if type(pair)is not list or len(pair)!=2 or type(pair[0])is not str or pair[0]in seen:raise ValueError('duplicate or nonstring source manifest key')
            seen.add(pair[0])
            if pair[0]in names:out[pair[0]]=self.decode(pair[1])
        return out
    def decode(self,node):
        probe(self.check)
        if type(node)is not dict:return node
        kind=node.get('type')
        if kind=='dict':return self.select(node,{pair[0]for pair in node['items'] if type(pair)is list and len(pair)==2})
        if kind in ('tuple','list')and set(node)=={'type','items'}:
            out=[self.decode(v)for v in node['items']];return tuple(out)if kind=='tuple'else out
        if kind=='object-reference'and set(node)=={'type','ref'}:
            ref=node['ref']
            if type(ref)is not dict or ref.get('format')not in {'bin','json','parquet'}:raise ValueError('bounded source object-reference format')
            descriptor(ref,ref['format'])
            if type(ref['key'])is not str or not 1<=len(ref['key'])<=1024:raise ValueError('bounded source object-reference key')
            return deepcopy(ref)
        if kind=='partitioned'and set(node)=={'type','value'}:return deepcopy(node['value'])
        if kind=='frame'and set(node)=={'type','ref','objects'}and node['objects']=={}:
            data=self.body(node['ref'],'parquet')
            import pyarrow.parquet as pq
            footer=pq.read_metadata(io.BytesIO(data))
            expanded=sum(footer.row_group(i).column(j).total_uncompressed_size for i in range(footer.num_row_groups)for j in range(footer.num_columns))
            self.decoded+=expanded
            if footer.num_rows>65536 or self.decoded>2*LIMIT:raise ValueError('bounded completed source decoded tables')
            # Dictionary pages do not bound materialized UTF8 cells. One-row
            # Arrow admission precedes Polars; this is no aggregate RSS claim.
            for batch in pq.ParquetFile(io.BytesIO(data)).iter_batches(batch_size=1):
                probe(self.check);self.logical+=batch.nbytes
                if self.logical>2*LIMIT:raise ValueError('bounded completed source logical tables')
                probe(self.check)
            probe(self.check)
            frame=pl.read_parquet(io.BytesIO(data),n_rows=65537)
            probe(self.check)
            if frame.height!=footer.num_rows:raise ValueError('completed source decoded row mismatch')
            return frame
        raise ValueError('unsupported completed source Codec node')


def restore_tables(value,layouts,path=''):
    if isinstance(value,pl.DataFrame):
        layout=layouts[path]
        if type(layout)is dict and 'kind'in layout:
            from .model_tables import restore_scalar_table
            return restore_scalar_table(value,layout)
        if type(layout.get('row_json'))is not bool or type(layout.get('json_columns'))is not list:raise ValueError('source table layout domain')
        rows=value.to_dicts()
        if layout['row_json']:
            if value.columns!=['source_json']:raise ValueError('source row JSON columns')
            return [loads(row['source_json'])for row in rows]
        return [{k:loads(v)if k in layout['json_columns']else v for k,v in row.items()}for row in rows]
    if type(value)is dict:return {k:restore_tables(v,layouts,path+'/'+k)for k,v in value.items()}
    return value

