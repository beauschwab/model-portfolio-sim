//! Exact event-time CTMC paths with caller-owned CRN identity and realized monetary rights.
use crate::cashflow::validate_ordinal;
use crate::credit::{CreditMeasure, CreditState, MAX_ROW_HAZARD};
use crate::credit_calibration::ObservationTime;
use crate::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "credit-path-1";
pub const MAX_PATHS: usize = 10_000;
pub const MAX_EVENTS: usize = 250_000;
pub const MAX_WORK: usize = 2_000_000;
pub const ACT365_SECONDS_PER_YEAR: f64 = 365.0 * 86_400.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryWeight {
    pub lag_years: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathInterval {
    pub end_year: f64,
    pub hazards_per_year: [[f64; 5]; 3],
    pub accrual_rates: [f64; 3],
    pub loss_given_default: [f64; 3],
    pub credit_conversion_factor: [f64; 3],
    pub interest_recovery_fraction: [f64; 3],
    pub complete_recovery_tails: [Vec<RecoveryWeight>; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BoundaryAction {
    Repayment {
        principal_fraction: [f64; 3],
        interest_fraction: [f64; 3],
        cancel_undrawn_fraction: [f64; 3],
    },
    Draw {
        undrawn_fraction: [f64; 3],
    },
    Maturity {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathBoundary {
    pub event_id: String,
    pub policy_id: String,
    pub year: f64,
    pub action: BoundaryAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathOpening {
    pub path_id: String,
    pub path_key: u64,
    pub initial_live_state: CreditState,
    pub principal: f64,
    pub undrawn_commitment: f64,
    pub accrued_interest: f64,
    pub book_adjustment: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniformPair {
    pub waiting_uniform: f64,
    pub destination_uniform: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedPathDraws {
    pub path_key: u64,
    pub pairs: Vec<UniformPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DrawStream {
    Supplied {
        stream_id: String,
        stream_revision: String,
        paths: Vec<SuppliedPathDraws>,
    },
    CounterSplitmix64V1 {
        stream_id: String,
        stream_revision: String,
        seed: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditPathRequest {
    pub schema: String,
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    pub source_id: String,
    pub source_revision: String,
    pub model_policy_id: String,
    pub origin_time: ObservationTime,
    pub max_jumps_per_path: usize,
    pub intervals: Vec<PathInterval>,
    pub boundaries: Vec<PathBoundary>,
    pub draw_stream: DrawStream,
    pub paths: Vec<PathOpening>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathOutcome {
    Performing,
    Watch,
    Delinquent,
    Default,
    Prepaid,
    Matured,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathEvent {
    pub year: f64,
    pub event_id: String,
    pub policy_id: String,
    pub origin: CreditState,
    pub destination: Option<CreditState>,
    pub kind: String,
    /// Positive means cash received; negative means funded draw or negative coupon settlement.
    pub cash: f64,
    pub principal: f64,
    pub interest: f64,
    pub book_adjustment_recognition: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRecovery {
    pub year: f64,
    pub principal: f64,
    pub interest: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathDefault {
    pub year: f64,
    pub origin: CreditState,
    pub principal: f64,
    pub accrued_interest: f64,
    pub book_adjustment: f64,
    pub contingent_draw: f64,
    pub economic_principal_loss: f64,
    pub recoveries: Vec<PathRecovery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditPathResult {
    pub path_id: String,
    pub path_key: u64,
    pub outcome: PathOutcome,
    pub closing_principal: f64,
    pub closing_undrawn_commitment: f64,
    pub closing_accrued_interest: f64,
    pub closing_book_adjustment: f64,
    pub earned_interest: f64,
    pub earned_interest_by_origin: [f64; 3],
    pub cash_received: f64,
    pub cash_funded: f64,
    pub book_adjustment_recognized: f64,
    pub random_pairs_used: usize,
    pub events: Vec<PathEvent>,
    pub default: Option<PathDefault>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditPathsResult {
    pub schema: String,
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    pub source_id: String,
    pub source_revision: String,
    pub model_policy_id: String,
    pub origin_time: ObservationTime,
    pub clock: String,
    pub stream_id: String,
    pub stream_revision: String,
    pub work_units: usize,
    pub paths: Vec<CreditPathResult>,
}

/// Exact economic live occupation, available only through the explicit traced API.
#[derive(Debug, Clone, Serialize)]
pub struct LiveSojourn {
    pub path_key: u64,
    pub start_year: f64,
    pub end_year: f64,
    pub origin: CreditState,
    pub principal: f64,
    pub undrawn_commitment: f64,
    pub opening_accrued_interest: f64,
    pub book_adjustment: f64,
    pub accrual_rate: f64,
    pub earned_interest: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreditPathTraceResult {
    pub result: CreditPathsResult,
    pub sojourns: Vec<LiveSojourn>,
}

fn label(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("identity/unit labels must contain 1..256 control-free bytes".into())
    } else {
        Ok(())
    }
}
fn range(v: f64, low: f64, high: f64) -> Result<(), String> {
    if !v.is_finite() || v < low || v > high {
        Err(format!("value must be finite in [{low},{high}]"))
    } else {
        Ok(())
    }
}
fn index(s: CreditState) -> Result<usize, String> {
    match s {
        CreditState::Performing => Ok(0),
        CreditState::Watch => Ok(1),
        CreditState::Delinquent => Ok(2),
        _ => Err("opening must be a live state".into()),
    }
}
fn state(i: usize) -> CreditState {
    [
        CreditState::Performing,
        CreditState::Watch,
        CreditState::Delinquent,
        CreditState::Default,
        CreditState::Prepaid,
    ][i]
}
fn mix(mut v: u64) -> u64 {
    v = v.wrapping_add(0x9e3779b97f4a7c15);
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d049bb133111eb);
    v ^ (v >> 31)
}
fn unit(v: u64) -> f64 {
    ((v >> 12) as f64 + 0.5) / 4_503_599_627_370_496.0
}
fn tick(work: &mut usize, n: usize) -> Result<(), String> {
    *work = work.checked_add(n).ok_or("work overflow")?;
    if *work > MAX_WORK {
        Err("credit path work limit exceeded".into())
    } else {
        Ok(())
    }
}
fn money(b: f64, u: f64, a: f64, k: f64) -> Result<(), String> {
    range(b, 0.0, MAX_MONETARY_AMOUNT)?;
    range(u, 0.0, MAX_MONETARY_AMOUNT)?;
    range(a, -MAX_MONETARY_AMOUNT, MAX_MONETARY_AMOUNT)?;
    range(k, -MAX_MONETARY_AMOUNT, MAX_MONETARY_AMOUNT)?;
    range(b + u + a.abs() + k.abs(), 0.0, MAX_MONETARY_AMOUNT)
}

pub fn evaluate(request: &CreditPathRequest) -> Result<CreditPathsResult, String> {
    Ok(evaluate_internal(request, false, None)?.result)
}

pub fn evaluate_traced(request: &CreditPathRequest) -> Result<CreditPathTraceResult, String> {
    evaluate_internal(request, true, None)
}

pub(crate) fn evaluate_income_internal(
    request: &CreditPathRequest,
    income: &mut crate::credit_path_income::Owner<'_>,
) -> Result<CreditPathTraceResult, String> {
    evaluate_internal(request, true, Some(income))
}

struct Validated<'a> {
    horizon: f64,
    draws: HashMap<u64, &'a Vec<UniformPair>>,
    stream_id: &'a String,
    stream_revision: &'a String,
}
/// Same domain validator used by actual execution; no path or money projection.
pub fn validate_request_with_check(
    request: &CreditPathRequest,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    check()?;
    validate_native(request)?;
    check()
}
fn validate_native(request: &CreditPathRequest) -> Result<Validated<'_>, String> {
    if request.schema != SCHEMA {
        return Err("unsupported credit path schema".into());
    }
    for s in [
        &request.monetary_unit,
        &request.source_id,
        &request.source_revision,
        &request.model_policy_id,
    ] {
        label(s)?;
    }
    validate_ordinal(request.origin_time.ordinal)?;
    if request.origin_time.seconds_of_day >= 86_400 {
        return Err("invalid origin seconds_of_day".into());
    }
    if request.paths.is_empty()
        || request.paths.len() > MAX_PATHS
        || request.intervals.is_empty()
        || request.intervals.len() > 4096
        || request.boundaries.len() > 4096
        || request.max_jumps_per_path == 0
        || request.max_jumps_per_path > MAX_EVENTS
        || request
            .paths
            .len()
            .checked_mul(request.max_jumps_per_path)
            .is_none_or(|n| n > MAX_EVENTS)
    {
        return Err("path/interval/boundary/jump admission limit exceeded".into());
    }
    let horizon = request.intervals.last().unwrap().end_year;
    let mut prev = 0.0;
    for term in &request.intervals {
        range(term.end_year, 0.0, 100.0)?;
        if term.end_year <= prev {
            return Err("interval end years must strictly increase".into());
        }
        prev = term.end_year;
        for i in 0..3 {
            let mut total = 0.0;
            for (j, h) in term.hazards_per_year[i].iter().enumerate() {
                range(*h, 0.0, MAX_ROW_HAZARD)?;
                if i == j && *h != 0.0 {
                    return Err("diagonal hazard must be zero".into());
                }
                total += h;
            }
            range(total, 0.0, MAX_ROW_HAZARD)?;
            range(term.accrual_rates[i], -100.0, 100.0)?;
            for v in [
                term.loss_given_default[i],
                term.credit_conversion_factor[i],
                term.interest_recovery_fraction[i],
            ] {
                range(v, 0.0, 1.0)?;
            }
            let tail = &term.complete_recovery_tails[i];
            if tail.is_empty() || tail.len() > 32 {
                return Err("each origin requires a complete 1..32-point recovery tail".into());
            }
            let mut lag = -1.0;
            let mut sum = 0.0;
            for point in tail {
                range(point.lag_years, 0.0, 100.0)?;
                range(point.weight, 0.0, 1.0)?;
                if point.lag_years <= lag {
                    return Err("recovery lags must strictly increase".into());
                }
                lag = point.lag_years;
                sum += point.weight;
            }
            if (sum - 1.0).abs() > 1.0e-12 || sum > 1.0 {
                return Err(
                    "complete recovery weights must sum to one without exceeding one".into(),
                );
            }
        }
    }
    let mut ids = HashSet::new();
    prev = -1.0;
    for boundary in &request.boundaries {
        label(&boundary.event_id)?;
        label(&boundary.policy_id)?;
        range(boundary.year, 0.0, horizon)?;
        if boundary.year < prev || !ids.insert(&boundary.event_id) {
            return Err("boundaries must be ordered with unique event IDs".into());
        }
        prev = boundary.year;
        match &boundary.action {
            BoundaryAction::Repayment {
                principal_fraction,
                interest_fraction,
                cancel_undrawn_fraction,
            } => {
                for v in principal_fraction
                    .iter()
                    .chain(interest_fraction)
                    .chain(cancel_undrawn_fraction)
                {
                    range(*v, 0.0, 1.0)?;
                }
            }
            BoundaryAction::Draw { undrawn_fraction } => {
                for v in undrawn_fraction {
                    range(*v, 0.0, 1.0)?;
                }
            }
            BoundaryAction::Maturity {} => (),
        }
    }
    ids.clear();
    let mut keys = HashSet::new();
    for path in &request.paths {
        label(&path.path_id)?;
        index(path.initial_live_state)?;
        money(
            path.principal,
            path.undrawn_commitment,
            path.accrued_interest,
            path.book_adjustment,
        )?;
        if !ids.insert(&path.path_id) || !keys.insert(path.path_key) {
            return Err("path IDs and CRN path keys must be unique".into());
        }
    }
    let mut draws = HashMap::new();
    let (stream_id, stream_revision) = match &request.draw_stream {
        DrawStream::Supplied {
            stream_id,
            stream_revision,
            paths,
        } => {
            if paths.len() != request.paths.len() {
                return Err("supplied draw paths require exact path-key coverage".into());
            }
            let mut count = 0usize;
            for path in paths {
                if !keys.contains(&path.path_key)
                    || draws.insert(path.path_key, &path.pairs).is_some()
                {
                    return Err("duplicate or unknown supplied draw path key".into());
                }
                count = count
                    .checked_add(path.pairs.len())
                    .ok_or("draw count overflow")?;
                for pair in &path.pairs {
                    for u in [pair.waiting_uniform, pair.destination_uniform] {
                        if !u.is_finite() || u <= 0.0 || u >= 1.0 {
                            return Err(
                                "uniform draws must be strictly between zero and one".into()
                            );
                        }
                    }
                }
            }
            if count > MAX_EVENTS + MAX_PATHS {
                return Err("supplied draw admission exceeded".into());
            }
            (stream_id, stream_revision)
        }
        DrawStream::CounterSplitmix64V1 {
            stream_id,
            stream_revision,
            ..
        } => (stream_id, stream_revision),
    };
    label(stream_id)?;
    label(stream_revision)?;
    Ok(Validated {
        horizon,
        draws,
        stream_id,
        stream_revision,
    })
}

fn evaluate_internal(
    request: &CreditPathRequest,
    retain_trace: bool,
    mut income: Option<&mut crate::credit_path_income::Owner<'_>>,
) -> Result<CreditPathTraceResult, String> {
    let Validated {
        horizon,
        draws,
        stream_id,
        stream_revision,
    } = validate_native(request)?;
    let mut work = 0;
    let mut event_count = 0;
    let mut sojourns = Vec::new();
    let mut paths = Vec::with_capacity(request.paths.len());
    for opening in &request.paths {
        if let Some(owner) = income.as_deref_mut() {
            owner.begin(opening)?;
        }
        let mut live = index(opening.initial_live_state)?;
        let mut result = CreditPathResult {
            path_id: opening.path_id.clone(),
            path_key: opening.path_key,
            outcome: [
                PathOutcome::Performing,
                PathOutcome::Watch,
                PathOutcome::Delinquent,
            ][live],
            closing_principal: opening.principal,
            closing_undrawn_commitment: opening.undrawn_commitment,
            closing_accrued_interest: opening.accrued_interest,
            closing_book_adjustment: opening.book_adjustment,
            earned_interest: 0.0,
            earned_interest_by_origin: [0.0; 3],
            cash_received: 0.0,
            cash_funded: 0.0,
            book_adjustment_recognized: 0.0,
            random_pairs_used: 0,
            events: Vec::new(),
            default: None,
        };
        let mut t = 0.0;
        let mut interval_index = 0;
        let mut boundary_index = 0;
        let mut waiting: Option<(f64, f64)> = None;
        let mut jumps = 0;
        while t < horizon
            || boundary_index < request.boundaries.len()
                && request.boundaries[boundary_index].year == t
        {
            tick(&mut work, 1)?;
            if let Some(owner) = income.as_deref_mut() {
                owner.check()?;
            }
            let term = &request.intervals[interval_index];
            let boundary_time = request
                .boundaries
                .get(boundary_index)
                .map_or(horizon, |x| x.year);
            let end = term.end_year.min(boundary_time);
            let total: f64 = term.hazards_per_year[live].iter().sum();
            if total > 0.0 && waiting.is_none() {
                let n = result.random_pairs_used;
                let pair = match &request.draw_stream {
                    DrawStream::Supplied { .. } => *draws[&opening.path_key]
                        .get(n)
                        .ok_or("supplied uniform pairs exhausted; no random fallback")?,
                    DrawStream::CounterSplitmix64V1 { seed, .. } => {
                        // Domain order matters: swapping path key and jump index must not
                        // produce the same counter as a commutative XOR of both mixes.
                        let key = mix(seed ^ mix(opening.path_key ^ 0xa0761d6478bd642f))
                            .wrapping_add((n as u64).wrapping_mul(0xe7037ed1a0b428db));
                        UniformPair {
                            waiting_uniform: unit(mix(key)),
                            destination_uniform: unit(mix(key ^ 0xd2b74407b1ce6e93)),
                        }
                    }
                };
                result.random_pairs_used += 1;
                waiting = Some((-pair.waiting_uniform.ln(), pair.destination_uniform));
            }
            let capacity = total * (end - t);
            let jump = waiting.filter(|(e, _)| total > 0.0 && *e <= capacity);
            let next = jump.map_or(end, |(e, _)| (t + e / total).min(end));
            let interest = if let Some(owner) = income.as_deref_mut() {
                owner.contractual_earned(
                    interval_index,
                    live,
                    t,
                    next,
                    result.closing_principal,
                    term.accrual_rates[live],
                )?
            } else {
                result.closing_principal * term.accrual_rates[live] * (next - t)
            };
            if retain_trace && next > t {
                if sojourns.len() >= MAX_EVENTS {
                    return Err("retained sojourn admission exceeded".into());
                }
                sojourns.push(LiveSojourn {
                    path_key: opening.path_key,
                    start_year: t,
                    end_year: next,
                    origin: state(live),
                    principal: result.closing_principal,
                    undrawn_commitment: result.closing_undrawn_commitment,
                    opening_accrued_interest: result.closing_accrued_interest,
                    book_adjustment: result.closing_book_adjustment,
                    accrual_rate: term.accrual_rates[live],
                    earned_interest: interest,
                });
            }
            result.earned_interest += interest;
            result.earned_interest_by_origin[live] += interest;
            result.closing_accrued_interest += interest;
            if let Some(owner) = income.as_deref_mut() {
                owner.advance(&mut result, interval_index, live, t, next)?;
            }
            t = next;
            money(
                result.closing_principal,
                result.closing_undrawn_commitment,
                result.closing_accrued_interest,
                result.closing_book_adjustment,
            )?;
            if let Some((_, u)) = jump {
                jumps += 1;
                if jumps > request.max_jumps_per_path {
                    return Err("maximum jumps per path exceeded".into());
                }
                let target = u * total;
                let mut cumulative = 0.0;
                let destination = term.hazards_per_year[live]
                    .iter()
                    .enumerate()
                    .find_map(|(j, h)| {
                        cumulative += h;
                        if target < cumulative {
                            Some(j)
                        } else {
                            None
                        }
                    })
                    .ok_or("categorical transition could not be resolved")?;
                let mut event = PathEvent {
                    year: t,
                    event_id: format!("jump:{jumps}"),
                    policy_id: request.model_policy_id.clone(),
                    origin: state(live),
                    destination: Some(state(destination)),
                    kind: "transition".into(),
                    cash: 0.0,
                    principal: 0.0,
                    interest: 0.0,
                    book_adjustment_recognition: 0.0,
                };
                waiting = None;
                if destination == 3 {
                    let draw =
                        result.closing_undrawn_commitment * term.credit_conversion_factor[live];
                    result.closing_principal += draw;
                    result.closing_undrawn_commitment = 0.0;
                    result.cash_funded += draw;
                    event.cash = -draw;
                    event.principal = result.closing_principal;
                    event.interest = result.closing_accrued_interest;
                    if let Some(owner) = income.as_deref_mut() {
                        owner.fund(&mut result, t, draw)?;
                        owner.default_begin(&result)?;
                    }
                    let mut claim = PathDefault {
                        year: t,
                        origin: state(live),
                        principal: result.closing_principal,
                        accrued_interest: result.closing_accrued_interest,
                        book_adjustment: result.closing_book_adjustment,
                        contingent_draw: draw,
                        economic_principal_loss: result.closing_principal
                            * term.loss_given_default[live],
                        recoveries: Vec::new(),
                    };
                    let mut applied_weight = 0.0;
                    for (recovery_index, point) in
                        term.complete_recovery_tails[live].iter().enumerate()
                    {
                        tick(&mut work, 1)?;
                        if let Some(owner) = income.as_deref_mut() {
                            owner.check()?;
                        }
                        let interest = claim.accrued_interest
                            * term.interest_recovery_fraction[live]
                            * point.weight;
                        let interest = if let Some(owner) = income.as_deref_mut() {
                            owner.recovery_interest(
                                crate::credit_path_income::NativeRecoveryQuery {
                                    path_key: result.path_key,
                                    default_year: t,
                                    recovery_index,
                                    year: t + point.lag_years,
                                    fraction: term.interest_recovery_fraction[live],
                                    weight: point.weight,
                                    fallback: interest,
                                    horizon,
                                },
                            )?
                        } else {
                            interest
                        };
                        let recovery = PathRecovery {
                            year: t + point.lag_years,
                            principal: claim.principal
                                * (1.0 - term.loss_given_default[live])
                                * point.weight,
                            interest,
                        };
                        if recovery.year <= horizon {
                            applied_weight += point.weight;
                            result.cash_received += recovery.principal + recovery.interest;
                        }
                        claim.recoveries.push(recovery);
                    }
                    result.closing_principal = claim.principal
                        * (term.loss_given_default[live]
                            + (1.0 - term.loss_given_default[live]) * (1.0 - applied_weight));
                    result.closing_accrued_interest = claim.accrued_interest
                        * (1.0 - term.interest_recovery_fraction[live] * applied_weight);
                    if let Some(owner) = income.as_deref_mut() {
                        owner.default_tail(&mut result, &claim, horizon)?;
                    }
                    result.default = Some(claim);
                    result.outcome = PathOutcome::Default;
                } else if destination == 4 {
                    event.kind = "prepayment".into();
                    event.principal = result.closing_principal;
                    event.interest = result.closing_accrued_interest;
                    event.cash = event.principal + event.interest;
                    event.book_adjustment_recognition = result.closing_book_adjustment;
                    if let Some(owner) = income.as_deref_mut() {
                        if let Some((cash, basis)) =
                            owner.native_exit(&result, &event.event_id, t, true)?
                        {
                            event.cash = cash;
                            event.book_adjustment_recognition = basis;
                        }
                    }
                    result.cash_received += event.cash;
                    result.book_adjustment_recognized += event.book_adjustment_recognition;
                    result.closing_principal = 0.0;
                    result.closing_accrued_interest = 0.0;
                    result.closing_book_adjustment = 0.0;
                    result.closing_undrawn_commitment = 0.0;
                    result.outcome = PathOutcome::Prepaid;
                    if let Some(owner) = income.as_deref_mut() {
                        owner.exit(&result, t, event.cash, event.book_adjustment_recognition)?;
                    }
                } else {
                    live = destination;
                    result.outcome = [
                        PathOutcome::Performing,
                        PathOutcome::Watch,
                        PathOutcome::Delinquent,
                    ][live];
                }
                result.events.push(event);
                event_count += 1;
                if destination >= 3 {
                    break;
                }
            } else {
                if let Some((remaining, u)) = waiting {
                    waiting = Some((remaining - capacity, u));
                }
                while let Some(boundary) = request
                    .boundaries
                    .get(boundary_index)
                    .filter(|x| x.year == t)
                {
                    tick(&mut work, 1)?;
                    if let Some(owner) = income.as_deref_mut() {
                        owner.check()?;
                        if !owner.boundary_applies(result.path_key, &boundary.event_id)? {
                            boundary_index += 1;
                            continue;
                        }
                    }
                    let mut event = PathEvent {
                        year: t,
                        event_id: boundary.event_id.clone(),
                        policy_id: boundary.policy_id.clone(),
                        origin: state(live),
                        destination: None,
                        kind: "repayment".into(),
                        cash: 0.0,
                        principal: 0.0,
                        interest: 0.0,
                        book_adjustment_recognition: 0.0,
                    };
                    match &boundary.action {
                        BoundaryAction::Repayment {
                            principal_fraction,
                            interest_fraction,
                            cancel_undrawn_fraction,
                        } => {
                            let source_cash = if let Some(owner) = income.as_deref_mut() {
                                owner.source_repayment(
                                    &boundary.event_id,
                                    t,
                                    result.closing_principal,
                                )?
                            } else {
                                None
                            };
                            let mut fee = 0.;
                            if let Some((principal, interest, native_fee)) = source_cash {
                                event.principal = principal;
                                event.interest = interest;
                                fee = native_fee;
                            } else {
                                event.principal =
                                    result.closing_principal * principal_fraction[live];
                                event.interest =
                                    result.closing_accrued_interest * interest_fraction[live];
                            }
                            event.book_adjustment_recognition = if income.is_some() {
                                0.0
                            } else {
                                result.closing_book_adjustment * principal_fraction[live]
                            };
                            result.closing_principal -= event.principal;
                            result.closing_accrued_interest -= event.interest;
                            result.closing_book_adjustment -= event.book_adjustment_recognition;
                            result.closing_undrawn_commitment *=
                                1.0 - cancel_undrawn_fraction[live];
                            event.cash = event.principal + event.interest;
                            if fee != 0. {
                                event.cash += fee;
                            }
                            result.cash_received += event.cash;
                            result.book_adjustment_recognized += event.book_adjustment_recognition;
                            if let Some(owner) = income.as_deref_mut() {
                                owner.settle(&mut result, t, event.cash)?;
                            }
                        }
                        BoundaryAction::Draw { undrawn_fraction } => {
                            event.kind = "draw".into();
                            let native = if let Some(owner) = income.as_deref_mut() {
                                owner.native_draw(&result, &boundary.event_id, t, live)?
                            } else {
                                None
                            };
                            let cost = match native {
                                Some(crate::credit_path_income::NativeDrawResolution::Skip) => {
                                    boundary_index += 1;
                                    continue;
                                }
                                Some(crate::credit_path_income::NativeDrawResolution::Fund {
                                    principal,
                                    cost,
                                    ..
                                }) => {
                                    event.principal = principal;
                                    cost
                                }
                                None => {
                                    event.principal =
                                        result.closing_undrawn_commitment * undrawn_fraction[live];
                                    event.principal
                                }
                            };
                            result.closing_principal += event.principal;
                            result.closing_undrawn_commitment -= event.principal;
                            event.cash = -cost;
                            result.cash_funded += cost;
                            if let Some(owner) = income.as_deref_mut() {
                                owner.fund(&mut result, t, cost)?;
                            }
                        }
                        BoundaryAction::Maturity {} => {
                            event.kind = "maturity".into();
                            event.principal = result.closing_principal;
                            event.interest = result.closing_accrued_interest;
                            event.cash = event.principal + event.interest;
                            event.book_adjustment_recognition = result.closing_book_adjustment;
                            if let Some(owner) = income.as_deref_mut() {
                                if let Some((cash, basis)) =
                                    owner.native_exit(&result, &event.event_id, t, false)?
                                {
                                    event.cash = cash;
                                    event.book_adjustment_recognition = basis;
                                }
                            }
                            result.cash_received += event.cash;
                            result.book_adjustment_recognized += event.book_adjustment_recognition;
                            result.closing_principal = 0.0;
                            result.closing_accrued_interest = 0.0;
                            result.closing_undrawn_commitment = 0.0;
                            result.closing_book_adjustment = 0.0;
                            result.outcome = PathOutcome::Matured;
                            if let Some(owner) = income.as_deref_mut() {
                                owner.exit(
                                    &result,
                                    t,
                                    event.cash,
                                    event.book_adjustment_recognition,
                                )?;
                            }
                        }
                    }
                    result.events.push(event);
                    event_count += 1;
                    boundary_index += 1;
                    if result.outcome == PathOutcome::Matured {
                        break;
                    }
                }
                if result.outcome == PathOutcome::Matured {
                    break;
                }
                if t == term.end_year && interval_index + 1 < request.intervals.len() {
                    interval_index += 1;
                }
                if t == horizon {
                    break;
                }
            }
            if event_count > MAX_EVENTS {
                return Err("retained event limit exceeded".into());
            }
        }
        if event_count > MAX_EVENTS {
            return Err("retained event limit exceeded".into());
        }
        money(
            result.closing_principal,
            result.closing_undrawn_commitment,
            result.closing_accrued_interest,
            result.closing_book_adjustment,
        )?;
        for v in [
            result.earned_interest,
            result.cash_received,
            result.cash_funded,
            result.book_adjustment_recognized,
        ] {
            range(v, -MAX_MONETARY_AMOUNT, MAX_MONETARY_AMOUNT)?;
        }
        for v in result.earned_interest_by_origin {
            range(v, -MAX_MONETARY_AMOUNT, MAX_MONETARY_AMOUNT)?;
        }
        if let Some(owner) = income.as_deref_mut() {
            owner.finish(&result, horizon)?;
        }
        paths.push(result);
    }
    let result = CreditPathsResult {
        schema: SCHEMA.into(),
        measure: request.measure,
        monetary_unit: request.monetary_unit.clone(),
        source_id: request.source_id.clone(),
        source_revision: request.source_revision.clone(),
        model_policy_id: request.model_policy_id.clone(),
        origin_time: request.origin_time,
        clock: "act_365_elapsed_seconds".into(),
        stream_id: stream_id.clone(),
        stream_revision: stream_revision.clone(),
        work_units: work,
        paths,
    };
    Ok(CreditPathTraceResult { result, sojourns })
}
