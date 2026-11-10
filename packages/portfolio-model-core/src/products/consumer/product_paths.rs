//! Selected analytical bridge from fitted CTMC states to daily consumer owners.
//! Shared paths carry zero monetary stocks; product engines own every cash/right.
use super::behavior::{self, Product};
use super::contracts::{ConsumerIdentity, ConsumerState};
use crate::cashflow::CashflowMeasure;
use crate::credit::CreditState;
use crate::{consumer_card as card, consumer_installment as loan};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub const SCHEMA: &str = "consumer-behaviour-product-paths-1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockPolicy {
    NextCalendarDayBoundary,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    Reject,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub review: crate::credit_accounting::Identity,
    pub clock: ClockPolicy,
    pub same_day_transitions: CollisionPolicy,
    /// Explicit reduction of performing/watch/delinquent to product live states.
    pub live_states: [ConsumerState; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProductSource {
    Installment {
        path_id: String,
        path_key: u64,
        contract: Box<loan::InstallmentRequest>,
    },
    Card {
        path_id: String,
        path_key: u64,
        contract: Box<card::CardRequest>,
    },
}
impl ProductSource {
    fn identity(&self) -> (&str, u64, &ConsumerIdentity, i32, u32, ConsumerState) {
        match self {
            Self::Installment {
                path_id,
                path_key,
                contract: r,
            } => (
                path_id,
                *path_key,
                &r.identity,
                r.as_of_ordinal,
                r.horizon_days,
                r.initial_state,
            ),
            Self::Card {
                path_id,
                path_key,
                contract: r,
            } => (
                path_id,
                *path_key,
                &r.identity,
                r.as_of_ordinal,
                r.horizon_days,
                r.initial_state,
            ),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub timeout_ms: u64,
    pub fit: behavior::Request,
    pub projection: behavior::Projection,
    pub policy: Policy,
    pub products: Vec<ProductSource>,
}
#[derive(Debug, Serialize)]
pub struct MappedTransition {
    pub event_id: String,
    pub original_year: f64,
    pub ordinal: i32,
    pub origin: CreditState,
    pub destination: CreditState,
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProductResult {
    Installment {
        path_id: String,
        path_key: u64,
        transitions: Vec<MappedTransition>,
        compiled_source: Box<loan::InstallmentRequest>,
        result: Box<loan::InstallmentResult>,
    },
    Card {
        path_id: String,
        path_key: u64,
        transitions: Vec<MappedTransition>,
        closure_ordinal: Option<i32>,
        compiled_source: Box<card::CardRequest>,
        result: Box<card::CardResult>,
    },
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub fitted: behavior::ResultData,
    pub credit_states: behavior::ProjectionResult,
    pub products: Vec<ProductResult>,
    pub prospective_product_rows: usize,
    pub actual_product_rows: usize,
    pub reserved_response_bytes: usize,
    pub adoption_eligible: bool,
    pub empirical_accuracy_validated: bool,
}

fn live(state: CreditState, policy: &Policy) -> Result<ConsumerState, String> {
    Ok(policy.live_states[match state {
        CreditState::Performing => 0,
        CreditState::Watch => 1,
        CreditState::Delinquent => 2,
        _ => return Err("consumer product opening must be live".into()),
    }])
}

#[derive(Debug)]
pub struct Admission {
    pub product_rows: usize,
    pub response_bytes: usize,
}

/// Read every raw owner before fitting; independent of realized transition counts.
pub fn admit(r: &Request) -> Result<Admission, String> {
    let raw_bytes = crate::strict_json::serialized_size(r, 1 << 20)?;
    let source = &r.projection.path_source;
    if r.schema != SCHEMA
        || !(1..=120000).contains(&r.timeout_ms)
        || r.products.is_empty()
        || r.products.len() > 16
        || r.products.len() != source.paths.len()
        || source.origin_time.seconds_of_day != 0
        || !source.boundaries.is_empty()
        || source.measure != crate::credit::CreditMeasure::Physical
        || r.policy.review.id.trim().is_empty()
        || r.policy.review.id.len() > 256
        || r.policy.review.revision.trim().is_empty()
        || r.policy.review.revision.len() > 256
        || r.policy.review.id.chars().any(char::is_control)
        || r.policy.review.revision.chars().any(char::is_control)
        || r.policy
            .live_states
            .iter()
            .any(|s| matches!(s, ConsumerState::Default | ConsumerState::PaidOff))
    {
        return Err("consumer product policy/count/clock admission".into());
    }
    let shared_events = behavior::projection_event_upper(&r.projection)?;
    let end = r
        .projection
        .intervals
        .last()
        .ok_or("consumer product missing horizon")?
        .end;
    if end.seconds_of_day != 0 {
        return Err("consumer product end must be a day boundary".into());
    }
    let mut days = 0usize;
    let mut rows = 0usize;
    let mut identities = HashSet::new();
    for (product, path) in r.products.iter().zip(&source.paths) {
        let (id, key, identity, asof, horizon, initial) = product.identity();
        days = days
            .checked_add(horizon as usize + 1)
            .ok_or("consumer product day work overflow")?;
        if id != path.path_id
            || key != path.path_key
            || !identities.insert((id, key))
            || identity.measure != CashflowMeasure::Physical
            || identity.currency != source.monetary_unit
            || asof != source.origin_time.ordinal
            || i64::from(asof) + i64::from(horizon) != i64::from(end.ordinal)
            || initial != live(path.initial_live_state, &r.policy)?
            || horizon > 4096
            || days > 4096
            || [
                path.principal,
                path.undrawn_commitment,
                path.accrued_interest,
                path.book_adjustment,
            ]
            .iter()
            .any(|v| *v != 0.)
        {
            return Err(
                "consumer product source identity/grid/zero monetary state admission".into(),
            );
        }
        match product {
            ProductSource::Installment { contract: q, .. } => {
                let matching = matches!(
                    (&q.product, r.fit.specification.product),
                    (loan::InstallmentProduct::Auto { .. }, Product::Auto)
                        | (loan::InstallmentProduct::Personal { .. }, Product::Personal)
                );
                if !matching
                    || q.steps.iter().any(|s| {
                        matches!(
                            s.action,
                            loan::InstallmentAction::SetState { .. }
                                | loan::InstallmentAction::Default {}
                                | loan::InstallmentAction::Payoff {}
                        )
                    })
                {
                    return Err(
                        "consumer product installment type or competing supplied transition".into(),
                    );
                }
                loan::validate_request(q)?;
                rows = rows
                    .checked_add(
                        4 * (q.steps.len() + q.payment_dates.len() + source.max_jumps_per_path + 2),
                    )
                    .ok_or("consumer product row overflow")?;
            }
            ProductSource::Card { contract: q, .. } => {
                if r.fit.specification.product != Product::CreditCard
                    || q.steps.iter().any(|s| {
                        matches!(
                            s.action,
                            card::CardAction::SetState { .. } | card::CardAction::Default {}
                        )
                    })
                {
                    return Err(
                        "consumer product card type or competing supplied transition".into(),
                    );
                }
                card::validate_request(q)?;
                let lots = 3 + q
                    .steps
                    .iter()
                    .filter(|s| {
                        matches!(
                            s.action,
                            card::CardAction::Draw { .. } | card::CardAction::Fee { .. }
                        )
                    })
                    .count();
                // Daily points, up to two recognition records per lot/day,
                // cash actions, statements and default/recovery metadata.
                rows = rows
                    .checked_add(
                        (horizon as usize + 1) * (1 + 2 * lots)
                            + 2 * q.steps.len()
                            + q.cycles.len()
                            + 8,
                    )
                    .ok_or("consumer product row overflow")?;
            }
        }
    }
    if rows > 4096 {
        return Err("consumer product aggregate full-tail row admission".into());
    }
    if source.intervals.iter().any(|i| {
        i.accrual_rates
            .iter()
            .chain(&i.credit_conversion_factor)
            .any(|v| *v != 0.)
    }) {
        return Err(
            "consumer product shared state source cannot own accrual or default draws".into(),
        );
    }
    // Joined response budget policy, rechecked on actual serialized output.
    // It reserves raw/prepared copies and both owners before either is generated.
    let response_bytes = raw_bytes
        .checked_mul(12)
        .and_then(|v| v.checked_add(4 << 20))
        .and_then(|v| {
            (shared_events + rows)
                .checked_mul(4096)
                .and_then(|extra| v.checked_add(extra))
        })
        .ok_or("consumer product joined response overflow")?;
    if response_bytes > 32 << 20 {
        return Err("consumer product joined response admission".into());
    }
    Ok(Admission {
        product_rows: rows,
        response_bytes,
    })
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_checked(r, &mut || Ok(()))
}
pub fn evaluate_checked(
    r: &Request,
    external: &mut impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let started = Instant::now();
    let mut check = || {
        external()?;
        if started.elapsed() >= Duration::from_millis(r.timeout_ms) {
            Err("consumer product original deadline exceeded".into())
        } else {
            Ok(())
        }
    };
    check()?;
    let admission = admit(r)?;
    let prospective_product_rows = admission.product_rows;
    crate::credit_path::validate_request_with_check(&r.projection.path_source, &mut check)?;
    check()?;
    let fitted = behavior::fit_checked(&r.fit, &mut check)?;
    let credit_states = fitted.project_paths_checked(&r.projection, &mut check)?;
    let mut products = Vec::with_capacity(r.products.len());
    for (product, path) in r.products.iter().zip(&credit_states.paths.paths) {
        check()?;
        let (id, key, _, asof, horizon, _) = product.identity();
        let mut transitions = Vec::new();
        let mut dates = HashSet::new();
        for event in &path.events {
            check()?;
            let offset = (event.year * 365.).ceil();
            if !offset.is_finite()
                || offset < 0.
                || offset > f64::from(horizon)
                || !matches!(event.kind.as_str(), "transition" | "prepayment")
            {
                return Err("consumer product event clock/kind".into());
            }
            let ordinal = asof
                .checked_add(offset as i32)
                .ok_or("consumer product event ordinal overflow")?;
            if !dates.insert(ordinal) {
                return Err("consumer product multiple transitions on one mapped day".into());
            }
            transitions.push(MappedTransition {
                event_id: event.event_id.clone(),
                original_year: event.year,
                ordinal,
                origin: event.origin,
                destination: event
                    .destination
                    .ok_or("consumer product event has no destination")?,
            });
        }
        match product {
            ProductSource::Installment { contract, .. } => {
                let mut compiled = contract.clone();
                for t in &transitions {
                    if compiled.steps.iter().any(|s| s.ordinal == t.ordinal) {
                        return Err(
                            "consumer product supplied action collides with transition".into()
                        );
                    }
                    let action = match t.destination {
                        CreditState::Default => loan::InstallmentAction::Default {},
                        CreditState::Prepaid => loan::InstallmentAction::Payoff {},
                        state => loan::InstallmentAction::SetState {
                            state: live(state, &r.policy)?,
                        },
                    };
                    compiled.steps.push(loan::InstallmentStep {
                        ordinal: t.ordinal,
                        action,
                    });
                }
                compiled.steps.sort_by_key(|s| s.ordinal);
                check()?;
                let result = loan::project_with_check(&compiled, &mut check)?;
                check()?;
                products.push(ProductResult::Installment {
                    path_id: id.into(),
                    path_key: key,
                    transitions,
                    compiled_source: compiled,
                    result: Box::new(result),
                });
            }
            ProductSource::Card { contract, .. } => {
                let mut compiled = contract.clone();
                let mut closure = None;
                for t in &transitions {
                    if t.destination == CreditState::Prepaid {
                        closure = Some(t.ordinal);
                        continue;
                    }
                    if compiled.steps.iter().any(|s| s.ordinal == t.ordinal) {
                        return Err(
                            "consumer product supplied action collides with transition".into()
                        );
                    }
                    let action = if t.destination == CreditState::Default {
                        card::CardAction::Default {}
                    } else {
                        card::CardAction::SetState {
                            state: live(t.destination, &r.policy)?,
                        }
                    };
                    compiled.steps.push(card::CardStep {
                        ordinal: t.ordinal,
                        action,
                    });
                }
                compiled.steps.sort_by_key(|s| s.ordinal); // Stable for existing equal-day transactions.
                check()?;
                let result = card::simulate_with_check(&compiled, closure, &mut check)?;
                check()?;
                products.push(ProductResult::Card {
                    path_id: id.into(),
                    path_key: key,
                    transitions,
                    closure_ordinal: closure,
                    compiled_source: compiled,
                    result: Box::new(result),
                });
            }
        }
    }
    let actual_product_rows = products
        .iter()
        .map(|p| match p {
            ProductResult::Installment { result, .. } => {
                result.points.len()
                    + result.recognition.len()
                    + result.cashflows.len()
                    + usize::from(result.default.is_some())
            }
            ProductResult::Card { result, .. } => {
                result.points.len()
                    + result.recognition.len()
                    + result.cashflows.len()
                    + result.statements.len()
                    + usize::from(result.default.is_some())
            }
        })
        .sum::<usize>();
    if actual_product_rows > prospective_product_rows {
        return Err("consumer product prospective row bound violated".into());
    }
    let result = ResultData {
        schema: SCHEMA,
        raw_source: r.clone(),
        fitted,
        credit_states,
        products,
        prospective_product_rows,
        actual_product_rows,
        reserved_response_bytes: admission.response_bytes,
        adoption_eligible: false,
        empirical_accuracy_validated: false,
    };
    crate::strict_json::serialized_size(&result, admission.response_bytes)?;
    check()?;
    Ok(result)
}
