"""Transport-only protocol-six source lineage and gross commercial books."""
import json
import math
import polars as pl
from .balance_stream import PartitionWriter
from .dated_ledger import DATED_SCHEMAS

SCHEMA='commercial-credit-ledger-1'
WORKFLOW='commercial-ledger-preparation-1'
BOOK={**dict.fromkeys('scenario position_id source_class unweighted_book_json'.split(),pl.String),
    'day':pl.Int64,'source_origin_day':pl.Int64,'path_key':pl.UInt64,**dict.fromkeys('source_weight allocation_factor'.split(),pl.Float64),'capital_ftp_eligible':pl.Boolean}
SCHEMAS=dict(DATED_SCHEMAS,
    commercial_source_lineage={**dict.fromkeys('scenario date_policy position_id source_class native_atomic_group_json'.split(),pl.String),
        **dict.fromkeys('day source_origin_day native_group_index transaction_number'.split(),pl.Int64),'path_key':pl.UInt64,
        **dict.fromkeys('native_credit_year global_native_credit_year source_weight allocation_factor'.split(),pl.Float64)},
    commercial_daily=BOOK,commercial_closing=BOOK)


class CommercialWriter(PartitionWriter):
    def add(self,name,values):
        if name.startswith('commercial_'):
            converted=[];field='native_atomic_group' if name=='commercial_source_lineage' else 'unweighted_book'
            for raw in values:
                row=dict(raw)
                if type(row.get(field)) is not dict:raise ValueError('commercial source/book object required')
                row[field+'_json']=json.dumps(row.pop(field),allow_nan=False,separators=(',',':'))
                if set(row)!=set(SCHEMAS[name]):raise ValueError('commercial table shape differs')
                for key,dtype in SCHEMAS[name].items():
                    value=row[key]
                    if dtype in (pl.Int64,pl.UInt64):
                        if value is None and key=='transaction_number':continue
                        if type(value) is not int or not (0<=value<1<<64 if dtype==pl.UInt64 else -(1<<63)<=value<1<<63):raise ValueError('commercial integer identity required')
                    elif dtype==pl.Float64:
                        if type(value) not in (float,int) or not math.isfinite(value):raise ValueError('commercial finite amount required')
                    elif dtype==pl.Boolean and type(value) is not bool:raise ValueError('commercial eligibility boolean required')
                    elif dtype==pl.String and type(value) is not str:raise ValueError('commercial text identity required')
                converted.append(row)
            values=converted
        return super().add(name,values)
