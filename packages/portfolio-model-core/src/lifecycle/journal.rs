//! Journal kinds and shapes shared by the lifecycle acquisition engine and the ledger owner that
//! adopts its intents. One financial owner per economic event; custody is memo only.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Cash,
    OpeningEquity,
    /// Security face held at amortized cost.
    AssetPrincipal,
    /// Purchase cost less face, so carrying equals cost: no par gain is manufactured.
    BookAdjustment,
    /// Accrued interest bought with the security.
    InterestReceivable,
    FundingPrincipal,
    DerivativeAsset,
    DerivativeLiability,
    PostedVm,
    ReceivedVm,
    PostedIm,
    /// Termination consideration due but unsettled between the effective and settlement dates.
    UnpaidReceivable,
    UnpaidPayable,
    FeeExpense,
    /// Consideration less removed carrying on a sale or termination; marks already in earnings
    /// are never counted again.
    RealizedGainLoss,
    /// Accrued funding interest and break costs paid on repayment.
    InterestExpense,
    CustodyAsset,
    CustodyObligation,
}
impl Kind {
    pub fn gl(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::OpeningEquity => "opening_equity",
            Self::AssetPrincipal => "asset_principal",
            Self::BookAdjustment => "book_adjustment",
            Self::InterestReceivable => "dated_interest_receivable",
            Self::FundingPrincipal => "funding_principal",
            Self::DerivativeAsset => "derivative_asset",
            Self::DerivativeLiability => "derivative_liability",
            Self::PostedVm => "derivative_posted_vm_asset",
            Self::ReceivedVm => "derivative_received_vm_liability",
            Self::PostedIm => "derivative_posted_im_asset",
            Self::UnpaidReceivable => "derivative_unpaid_receivable",
            Self::UnpaidPayable => "derivative_unpaid_payable",
            Self::FeeExpense => "pnl:lifecycle_fees",
            Self::RealizedGainLoss => "pnl:lifecycle_realized",
            Self::InterestExpense => "pnl:lifecycle_interest",
            Self::CustodyAsset => "memo:derivative_custodian_asset",
            Self::CustodyObligation => "memo:derivative_custodian_obligation",
        }
    }
    pub fn memo(self) -> bool {
        matches!(self, Self::CustodyAsset | Self::CustodyObligation)
    }
    pub fn pnl(self) -> bool {
        matches!(
            self,
            Self::FeeExpense | Self::RealizedGainLoss | Self::InterestExpense
        )
    }
    pub fn stock(self) -> bool {
        !self.memo() && !self.pnl() && !matches!(self, Self::Cash | Self::OpeningEquity)
    }
    /// Signed adjustment whose gross direction is not fixed by the account.
    pub fn signed(self) -> bool {
        self == Self::BookAdjustment
    }
    pub fn liability(self) -> bool {
        matches!(
            self,
            Self::FundingPrincipal
                | Self::DerivativeLiability
                | Self::ReceivedVm
                | Self::UnpaidPayable
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    pub kind: Kind,
    pub instrument: String,
    pub debit: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Intent {
    pub event_id: String,
    pub ordinal: i32,
    pub phase: u32,
    pub sequence: u32,
    pub memo: bool,
    pub lines: Vec<Line>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inventory {
    pub lot_id: String,
    pub owned: f64,
    pub pledged: f64,
    pub outside_encumbrance: f64,
    pub available: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Closing {
    pub ordinal: i32,
    pub financial: Vec<(Kind, f64)>,
    pub custody: Vec<(Kind, f64)>,
    pub cash: f64,
    /// Cash less received VM the agreement does not allow the entity to reuse. Restrictions are
    /// represented here once; the GL carries the gross received stock.
    pub usable_cash: f64,
    pub inventory: Vec<Inventory>,
}
