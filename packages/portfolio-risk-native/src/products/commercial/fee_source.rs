//! Separate service-fee rights bound to the already generated physical loan path.
//! Neither loan LGD nor loan coupon recovery fractions own fee money.
use crate::products::commercial::credit_compiler as loan;
use portfolio_model_core::{
    cashflow::CashflowMeasure, credit::CreditState, credit_accounting::Identity,
    nonintegral_fee as fee, strict_json,
};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "commercial-native-fees-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    pub lag_days: u32,
    pub fraction: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExitPolicy {
    pub review: Identity,
    pub cancel_future_service: bool,
    pub cancel_future_cash: bool,
    pub cancel_unearned_entitlement: bool,
    pub waive_unpaid_earned_fraction: f64,
    pub refund_prepaid_fraction: f64,
    pub recoveries: Vec<Recovery>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: String,
    pub policy: Identity,
    pub complete_end_ordinal: i32,
    pub fees: Vec<fee::Fee>,
    pub events: Vec<fee::Event>,
    pub default: ExitPolicy,
    pub prepayment: ExitPolicy,
    pub exercise: ExitPolicy,
    pub maturity: ExitPolicy,
    /// This policy is explicit; no financial clock is silently nudged.
    pub coincident_order: String,
}
#[derive(Debug, Serialize)]
pub struct ExitEvidence {
    pub native_event_id: String,
    pub native_year: f64,
    pub reason: fee::Reason,
}
#[derive(Debug, Serialize)]
pub struct BoundPath {
    path_key: u64,
    position_id: String,
    scenario: String,
    native_years: Vec<f64>,
    source: fee::ResultData,
    exit: Option<ExitEvidence>,
    native_credit_event_binding: bool,
    loan_carrying_value_changed: bool,
}
impl BoundPath {
    pub fn path_key(&self) -> u64 {
        self.path_key
    }
    pub fn source(&self) -> &fee::ResultData {
        &self.source
    }
    pub fn native_years(&self) -> &[f64] {
        &self.native_years
    }
}
fn checked_policy(p: &ExitPolicy) -> Result<(), String> {
    if p.review.id.is_empty()
        || p.review.revision.is_empty()
        || p.review.id.len() > 128
        || p.review.revision.len() > 128
        || p.review.id.chars().any(char::is_control)
        || p.review.revision.chars().any(char::is_control)
        || !p.waive_unpaid_earned_fraction.is_finite()
        || !(0. ..=1.).contains(&p.waive_unpaid_earned_fraction)
        || !p.refund_prepaid_fraction.is_finite()
        || !(0. ..=1.).contains(&p.refund_prepaid_fraction)
        || p.recoveries.len() > 32
    {
        return Err("explicit bounded fee exit review required".into());
    }
    let mut total = 0.;
    let mut previous = 0;
    for r in &p.recoveries {
        if r.lag_days == 0
            || r.lag_days <= previous
            || r.lag_days > 1080
            || !r.fraction.is_finite()
            || !(0. ..=1.).contains(&r.fraction)
        {
            return Err(
                "fee recoveries require ordered positive actual-day lags and independent fractions"
                    .into(),
            );
        }
        total += r.fraction;
        previous = r.lag_days;
    }
    if total > 1. || total > 0. && p.waive_unpaid_earned_fraction == 0. {
        return Err("fee recovery exceeds separately reviewed waived right".into());
    }
    Ok(())
}
pub fn admit(p: &Policy, r: &loan::Request) -> Result<(usize, usize), String> {
    portfolio_compute_control::checkpoint()?;
    if p.schema != SCHEMA
        || p.coincident_order != "native_exit_before_coincident_service_cash-1"
        || p.fees.is_empty()
        || p.fees.len() > 64
        || p.events.len() > 512
        || r.credit.paths.len() > 256
    {
        return Err("explicit bounded native fee binding required".into());
    }
    for policy in [&p.default, &p.prepayment, &p.exercise, &p.maturity] {
        checked_policy(policy)?;
    }
    for f in &p.fees {
        if f.position_id != r.source.current.position_id || f.tranche_id != "original" {
            return Err("fee contract requires exact position and original tranche; drawn/secondary fee rights need separate acquisition ownership".into());
        }
        if !f.prepaid_refunds_allowed
            && [&p.default, &p.prepayment, &p.exercise, &p.maturity]
                .iter()
                .any(|policy| policy.refund_prepaid_fraction > 0.)
        {
            return Err("fee exit refund policy contradicts supplied fee terms".into());
        }
    }
    for e in &p.events {
        if !matches!(
            e.action,
            fee::Action::Earn | fee::Action::Cash | fee::Action::RefundPrepaid
        ) {
            return Err(
                "fee disposition money must derive from the actual native loan exit".into(),
            );
        }
        if e.event_id.starts_with("native-fee:") {
            return Err("reserved native fee event namespace".into());
        }
    }
    let origin = r.source.current.asof_ordinal;
    let end = p.complete_end_ordinal;
    portfolio_model_core::cashflow::validate_ordinal(end)?;
    if end < r.source.current.tail_end_ordinal || end - origin > 1080 {
        return Err("complete fee tail must cover the loan contract within1080 days".into());
    }
    strict_json::serialized_size(p, 1 << 20)?;
    // Validate the complete supplied service/cash contract before any loan path
    // generation. Exit policies may remove events; they cannot legalize a bad
    // original source, duplicate identity, unavailable right or invalid clock.
    fee::evaluate_with_check(
        &request(p, r, p.events.clone()),
        portfolio_compute_control::checkpoint,
    )?;
    let recoveries = [&p.default, &p.prepayment, &p.exercise, &p.maturity]
        .iter()
        .map(|p| p.recoveries.len())
        .max()
        .unwrap_or(0);
    let events = p.events.len() + p.fees.len() * (3 + recoveries);
    let cells = events
        .checked_mul(r.credit.paths.len())
        .ok_or("fee admission overflow")?;
    let bytes = cells.checked_mul(4096).ok_or("fee admission overflow")?;
    let work = cells.checked_mul(128).ok_or("fee admission overflow")?;
    if work > 8_000_000 || bytes > 32 << 20 {
        return Err("native fee joined capacity exceeds8M/32MiB before loan generation".into());
    }
    Ok((work, bytes))
}
fn ordinal(origin: i32, year: f64) -> Result<i32, String> {
    if !year.is_finite() || year < 0. {
        return Err("fee native credit clock domain".into());
    }
    let elapsed = year * 365.;
    let round = elapsed.round();
    let mut days = if (round / 365.).to_bits() == year.to_bits() {
        round
    } else {
        elapsed.ceil()
    };
    if year > days / 365. {
        days += 1.;
    }
    origin
        .checked_add(days as i32)
        .ok_or("fee ordinal overflow".into())
}
fn request(p: &Policy, r: &loan::Request, events: Vec<fee::Event>) -> fee::Request {
    fee::Request {
        schema: fee::SCHEMA.into(),
        measure: CashflowMeasure::Physical,
        currency: r.source.current.currency.clone(),
        monetary_unit: r.source.current.monetary_unit.clone(),
        policy: p.policy.clone(),
        asof_ordinal: r.source.current.asof_ordinal,
        reporting_end_ordinal: r.source.current.reporting_end_ordinal,
        complete_end_ordinal: p.complete_end_ordinal,
        fees: p.fees.clone(),
        events,
    }
}
pub fn bind(p: &Policy, source: &loan::ResultData) -> Result<Vec<BoundPath>, String> {
    let r = source.raw_request();
    admit(p, r)?;
    let origin = r.source.current.asof_ordinal;
    let mut out = Vec::new();
    for path in &source.carry().credit().result.paths {
        portfolio_compute_control::checkpoint()?;
        let mut exit = path
            .events
            .iter()
            .find_map(|e| match e.destination {
                Some(CreditState::Default) => Some(ExitEvidence {
                    native_event_id: e.event_id.clone(),
                    native_year: e.year,
                    reason: fee::Reason::Default,
                }),
                Some(CreditState::Prepaid) => Some(ExitEvidence {
                    native_event_id: e.event_id.clone(),
                    native_year: e.year,
                    reason: fee::Reason::OtherReviewed,
                }),
                _ => None,
            })
            .or_else(|| {
                path.events
                    .iter()
                    .find(|e| e.kind == "maturity")
                    .map(|e| ExitEvidence {
                        native_event_id: e.event_id.clone(),
                        native_year: e.year,
                        reason: fee::Reason::Maturity,
                    })
            });
        if let Some(x) = exit.as_mut() {
            if source.native_stops().iter().any(|stop| {
                stop.path_key() == path.path_key
                    && stop.event_id() == x.native_event_id
                    && (f64::from(stop.ordinal() - origin) / 365.).to_bits()
                        == x.native_year.to_bits()
            }) {
                x.reason = fee::Reason::Exercise;
            }
        }
        let mut timeline: Vec<(f64, fee::Event)> = Vec::new();
        let prefix = p
            .events
            .iter()
            .filter(|e| {
                exit.as_ref()
                    .is_none_or(|x| f64::from(e.ordinal - origin) / 365. < x.native_year)
            })
            .cloned()
            .collect::<Vec<_>>();
        let before = fee::evaluate_with_check(
            &request(p, r, prefix),
            portfolio_compute_control::checkpoint,
        )?;
        if let Some(x) = &exit {
            let policy = match x.reason {
                fee::Reason::Default => &p.default,
                fee::Reason::Maturity => &p.maturity,
                fee::Reason::Exercise => &p.exercise,
                _ => &p.prepayment,
            };
            let day = ordinal(origin, x.native_year)?;
            for (fi, (f, c)) in p.fees.iter().zip(before.closing()).enumerate() {
                let s = &c.state;
                let sign = f.entitlement.signum();
                let mut emit = |suffix: &str,
                                action: fee::Action,
                                amount: f64,
                                year: f64,
                                reason: fee::Reason|
                 -> Result<(), String> {
                    if amount != 0. {
                        timeline.push((
                            year,
                            fee::Event {
                                event_id: format!("native-fee:{}:{}:{suffix}", path.path_key, fi),
                                fee_id: f.fee_id.clone(),
                                source: f.source.clone(),
                                review: policy.review.clone(),
                                ordinal: ordinal(origin, year)?,
                                sequence: 0,
                                action,
                                reason,
                                amount,
                            },
                        ));
                    }
                    Ok(())
                };
                if policy.cancel_unearned_entitlement {
                    emit(
                        "cancel",
                        fee::Action::CancelUnearned,
                        s.unearned_entitlement,
                        x.native_year,
                        x.reason,
                    )?;
                }
                let unpaid =
                    ((s.earned - s.waived_earned - s.net_service_cash) * sign).max(0.) * sign;
                let waived = unpaid * policy.waive_unpaid_earned_fraction;
                emit(
                    "waive",
                    fee::Action::WaiveUnpaidEarned,
                    waived,
                    x.native_year,
                    x.reason,
                )?;
                let prepaid =
                    ((s.net_service_cash - s.earned + s.waived_earned) * sign).max(0.) * sign;
                emit(
                    "refund",
                    fee::Action::RefundPrepaid,
                    prepaid * policy.refund_prepaid_fraction,
                    x.native_year,
                    fee::Reason::Settlement,
                )?;
                for (i, v) in policy.recoveries.iter().enumerate() {
                    emit(
                        &format!("recovery:{i}"),
                        fee::Action::RecoverWaived,
                        waived * v.fraction,
                        x.native_year + f64::from(v.lag_days) / 365.,
                        x.reason,
                    )?;
                }
                if day > p.complete_end_ordinal {
                    return Err("native loan exit exceeds separately complete fee tail".into());
                }
            }
            for e in &p.events {
                let year = f64::from(e.ordinal - origin) / 365.;
                if year >= x.native_year
                    && ((e.action == fee::Action::Earn && policy.cancel_future_service)
                        || (e.action == fee::Action::Cash && policy.cancel_future_cash))
                {
                    continue;
                }
                timeline.push((year, e.clone()));
            }
        } else {
            timeline.extend(
                p.events
                    .iter()
                    .map(|e| (f64::from(e.ordinal - origin) / 365., e.clone())),
            );
        }
        // Stable source order; generated exit actions were inserted first, by explicit policy.
        timeline.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut events = Vec::new();
        let mut years = Vec::new();
        for (i, (year, mut event)) in timeline.into_iter().enumerate() {
            event.sequence = i as u32;
            years.push(year);
            events.push(event);
        }
        let prepared = request(p, r, events);
        let fee_source =
            fee::evaluate_with_check(&prepared, portfolio_compute_control::checkpoint)?;
        out.push(BoundPath {
            path_key: path.path_key,
            position_id: r.source.current.position_id.clone(),
            scenario: r.source.current.scenario_id.clone(),
            native_years: years,
            source: fee_source,
            exit,
            native_credit_event_binding: true,
            loan_carrying_value_changed: false,
        });
    }
    Ok(out)
}
