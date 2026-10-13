"""Version2 table transport; source1 monetary/table contracts stay historical."""
from .deposit_continuation_transport import SCHEMAS,JSON_CELLS,ContinuationDepositWriter
SCHEMA='deposit-performing-portfolio-ledger-2'
WORKFLOW='deposit-performing-portfolio-workflow-2'
class PortfolioDepositWriter(ContinuationDepositWriter):
    pass
