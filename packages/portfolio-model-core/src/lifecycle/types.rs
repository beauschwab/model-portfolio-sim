//! `liquidity-lifecycle-1` records (stage A of the Rust liquidity lifecycle plan, issue #43).
//! Every record is strict: unknown fields and unknown variants are refused.
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "liquidity-lifecycle-1";
pub const MAX_RAW_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RECORDS: usize = 16_384;
pub const MAX_DEPENDENCIES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub id: String,
    pub revision: String,
}

/// An immutable instrument or relationship source; hashes establish identity, not economics.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    pub source_id: String,
    pub revision: String,
    pub sha256: String,
    pub measure: Measure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    Physical,
    Pricing,
}

/// Supplied opening, posted once per account. No balancing plug is inferred.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningAccount {
    pub account_id: String,
    pub cash: f64,
    pub equity: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LotKind {
    Security,
    Loan,
    Derivative,
    Funding,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountingCategory {
    AmortizedCost,
    AvailableForSale,
    HeldToMaturity,
    Trading,
    Derivative,
    FundingLiability,
}

/// One lot of one position. Original anchors are retained for the life of the lot, including
/// any surviving part after a partial unwind.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PositionLot {
    pub lot_id: String,
    pub position_id: String,
    pub source_id: String,
    pub account_id: String,
    pub kind: LotKind,
    pub accounting_category: AccountingCategory,
    /// Face or notional in currency units; always positive. Direction comes from the kind.
    pub quantity: f64,
    pub acquisition_ordinal: i32,
    pub original_cost: f64,
    pub original_effective_yield: f64,
    pub original_oas: f64,
    /// Encumbrance held outside this plan (for example by an unrelated agreement).
    #[serde(default)]
    pub outside_encumbrance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnwindTreatment {
    Terminate,
    Retain,
    Offset,
    Rebalance,
}
/// Economic hedge link. It never confers formal hedge accounting qualification.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HedgeAllocation {
    pub allocation_id: String,
    pub exposure_lot_id: String,
    pub hedge_lot_id: String,
    pub amount: f64,
    pub risk_basis: RiskBasis,
    pub effective_start_ordinal: i32,
    #[serde(default)]
    pub effective_end_ordinal: Option<i32>,
    pub review: Revision,
    pub unwind_treatment: UnwindTreatment,
    /// Formal ASC 815 relationship this economic allocation is documented under, if any.
    #[serde(default)]
    pub formal_relationship_id: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskBasis {
    InterestRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingInstruction {
    Repay,
    Retain,
    Replace,
}
/// Attribution of financing to a lot. Attribution alone does not pledge inventory.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FundingAllocation {
    pub allocation_id: String,
    pub funding_lot_id: String,
    pub attributed_lot_id: String,
    pub amount: f64,
    pub effective_start_ordinal: i32,
    #[serde(default)]
    pub effective_end_ordinal: Option<i32>,
    pub instruction: FundingInstruction,
}

/// Pledge of an exact security lot under an agreement (a funding lot or a margin agreement).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pledge {
    pub pledge_id: String,
    pub lot_id: String,
    pub agreement_id: String,
    pub holder: String,
    pub quantity: f64,
    pub eligibility: Revision,
    pub haircut: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitialMarginMode {
    None,
    SuppliedRequirements,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Segregation {
    Segregated,
    Reusable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VmAtTermination {
    /// Existing VM is returned as its own obligation.
    ReturnSeparately,
    /// Existing VM is applied against termination consideration.
    ApplyAgainstConsideration,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MarginAgreement {
    pub agreement_id: String,
    pub counterparty: String,
    pub legal_review: Revision,
    pub settlement_lag_days: u32,
    pub initial_margin: InitialMarginMode,
    pub segregation: Segregation,
    pub vm_at_termination: VmAtTermination,
    pub calendar: Revision,
    /// Derivative lots in the agreement's legal scope.
    pub lot_ids: Vec<String>,
    /// Collateral already exchanged at the opening under this agreement.
    #[serde(default)]
    pub opening_collateral: Option<OpeningCollateral>,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningCollateral {
    pub posted_vm: f64,
    pub received_vm: f64,
    pub posted_im: f64,
    pub received_im_custody: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Facility {
    pub facility_id: String,
    pub provider: String,
    pub commitment: f64,
    pub drawn: f64,
    pub access_policy: Revision,
    pub expiry_ordinal: i32,
}

/// Handoff to the ASC 815 owner (#35): the formal relationship identity and its accounting
/// source. Lifecycle facts enter that owner; this plan never posts formal basis or OCI itself.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalRelationship {
    pub relationship_id: String,
    pub revision: String,
    pub accounting_source: SourceRef,
    pub adoption_policy: Revision,
    pub hedging_lot_ids: Vec<String>,
    #[serde(default)]
    pub hedged_lot_ids: Vec<String>,
    #[serde(default)]
    pub forecast_exposure_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Acquisition,
    Unwind,
    Offset,
    Rebalance,
    Refinance,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountingTreatment {
    /// Reviewed undesignated economic treatment.
    Economic,
    /// Requires an implemented formal owner for every affected hedge; never falls back.
    Formal,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionLot {
    pub lot_id: String,
    pub quantity: f64,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleAction {
    pub action_id: String,
    pub kind: ActionKind,
    pub trade_ordinal: i32,
    pub effective_ordinal: i32,
    pub settlement_ordinal: i32,
    pub lots: Vec<ActionLot>,
    pub policy: Revision,
    pub accounting_treatment: AccountingTreatment,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationKind {
    FinancingDraw,
    FinancingRepayment,
    CollateralRelease,
    SecurityDelivery,
    SecurityReceipt,
    CashPayment,
    CashReceipt,
    TerminationPayment,
    TerminationReceipt,
    CollateralReturnReceived,
    CollateralReturnPaid,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollateralKind {
    VariationMargin,
    InitialMargin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationState {
    Pending,
    Settled,
    Failed,
}
/// One settlement obligation with an exclusive source event. `cash` is the signed movement in
/// the account's cash (receipts positive); it is zero for nonmonetary deliveries and releases.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementObligation {
    pub obligation_id: String,
    pub source_event: String,
    pub account_id: String,
    pub kind: ObligationKind,
    pub cash: f64,
    #[serde(default)]
    pub lot_id: Option<String>,
    #[serde(default)]
    pub quantity: f64,
    #[serde(default)]
    pub pledge_id: Option<String>,
    #[serde(default)]
    pub facility_id: Option<String>,
    #[serde(default)]
    pub action_id: Option<String>,
    /// Margin agreement of a termination or collateral return.
    #[serde(default)]
    pub agreement_id: Option<String>,
    #[serde(default)]
    pub collateral: Option<CollateralKind>,
    pub due_ordinal: i32,
    pub phase: u32,
    pub sequence: u32,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub state: ObligationState,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleRequest {
    pub schema: String,
    pub model_version: String,
    pub workspace_revision: Revision,
    pub saved_book_revision: Revision,
    pub entity: String,
    pub currency: String,
    pub monetary_unit: String,
    pub as_of_ordinal: i32,
    pub settlement_policy: Revision,
    pub sources: Vec<SourceRef>,
    pub accounts: Vec<OpeningAccount>,
    pub lots: Vec<PositionLot>,
    #[serde(default)]
    pub hedge_allocations: Vec<HedgeAllocation>,
    #[serde(default)]
    pub funding_allocations: Vec<FundingAllocation>,
    #[serde(default)]
    pub pledges: Vec<Pledge>,
    #[serde(default)]
    pub margin_agreements: Vec<MarginAgreement>,
    #[serde(default)]
    pub facilities: Vec<Facility>,
    #[serde(default)]
    pub formal_relationships: Vec<FormalRelationship>,
    #[serde(default)]
    pub actions: Vec<LifecycleAction>,
    #[serde(default)]
    pub obligations: Vec<SettlementObligation>,
}

/// Structured refusal. Malformed input is distinguished from a valid but infeasible plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refusal {
    pub code: &'static str,
    pub detail: String,
}
impl Refusal {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
pub const MALFORMED_INPUT: &str = "MALFORMED_INPUT";
pub const BOUND_EXCEEDED: &str = "BOUND_EXCEEDED";
pub const DUPLICATE_IDENTITY: &str = "DUPLICATE_IDENTITY";
pub const LIFECYCLE_MAPPING_REQUIRED: &str = "LIFECYCLE_MAPPING_REQUIRED";
pub const OVERALLOCATION: &str = "OVERALLOCATION";
pub const UNSUPPORTED_POLICY: &str = "UNSUPPORTED_POLICY";
pub const FORMAL_OWNER_REQUIRED: &str = "FORMAL_OWNER_REQUIRED";
pub const SETTLEMENT_DEPENDENCY_CYCLE: &str = "SETTLEMENT_DEPENDENCY_CYCLE";
pub const SETTLEMENT_ORDER: &str = "SETTLEMENT_ORDER";
pub const INSUFFICIENT_SETTLEMENT_LIQUIDITY: &str = "INSUFFICIENT_SETTLEMENT_LIQUIDITY";
pub const INVENTORY_UNAVAILABLE: &str = "INVENTORY_UNAVAILABLE";
pub const DUPLICATE_COLLATERAL_CREDIT: &str = "DUPLICATE_COLLATERAL_CREDIT";
