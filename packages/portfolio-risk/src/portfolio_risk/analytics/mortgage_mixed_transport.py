"""Transport-only strict mortgage tables; financial verification is mandatory."""
import json
import polars as pl
from .balance_stream import PartitionWriter
from .dated_ledger import DATED_SCHEMAS
SCHEMA='mortgage-mixed-accounting-1'
SCHEMAS=dict(DATED_SCHEMAS,
    mortgage_lineage={**dict.fromkeys('scenario run_id position_id path_id registry_source_id native_source_id source_action_indices_json review_json original_yield_anchor_json'.split(),pl.String),**dict.fromkeys('ordinal native_group_index transaction_number'.split(),pl.Int64),**dict.fromkeys('source_weight allocation_factor'.split(),pl.Float64),'source_groups_sole_postings':pl.Boolean},
    mortgage_daily={**dict.fromkeys('scenario run_id position_id path_id registry_source_id memo_checkpoint_source unweighted_book_json'.split(),pl.String),**dict.fromkeys('ordinal native_daily_index native_daily_ordinal'.split(),pl.Int64),**dict.fromkeys('source_weight allocation_factor'.split(),pl.Float64),**dict.fromkeys('acquired capital_ftp_eligible closing_capital_eligible'.split(),pl.Boolean)},
    mortgage_closing={**dict.fromkeys('scenario position_id path_id registry_source_id unweighted_book_json'.split(),pl.String),**dict.fromkeys('full_tail_complete capital_ftp_eligible closing_capital_eligible'.split(),pl.Boolean)})
JSON_COLUMNS={'mortgage_lineage':('source_action_indices','review','original_yield_anchor'),'mortgage_daily':('unweighted_book',),'mortgage_closing':('unweighted_book',)}
class MortgageWriter(PartitionWriter):
    def add(self,name,values):
        if name in JSON_COLUMNS:
            rows=[]
            for raw in values:
                row=dict(raw)
                for field in JSON_COLUMNS[name]:
                    if field not in row:raise ValueError('missing mortgage metadata cell')
                    value=row.pop(field)
                    row[field+'_json']=None if value is None else json.dumps(value,allow_nan=False,separators=(',',':'))
                if set(row)!=set(SCHEMAS[name]):raise ValueError('mortgage metadata table shape differs')
                rows.append(row)
            values=rows
        return super().add(name,values)
