"""Transport-only performing deposit source-owner table contracts."""
import io,hashlib,json
import polars as pl
from .balance_stream import PartitionWriter
from .dated_ledger import DATED_SCHEMAS
from .deposit_revision_transport import VerificationFrames
SCHEMA='deposit-performing-income-ledger-1'
def _shape(strings='',integers='',floats='',booleans=''):
 return {**dict.fromkeys(strings.split(),pl.String),**dict.fromkeys(integers.split(),pl.Int64),**dict.fromkeys(floats.split(),pl.Float64),**dict.fromkeys(booleans.split(),pl.Boolean)}
SCHEMAS=dict(DATED_SCHEMAS,
 deposit_principal_lineage=_shape('scenario run_id position_id source_atom transition review','ordinal day allocation_index transaction_number','cash income','source_owned_principal capital_ftp_eligible'),
 deposit_continuation_state=_shape('scenario run_id position_id gross_rights source_income original_yield_anchor','ordinal day source_atom_cursor','principal book_adjustment accrued_interest gross_carrying_value','accounting_standard_approval capital_ftp_eligible'),
 deposit_revision=_shape('scenario run_id position_id review','ordinal day','basis_revision revised_basis cash','original_yield_retained accounting_standard_approval capital_ftp_eligible'))
JSON_CELLS={'deposit_principal_lineage':('source_atom','transition','review'),'deposit_continuation_state':('gross_rights','source_income','original_yield_anchor'),'deposit_revision':('review',)}
class ContinuationDepositWriter(PartitionWriter):
 def __init__(self,*args,**kwargs):
  super().__init__(*args,**kwargs);self.check=lambda:False;self.decoded=0;self.logical=0;self.admitted=False
 def frames(self,table):
  from ._deposit_continuation_partition_bounds import LIMIT,parquet_metadata
  if not self.admitted:
   count=0;size=0
   for name in ('dated-input.json','prepared-source.json','producer-completion.json'):
    path=self.root/name
    if path.is_file():size+=path.stat().st_size
   for parts in self.parts.values():
    for part in parts:
     self.check();count+=1
     if type(part['rows']) is not int or not 1<=part['rows']<=self.partition_rows or type(part['bytes']) is not int or not 0<part['bytes']<=LIMIT:
      raise ValueError('deposit standalone partition declared bounds')
     size+=part['bytes']
     if size>LIMIT or count>65536:raise ValueError('deposit standalone aggregate byte/count cap')
   if size>LIMIT:raise ValueError('deposit standalone aggregate metadata cap')
   self.admitted=True
  for part in self.parts[table]:
   self.check();path=self.root/part['path']
   if path.stat().st_size!=part['bytes']:raise ValueError('deposit standalone partition size')
   with path.open('rb') as stream:data=stream.read(part['bytes']+1)
   self.check()
   if len(data)!=part['bytes'] or hashlib.sha256(data).hexdigest()!=part['sha256']:raise ValueError('deposit standalone partition checksum')
   self.decoded,self.logical=parquet_metadata(data,part['rows'],{k:str(v) for k,v in self.schemas[table].items()},decoded=self.decoded,logical=self.logical,check=self.check)
   frame=pl.read_parquet(io.BytesIO(data),n_rows=part['rows']+1)
   if frame.height!=part['rows'] or frame.schema!=self.schemas[table]:raise ValueError('deposit standalone partition rows/schema')
   self.check();yield frame
 def add(self,name,values):
  if name in JSON_CELLS:
   rows=[]
   for raw in values:
    row=dict(raw)
    for key in JSON_CELLS[name]:row[key]=json.dumps(row[key],allow_nan=False,separators=(',',':'))
    if set(row)!=set(SCHEMAS[name]):raise ValueError('performing deposit metadata table shape differs')
    rows.append(row)
   values=rows
  return super().add(name,values)
