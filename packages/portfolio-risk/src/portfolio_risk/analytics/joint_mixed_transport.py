"""Bounded joint owner table transport, without monetary calculation."""
import io,hashlib,json
from collections import defaultdict
import polars as pl
from .deposit_portfolio_transport import SCHEMAS as DEPOSIT_SCHEMAS,JSON_CELLS
from .saved_derivative_transport import SCHEMAS as DERIVATIVE_SCHEMAS
from .deposit_continuation_transport import ContinuationDepositWriter
SCHEMA='saved-derivative-deposit-portfolio-mixed-1'
SCHEMAS=dict(DEPOSIT_SCHEMAS,**{k:v for k,v in DERIVATIVE_SCHEMAS.items() if k.startswith('derivative_')})

class JointWriter(ContinuationDepositWriter):
    def __init__(self,*a,**kw):
        # Use the existing checksummed/bounded Parquet frame reader with the
        # composed schema. No source-provided financial rows are transformed.
        super().__init__(*a,**kw)
        self.schemas={name:dict(schema)for name,schema in SCHEMAS.items()}
        self.parts=defaultdict(list)
        self.buffers=defaultdict(list)

    def frames(self,name):
        from .joint_partition_bounds import parquet_metadata,LIMIT
        for part in self.parts[name]:
            self.check()
            if type(part['rows'])is not int or not 1<=part['rows']<=self.partition_rows or type(part['bytes'])is not int or not 0<part['bytes']<=32*1024*1024:raise ValueError('joint partition declared bound')
            path=self.root/part['path']
            if path.stat().st_size!=part['bytes']:raise ValueError('joint partition size')
            with path.open('rb')as stream:data=stream.read(part['bytes']+1)
            if len(data)!=part['bytes']or hashlib.sha256(data).hexdigest()!=part['sha256']:raise ValueError('joint partition checksum')
            self.check()
            self.decoded,self.logical=parquet_metadata(data,part['rows'],{k:str(v)for k,v in self.schemas[name].items()},decoded=self.decoded,logical=self.logical,check=self.check)
            frame=pl.read_parquet(io.BytesIO(data),n_rows=part['rows']+1)
            if frame.height!=part['rows']or frame.schema!=self.schemas[name]:raise ValueError('joint partition row/schema')
            self.check();yield frame
