"""Typed native row transport. No pricing, policy or financial authority."""
import polars as pl
from .saved_derivative_dated_reference import DATED_SCHEMAS
from .balance_stream import PartitionWriter
SCHEMA='saved-derivative-mixed-1'
_COMMON={k:pl.String for k in ('scenario','account','book_key','gl_account','instrument_id')}
_LINEAGE=dict(_COMMON,source_event_id=pl.String,source_intent_index=pl.Int64,source_posting_index=pl.Int64,ordinal=pl.Int64,day=pl.Int64,debit_amount=pl.Float64)
SCHEMAS=dict(DATED_SCHEMAS,derivative_journal_lineage=_LINEAGE,derivative_custody_journal=_LINEAGE,
    derivative_opening_lineage=dict(_COMMON,ordinal=pl.Int64,debit_amount=pl.Float64,posted_in_shared_opening=pl.Boolean,custody_memo=pl.Boolean))
class DerivativeWriter(PartitionWriter):
    def __init__(self,*a,**kw):super().__init__(*a,schemas=SCHEMAS,**kw)
