//! Native typed receipts; caller supplies raw contracts, never journal arrays.
use portfolio_model_core::{card_credit_accounting as card, card_credit_accounting_v2 as card_v2};
use portfolio_model_core::{
    consumer_contract::ConsumerIdentity,
    consumer_credit_accounting as installment,
    credit_accounting::{AccountingPolicy, AccountingState, Identity, JournalIntent},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Request {
    Installment(Box<installment::Request>),
    Card(Box<card::Request>),
    CardV2(Box<card_v2::Request>),
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum Native {
    Installment(Box<installment::ResultData>),
    Card(Box<card::ResultData>),
    CardV2(Box<card_v2::ResultData>),
}
pub struct Contract<'a> {
    pub identity: &'a ConsumerIdentity,
    pub as_of_ordinal: i32,
    pub horizon_days: u32,
}
#[derive(Serialize)]
pub struct Acquisition<'a> {
    pub identity: &'a Identity,
    pub original_yield_anchor: &'a Identity,
    pub income_review: &'a Identity,
    pub accounting_basis: &'a str,
    pub purchase_ordinal: i32,
    pub principal: f64,
    pub cost: f64,
    pub original_annual_effective_rate: f64,
}
#[derive(Serialize)]
pub struct LegacyPolicy<'a> {
    pub accounting: &'a AccountingPolicy,
    pub recovery_review: &'a Identity,
    pub cost_review: &'a Identity,
    pub exact_raw_policy: PolicyView<'a>,
}
#[derive(Serialize)]
pub struct CardV2Policy<'a> {
    pub accounting: &'a AccountingPolicy,
    pub exact_raw_policy: PolicyView<'a>,
}
#[derive(Serialize)]
#[serde(untagged)]
pub enum Policy<'a> {
    Legacy(LegacyPolicy<'a>),
    CardV2(CardV2Policy<'a>),
}
#[derive(Serialize)]
#[serde(untagged)]
pub enum PolicyView<'a> {
    Installment(&'a installment::Policy),
    Card(&'a card::Policy),
    CardV2(&'a card_v2::RightsPolicy),
}
pub struct View<'a> {
    pub kind: &'static str,
    pub contract: Contract<'a>,
    pub acquisition: Acquisition<'a>,
    pub policy: Policy<'a>,
    pub economic_source: &'a Identity,
    pub path_id: Option<&'a str>,
    pub claim_id: &'a str,
    pub reporting_end_ordinal: i32,
    pub opening_allowance: f64,
    pub actions: usize,
    pub reviews: usize,
}
impl Request {
    pub fn view(&self) -> View<'_> {
        match self {
            Self::Installment(r) => RequestView::installment(r),
            Self::Card(r) => RequestView::card(r),
            Self::CardV2(r) => RequestView::card_v2(r),
        }
    }
    pub fn original_terms(&self) -> Value {
        match self {
            Self::Installment(_) => Value::Null,
            Self::Card(r) => card_original_terms(&r.contract),
            Self::CardV2(r) => card_original_terms(&r.contract),
        }
    }
    pub fn validate_source(&self) -> Result<(), String> {
        if self.view().contract.identity.measure
            != portfolio_model_core::cashflow::CashflowMeasure::Physical
        {
            return Err("product accounting owner requires physical raw histories".into());
        }
        match self {
            Self::CardV2(r) => {
                card_v2::audit_request_with_check(r, &mut portfolio_compute_control::checkpoint)
                    .map(|_| ())
            }
            Self::Installment(r) => {
                portfolio_model_core::consumer_installment::validate_request(&r.contract)
            }
            Self::Card(r) => {
                // Artifact seam is stricter than the economic source before any
                // generation. The native adapter repeats its complete policy checks.
                portfolio_model_core::consumer_card::validate_request(&r.contract)?;
                if r.contract.initial_purchase_grace
                    || r.contract.regain_grace_after_full_statement_payment
                    || r.contract.opening_fees.iter().any(|v| *v != 0.)
                    || r.contract
                        .opening_unrecognized_billed_interest
                        .iter()
                        .any(|v| *v != 0.)
                    || r.contract.opening_contingent_purchase_interest != 0.
                {
                    return Err("card owner rejects grace/fees/unrecognized ownership".into());
                }
                for s in &r.contract.steps {
                    if !matches!(
                        s.action,
                        portfolio_model_core::consumer_card::CardAction::Payment { .. }
                            | portfolio_model_core::consumer_card::CardAction::Default {}
                            | portfolio_model_core::consumer_card::CardAction::SetLimit {
                                amount: 0.0
                            }
                    ) {
                        return Err("card owner rejects draw/CCF/fee or other ownership".into());
                    }
                }
                Ok(())
            }
        }
    }
    pub fn evaluate(&self) -> Result<Native, String> {
        match self {
            Self::Installment(r) => {
                installment::evaluate_with_check(r, portfolio_compute_control::checkpoint)
                    .map(|result| Native::Installment(Box::new(result)))
            }
            Self::Card(r) => card::evaluate_with_check(r, portfolio_compute_control::checkpoint)
                .map(|result| Native::Card(Box::new(result))),
            Self::CardV2(r) => {
                card_v2::evaluate_with_check(r, portfolio_compute_control::checkpoint)
                    .map(|result| Native::CardV2(Box::new(result)))
            }
        }
    }
}
impl Native {
    pub fn raw(&self) -> View<'_> {
        match self {
            Self::Installment(r) => RequestView::installment(&r.raw_request),
            Self::Card(r) => RequestView::card(&r.raw_request),
            Self::CardV2(r) => RequestView::card_v2(&r.raw_request),
        }
    }
    pub fn opening(&self) -> &AccountingState {
        match self {
            Self::Installment(r) => &r.acquisition_opening,
            Self::Card(r) => &r.acquisition_opening,
            Self::CardV2(r) => &r.acquisition_opening.core,
        }
    }
    pub fn closing(&self) -> &AccountingState {
        match self {
            Self::Installment(r) => &r.closing,
            Self::Card(r) => &r.closing,
            Self::CardV2(r) => &r.closing.core,
        }
    }
    pub fn intents(&self) -> &[JournalIntent] {
        match self {
            Self::Installment(r) => &r.intents,
            Self::Card(r) => &r.intents,
            Self::CardV2(_) => &[], // referenced shared intents are evidence, never another posting path
        }
    }
    pub fn owned(&self, day: usize) -> &AccountingState {
        match self {
            Self::Installment(r) => &r.daily[day.min(r.daily.len() - 1)].owned,
            Self::Card(r) => &r.daily[day.min(r.daily.len() - 1)].owned,
            Self::CardV2(r) => &r.daily[day.min(r.daily.len() - 1)].snapshot.core,
        }
    }
    pub fn lineage(&self, index: usize) -> Value {
        match self {
            Self::Installment(r) => json!(r.lineage[index]),
            Self::Card(r) => json!(r.lineage[index]),
            Self::CardV2(r) => json!(r.actions[index].source),
        }
    }
}
// These borrowed constructors keep Native::raw allocation-free apart from the
// small explicit policy JSON used for exact admitted anchor comparison.
pub(crate) struct RequestView;
impl RequestView {
    pub(crate) fn installment(r: &installment::Request) -> View<'_> {
        View {
            kind: "installment",
            contract: Contract {
                identity: &r.contract.identity,
                as_of_ordinal: r.contract.as_of_ordinal,
                horizon_days: r.contract.horizon_days,
            },
            acquisition: Acquisition {
                identity: &r.acquisition.identity,
                original_yield_anchor: &r.acquisition.original_yield_anchor,
                income_review: &r.acquisition.income_review,
                accounting_basis: &r.acquisition.accounting_basis,
                purchase_ordinal: r.acquisition.purchase_ordinal,
                principal: r.acquisition.principal,
                cost: r.acquisition.cost,
                original_annual_effective_rate: r.acquisition.original_annual_effective_rate,
            },
            policy: Policy::Legacy(LegacyPolicy {
                accounting: &r.policy.accounting,
                recovery_review: &r.policy.recovery_review,
                cost_review: &r.policy.cost_review,
                exact_raw_policy: PolicyView::Installment(&r.policy),
            }),
            economic_source: &r.economic_source,
            path_id: Some(&r.path_id),
            claim_id: &r.claim_id,
            reporting_end_ordinal: r.reporting_end_ordinal,
            opening_allowance: r.opening_allowance,
            actions: r.contract.payment_dates.len() + r.contract.steps.len(),
            reviews: r.allowance_reviews.len() + r.chargeoffs.len(),
        }
    }
    pub(crate) fn card(r: &card::Request) -> View<'_> {
        View {
            kind: "card",
            contract: Contract {
                identity: &r.contract.identity,
                as_of_ordinal: r.contract.as_of_ordinal,
                horizon_days: r.contract.horizon_days,
            },
            acquisition: Acquisition {
                identity: &r.acquisition.identity,
                original_yield_anchor: &r.acquisition.original_yield_anchor,
                income_review: &r.acquisition.income_review,
                accounting_basis: &r.acquisition.accounting_basis,
                purchase_ordinal: r.acquisition.purchase_ordinal,
                principal: r.acquisition.principal,
                cost: r.acquisition.cost,
                original_annual_effective_rate: r.acquisition.original_annual_effective_rate,
            },
            policy: Policy::Legacy(LegacyPolicy {
                accounting: &r.policy.accounting,
                recovery_review: &r.policy.recovery_review,
                cost_review: &r.policy.cost_review,
                exact_raw_policy: PolicyView::Card(&r.policy),
            }),
            economic_source: &r.economic_source,
            path_id: Some(&r.path_id),
            claim_id: &r.claim_id,
            reporting_end_ordinal: r.reporting_end_ordinal,
            opening_allowance: r.opening_allowance,
            actions: r.contract.steps.len() + r.contract.cycles.len(),
            reviews: r.allowance_reviews.len() + r.chargeoffs.len(),
        }
    }
    pub(crate) fn card_v2(r: &card_v2::Request) -> View<'_> {
        View {
            kind: "card_v2",
            contract: Contract {
                identity: &r.contract.identity,
                as_of_ordinal: r.contract.as_of_ordinal,
                horizon_days: r.contract.horizon_days,
            },
            acquisition: Acquisition {
                identity: &r.review.identity,
                original_yield_anchor: &r.review.original_yield_anchor,
                income_review: &r.review.identity,
                accounting_basis: &r.review.accounting_basis,
                purchase_ordinal: r.review.acquisition_ordinal,
                principal: r.review.principal,
                cost: r.review.cost,
                original_annual_effective_rate: r.review.original_annual_effective_rate,
            },
            policy: Policy::CardV2(CardV2Policy {
                accounting: &r.review.policy,
                exact_raw_policy: PolicyView::CardV2(&r.policy),
            }),
            economic_source: &r.review.economic_source,
            path_id: None,
            claim_id: &r.claim_id,
            reporting_end_ordinal: r.reporting_ordinal,
            opening_allowance: 0.,
            actions: r.contract.steps.len() + r.contract.cycles.len(),
            reviews: r.interest_recoveries.len(),
        }
    }
}

fn card_original_terms(r: &portfolio_model_core::consumer_card::CardRequest) -> Value {
    let mut v = json!(r);
    for k in [
        "steps",
        "horizon_days",
        "collection_fraction",
        "collection_cost",
        "recovery_delay_days",
    ] {
        v.as_object_mut().expect("typed card object").remove(k);
    }
    v["identity"]
        .as_object_mut()
        .expect("typed identity object")
        .remove("run_id");
    v
}
