"""Transport-only revised-deposit table schema; verification is mandatory."""
import json
import polars as pl
from .balance_stream import PartitionWriter
from .dated_ledger import DATED_SCHEMAS
SCHEMA='deposit-revised-income-ledger-1'
SCHEMAS=dict(DATED_SCHEMAS,deposit_revision={
    **dict.fromkeys('scenario run_id position_id review_json'.split(),pl.String),
    'ordinal':pl.Int64,
    **dict.fromkeys('original_basis revised_basis basis_revision cash'.split(),pl.Float64),
    **dict.fromkeys('original_yield_retained accounting_standard_approval capital_ftp_eligible'.split(),pl.Boolean)})
class RevisedDepositWriter(PartitionWriter):
    def add(self,name,values):
        if name=='deposit_revision':
            rows=[]
            for raw in values:
                row=dict(raw)
                row['review_json']=json.dumps(row.pop('review'),allow_nan=False,separators=(',',':'))
                if set(row)!=set(SCHEMAS[name]):raise ValueError('revised deposit metadata table shape differs')
                rows.append(row)
            values=rows
        return super().add(name,values)

class VerificationFrames:
    """Borrow persisted partitions; stringify schema metadata, reread completion."""
    def __init__(self,frames,completion):
        self._frames=frames
        self._completion=completion
        self.schemas={name:{key:str(dtype) for key,dtype in schema.items()}
                      for name,schema in frames.schemas.items()}
    @property
    def completion(self):return self._completion()
    @property
    def root(self):return self._frames.root
    def frames(self,name):return self._frames.frames(name)
