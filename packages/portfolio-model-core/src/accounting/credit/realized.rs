//! Bounded realized path adoption into explicit reviewed daily credit accounting.
use crate::accounting::credit::expected::{
    self as accounting, AccountCode, AccountingPolicy, AccountingState, DefaultInstruction,
    ExcessCashRecovery, Identity, InterestRecoveryRequest, JournalIntent, LiveState,
    OriginDefaultDelta, PostingIntent, RecoveryPoint, ReviewedAllowance, ReviewedChargeoff,
    ReviewedInterestRecovery,
};
use crate::cashflows::dated::{validate_ordinal, CashflowMeasure, MAX_HORIZON_DAYS};
use crate::models::credit::paths::{
    self as credit_path, CreditPathRequest, CreditPathResult, LiveSojourn, ACT365_SECONDS_PER_YEAR,
};
use crate::models::credit::transition::{CreditMeasure, CreditState};
use crate::validation::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SCHEMA: &str = "credit-path-accounting-1";
pub const MAX_PATHS: usize = 256;
pub const MAX_ROWS: usize = 250_000;
pub const MAX_INTENTS: usize = 250_000;
pub const MAX_WORK: usize = 2_000_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayMapping {
    CalendarMidnightRightClosed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathBinding {
    pub path_key: u64,
    pub position_id: String,
    pub currency: String,
    pub claim_id: String,
    pub economic_source: Identity,
    pub recovery_schedule_review: Identity,
    pub policy: AccountingPolicy,
    pub interest_recovery_policy: Identity,
    pub interest_recovery_excess: ExcessCashRecovery,
    pub opening_allowance: f64,
    pub allowance_reviews: Vec<ReviewedAllowance>,
    pub chargeoffs: Vec<ReviewedChargeoff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathAccountingRequest {
    pub schema: String,
    pub path: CreditPathRequest,
    pub day_mapping: DayMapping,
    pub reporting_end_ordinal: i32,
    /// Explicit numerical calendar tie tolerance; exact raw economic offsets are retained.
    pub timestamp_tolerance_seconds: f64,
    pub bindings: Vec<PathBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExactTime {
    pub elapsed_year: f64,
    pub elapsed_seconds: f64,
    pub ordinal: i32,
    pub seconds_of_day: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct MappedRecovery {
    pub recovery_id: Identity,
    pub exact_time: ExactTime,
    pub recognition_ordinal: i32,
    pub principal: f64,
    pub interest: f64,
    pub after_reporting: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct TimedIntent {
    pub intent: JournalIntent,
    pub exact_event: Option<ExactTime>,
    /// Complete origin event IDs, or underlying coalesced recovery IDs, retained independently.
    pub event_ids: Vec<String>,
    pub timing: String,
    /// Contractual policy name resolved within the immutable economic source, without inventing a revision.
    pub contractual_policy_id: Option<String>,
    pub accrual: Option<AccrualSlice>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AccrualSlice {
    pub start_year: f64,
    pub end_year: f64,
    pub origin: CreditState,
    pub principal: f64,
    pub accrual_rate: f64,
    pub earned_interest: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DailyPathRow {
    pub recognition_ordinal: i32,
    pub probability: [f64; 7],
    pub matured_probability: f64,
    pub live: [LiveState; 3],
    pub default_principal: f64,
    pub default_accrued_interest: f64,
    pub default_book_adjustment: f64,
    pub allowance: f64,
    pub earned_interest: f64,
    pub cash_delta: f64,
    pub profit_delta: f64,
    pub cumulative_cash: f64,
    pub cumulative_profit: f64,
    pub net_assets_including_cash: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct AccountedPath {
    pub binding: PathBinding,
    pub economic: CreditPathResult,
    pub daily: Vec<DailyPathRow>,
    pub intents: Vec<TimedIntent>,
    pub complete_recoveries: Vec<MappedRecovery>,
    pub closing: AccountingState,
}
#[derive(Debug, Clone, Serialize)]
pub struct PathAccountingResult {
    pub schema: String,
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    pub source_id: String,
    pub source_revision: String,
    pub day_mapping: DayMapping,
    pub reporting_end_ordinal: i32,
    pub work_units: usize,
    pub paths: Vec<AccountedPath>,
}

fn amount(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONETARY_AMOUNT {
        Err("path accounting finite currency amount admission exceeded".into())
    } else {
        Ok(())
    }
}
fn label(x: &str, bytes: &mut usize) -> Result<(), String> {
    if x.is_empty() || x.len() > 256 || x.chars().any(char::is_control) {
        return Err("path accounting labels need 1..256 control-free bytes".into());
    }
    *bytes = bytes.checked_add(x.len()).ok_or("metadata overflow")?;
    if *bytes > MAX_DATED_METADATA_BYTES {
        Err("path accounting metadata admission exceeded".into())
    } else {
        Ok(())
    }
}
fn identity(x: &Identity, bytes: &mut usize) -> Result<(), String> {
    label(&x.id, bytes)?;
    label(&x.revision, bytes)
}
fn live_index(s: CreditState) -> Result<usize, String> {
    match s {
        CreditState::Performing => Ok(0),
        CreditState::Watch => Ok(1),
        CreditState::Delinquent => Ok(2),
        _ => Err("expected live realized origin".into()),
    }
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if (a - b).abs() > 1e-9_f64.max(128.0 * f64::EPSILON * a.abs().max(b.abs())) {
        Err(format!(
            "realized path accounting conservation failed: {a} != {b}"
        ))
    } else {
        Ok(())
    }
}
fn tick(work: &mut usize, n: usize) -> Result<(), String> {
    *work = work.checked_add(n).ok_or("path accounting work overflow")?;
    if *work > MAX_WORK {
        Err("path accounting work admission exceeded".into())
    } else {
        Ok(())
    }
}

fn clock(request: &PathAccountingRequest, year: f64) -> Result<(ExactTime, i32), String> {
    if !year.is_finite() || !(0.0..=200.0).contains(&year) {
        return Err("invalid exact path event time".into());
    }
    let elapsed = year * ACT365_SECONDS_PER_YEAR;
    let absolute = f64::from(request.path.origin_time.seconds_of_day) + elapsed;
    let mut day = (absolute / 86400.0).floor() as i32;
    let mut seconds = absolute - f64::from(day) * 86400.0;
    if seconds <= request.timestamp_tolerance_seconds {
        seconds = 0.0;
    } else if 86400.0 - seconds <= request.timestamp_tolerance_seconds {
        day = day.checked_add(1).ok_or("calendar overflow")?;
        seconds = 0.0;
    }
    let ordinal = request
        .path
        .origin_time
        .ordinal
        .checked_add(day)
        .ok_or("calendar overflow")?;
    validate_ordinal(ordinal)?;
    let closing = if seconds == 0.0 {
        ordinal
    } else {
        ordinal.checked_add(1).ok_or("calendar overflow")?
    };
    validate_ordinal(closing)?;
    Ok((
        ExactTime {
            elapsed_year: year,
            elapsed_seconds: elapsed,
            ordinal,
            seconds_of_day: seconds,
        },
        closing,
    ))
}
fn day_end_year(request: &PathAccountingRequest, day: i32) -> f64 {
    (f64::from(day - request.path.origin_time.ordinal) * 86400.0
        - f64::from(request.path.origin_time.seconds_of_day))
        / ACT365_SECONDS_PER_YEAR
}

#[derive(Clone)]
enum Operation {
    Accrual {
        year: f64,
        origin: usize,
        amount: f64,
        slice: AccrualSlice,
    },
    Event {
        year: f64,
        index: usize,
    },
}
impl Operation {
    fn year(&self) -> f64 {
        match self {
            Self::Accrual { year, .. } | Self::Event { year, .. } => *year,
        }
    }
    fn order(&self) -> (usize, usize) {
        match self {
            Self::Accrual { .. } => (0, 0),
            Self::Event { index, .. } => (1, *index),
        }
    }
}
fn operations(
    request: &PathAccountingRequest,
    path: &CreditPathResult,
    sojourns: &[&LiveSojourn],
    work: &mut usize,
) -> Result<BTreeMap<i32, Vec<Operation>>, String> {
    let mut result: BTreeMap<i32, Vec<Operation>> = BTreeMap::new();
    let mut total = 0usize;
    for sojourn in sojourns {
        let origin = live_index(sojourn.origin)?;
        let mut start = sojourn.start_year;
        let mut earned = 0.0;
        while start < sojourn.end_year {
            tick(work, 1)?;
            let (_, mut day) = clock(request, start)?;
            if day_end_year(request, day) <= start {
                day += 1;
            }
            let end = sojourn.end_year.min(day_end_year(request, day));
            if end <= start {
                return Err("daily accrual clock failed to advance".into());
            }
            let interest = sojourn.principal * sojourn.accrual_rate * (end - start);
            amount(interest)?;
            earned += interest;
            result.entry(day).or_default().push(Operation::Accrual {
                year: end,
                origin,
                amount: interest,
                slice: AccrualSlice {
                    start_year: start,
                    end_year: end,
                    origin: sojourn.origin,
                    principal: sojourn.principal,
                    accrual_rate: sojourn.accrual_rate,
                    earned_interest: interest,
                },
            });
            start = end;
            total += 1;
            if total > MAX_ROWS {
                return Err("path daily operation admission exceeded".into());
            }
        }
        near(earned, sojourn.earned_interest)?;
    }
    for (index, event) in path.events.iter().enumerate() {
        let (_, day) = clock(request, event.year)?;
        if day <= request.path.origin_time.ordinal || day > request.reporting_end_ordinal {
            return Err("opening-instant or out-of-report path events require another explicit accounting window".into());
        }
        result.entry(day).or_default().push(Operation::Event {
            year: event.year,
            index,
        });
        total += 1;
        if total > MAX_ROWS {
            return Err("path daily operation admission exceeded".into());
        }
    }
    for rows in result.values_mut() {
        rows.sort_by(|a, b| {
            a.year()
                .total_cmp(&b.year())
                .then(a.order().cmp(&b.order()))
        });
    }
    Ok(result)
}
// One journal transaction updates the conservation book and both global admission counters.
#[allow(clippy::too_many_arguments)]
fn append(
    intents: &mut Vec<TimedIntent>,
    intent: JournalIntent,
    time: Option<ExactTime>,
    ids: Vec<String>,
    timing: &str,
    gl: &mut [f64; 12],
    metadata: &mut usize,
    global_count: &mut usize,
) -> Result<(), String> {
    *global_count += 1;
    if *global_count > MAX_INTENTS {
        return Err("path journal intent admission exceeded".into());
    }
    for s in [
        &intent.event,
        &intent.position_id,
        &intent.policy.id,
        &intent.policy.revision,
        &intent.source.id,
        &intent.source.revision,
    ] {
        label(s, metadata)?;
    }
    for id in &ids {
        label(id, metadata)?;
    }
    let mut sum = 0.0;
    let mut scale = 0.0;
    for posting in &intent.postings {
        amount(posting.amount)?;
        gl[posting.account.numeric_code() as usize - 1] += posting.amount;
        amount(gl[posting.account.numeric_code() as usize - 1])?;
        sum += posting.amount;
        scale += posting.amount.abs();
    }
    if sum.abs() > 1e-9_f64.max(128.0 * f64::EPSILON * scale) {
        return Err("realized accounting intent is unbalanced".into());
    }
    intents.push(TimedIntent {
        intent,
        exact_event: time,
        event_ids: ids,
        timing: timing.into(),
        contractual_policy_id: None,
        accrual: None,
    });
    Ok(())
}
fn economic_intent(
    binding: &PathBinding,
    day: i32,
    event: &str,
    postings: &[(AccountCode, f64)],
) -> JournalIntent {
    JournalIntent {
        recognition_ordinal: day,
        event: event.into(),
        position_id: binding.position_id.clone(),
        claim_id: None,
        policy: binding.policy.identity.clone(),
        source: binding.economic_source.clone(),
        postings: postings
            .iter()
            .filter(|(_, amount)| *amount != 0.0)
            .map(|(account, amount)| PostingIntent {
                account: *account,
                amount: *amount,
            })
            .collect(),
    }
}
fn summed_state(state: &AccountingState) -> [f64; 8] {
    [
        state.live.iter().map(|s| s.principal).sum(),
        state.claims.iter().map(|c| c.outstanding_principal).sum(),
        state.live.iter().map(|s| s.accrued_interest).sum(),
        state
            .claims
            .iter()
            .map(|c| c.outstanding_accrued_interest)
            .sum(),
        state.live.iter().map(|s| s.book_adjustment).sum(),
        state
            .claims
            .iter()
            .map(|c| c.outstanding_book_adjustment)
            .sum(),
        -state.allowance,
        state.live.iter().map(|s| s.undrawn).sum(),
    ]
}

pub fn evaluate(request: &PathAccountingRequest) -> Result<PathAccountingResult, String> {
    if request.schema != SCHEMA
        || request.path.measure != CreditMeasure::Physical
        || request.path.monetary_unit != "currency_units"
    {
        return Err(
            "realized daily accounting requires explicit physical measure and currency_units"
                .into(),
        );
    }
    validate_ordinal(request.reporting_end_ordinal)?;
    amount(request.timestamp_tolerance_seconds)?;
    if request.timestamp_tolerance_seconds < 0.0 || request.timestamp_tolerance_seconds > 1e-6 {
        return Err("calendar tie tolerance must be in 0..1 microsecond".into());
    }
    let days =
        i64::from(request.reporting_end_ordinal) - i64::from(request.path.origin_time.ordinal);
    if days < 1
        || days > i64::from(MAX_HORIZON_DAYS)
        || request.path.paths.is_empty()
        || request.path.paths.len() > MAX_PATHS
        || request.bindings.len() != request.path.paths.len()
        || (days as usize)
            .checked_mul(request.bindings.len())
            .is_none_or(|n| n > MAX_ROWS)
    {
        return Err("realized daily path/row/horizon admission exceeded".into());
    }
    let horizon = request
        .path
        .intervals
        .last()
        .ok_or("path intervals are required")?
        .end_year;
    if request.path.intervals.len() > 4096 || request.path.boundaries.len() > 4096 {
        return Err("interval/boundary admission exceeded".into());
    }
    let mut work = 0;
    for term in &request.path.intervals {
        for tail in &term.complete_recovery_tails {
            if tail.len() > 32 {
                return Err("complete recovery tail admission exceeded".into());
            }
            tick(&mut work, tail.len())?;
        }
    }
    let expected = day_end_year(request, request.reporting_end_ordinal);
    if (horizon - expected).abs() * ACT365_SECONDS_PER_YEAR
        > request.timestamp_tolerance_seconds.max(1e-9)
    {
        return Err("path horizon must equal the explicit reporting midnight".into());
    }
    let mut metadata = 0;
    let mut bindings = HashMap::new();
    let mut positions = HashSet::new();
    let mut claims = HashSet::new();
    for b in &request.bindings {
        label(&b.position_id, &mut metadata)?;
        label(&b.claim_id, &mut metadata)?;
        identity(&b.economic_source, &mut metadata)?;
        identity(&b.recovery_schedule_review, &mut metadata)?;
        identity(&b.policy.identity, &mut metadata)?;
        identity(&b.interest_recovery_policy, &mut metadata)?;
        if b.currency.len() != 3
            || !b.currency.bytes().all(|x| x.is_ascii_uppercase())
            || b.economic_source.id != request.path.source_id
            || b.economic_source.revision != request.path.source_revision
            || !positions.insert(&b.position_id)
            || !claims.insert(&b.claim_id)
            || bindings.insert(b.path_key, b).is_some()
        {
            return Err("binding source/currency/path/position/claim identities are incompatible or duplicate".into());
        }
        amount(b.opening_allowance)?;
        if b.opening_allowance < 0.0
            || b.allowance_reviews.len() > 4096
            || b.chargeoffs.len() > 4096
        {
            return Err("invalid opening allowance/review admission".into());
        }
        let mut reviewed = HashSet::new();
        for r in &b.allowance_reviews {
            identity(&r.review, &mut metadata)?;
            if r.recognition_ordinal <= request.path.origin_time.ordinal
                || r.recognition_ordinal > request.reporting_end_ordinal
                || !reviewed.insert(r.recognition_ordinal)
            {
                return Err("allowance reviews need unique in-report dates".into());
            }
        }
        for r in &b.chargeoffs {
            identity(&r.review, &mut metadata)?;
            if r.claim_id != b.claim_id
                || r.recognition_ordinal <= request.path.origin_time.ordinal
                || r.recognition_ordinal > request.reporting_end_ordinal
            {
                return Err("chargeoff review names another claim or date outside report".into());
            }
        }
    }
    let trace = credit_path::evaluate_traced(&request.path)?;
    tick(&mut work, trace.result.work_units)?;
    let recovery_records: usize = trace
        .result
        .paths
        .iter()
        .filter_map(|p| p.default.as_ref())
        .map(|d| d.recoveries.len())
        .sum();
    if recovery_records > 32 * MAX_PATHS {
        return Err("complete mapped recovery record admission exceeded".into());
    }
    tick(&mut work, recovery_records)?;
    for economic in &trace.result.paths {
        let binding = bindings
            .get(&economic.path_key)
            .ok_or("binding requires exact path-key coverage")?;
        if let Some(default) = &economic.default {
            for (index, recovery) in default.recoveries.iter().enumerate() {
                clock(request, recovery.year)?;
                amount(recovery.principal)?;
                amount(recovery.interest)?;
                identity(
                    &Identity {
                        id: format!("{}:recovery:{index}", binding.claim_id),
                        revision: binding.recovery_schedule_review.revision.clone(),
                    },
                    &mut metadata,
                )?;
            }
        }
    }
    let mut sojourn_index: HashMap<u64, Vec<&LiveSojourn>> = HashMap::new();
    for sojourn in &trace.sojourns {
        tick(&mut work, 1)?;
        sojourn_index
            .entry(sojourn.path_key)
            .or_default()
            .push(sojourn);
    }
    let boundary_index: HashMap<_, _> = request
        .path
        .boundaries
        .iter()
        .map(|b| (b.event_id.as_str(), b))
        .collect();
    let mut intent_count = 0;
    let mut paths = Vec::with_capacity(request.bindings.len());
    for (path_index, economic) in trace.result.paths.into_iter().enumerate() {
        let binding = *bindings
            .get(&economic.path_key)
            .ok_or("binding requires exact path-key coverage")?;
        let opening = &request.path.paths[path_index];
        let origin = live_index(opening.initial_live_state)?;
        let sojourns = sojourn_index
            .get(&economic.path_key)
            .map_or(&[][..], Vec::as_slice);
        let mut ops = operations(request, &economic, sojourns, &mut work)?;
        let mut allowance_index: HashMap<_, _> = binding
            .allowance_reviews
            .iter()
            .map(|r| (r.recognition_ordinal, r.clone()))
            .collect();
        let mut chargeoff_index: HashMap<i32, Vec<ReviewedChargeoff>> = HashMap::new();
        for review in &binding.chargeoffs {
            chargeoff_index
                .entry(review.recognition_ordinal)
                .or_default()
                .push(review.clone());
        }
        tick(
            &mut work,
            binding.allowance_reviews.len() + binding.chargeoffs.len(),
        )?;
        let mut state = AccountingState {
            live: [LiveState::default(); 3],
            allowance: binding.opening_allowance,
            claims: Vec::new(),
        };
        state.live[origin] = LiveState {
            principal: opening.principal,
            undrawn: opening.undrawn_commitment,
            accrued_interest: opening.accrued_interest,
            book_adjustment: opening.book_adjustment,
        };
        let mut probability = [0.0; 7];
        probability[origin] = 1.0;
        let mut matured = 0.0;
        let mut gl = [0.0; 12];
        gl[1] = opening.principal;
        gl[3] = opening.accrued_interest;
        gl[5] = opening.book_adjustment;
        gl[7] = -binding.opening_allowance;
        let initial_net = opening.principal + opening.accrued_interest + opening.book_adjustment
            - binding.opening_allowance;
        let mut intents = Vec::new();
        let mut daily = Vec::with_capacity(days as usize);
        let mut complete =
            Vec::with_capacity(economic.default.as_ref().map_or(0, |d| d.recoveries.len()));
        let mut mapped: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
        if let Some(default) = &economic.default {
            for (index, recovery) in default.recoveries.iter().enumerate() {
                let (time, day) = clock(request, recovery.year)?;
                let id = Identity {
                    id: format!("{}:recovery:{index}", binding.claim_id),
                    revision: binding.recovery_schedule_review.revision.clone(),
                };
                mapped.entry(day).or_default().push(complete.len());
                complete.push(MappedRecovery {
                    recovery_id: id,
                    exact_time: time,
                    recognition_ordinal: day,
                    principal: recovery.principal,
                    interest: recovery.interest,
                    after_reporting: day > request.reporting_end_ordinal,
                });
            }
        }
        let mut total_earned = 0.0;
        for day in (request.path.origin_time.ordinal + 1)..=request.reporting_end_ordinal {
            tick(
                &mut work,
                1 + state
                    .claims
                    .iter()
                    .map(|c| {
                        1 + c.remaining_recoveries.len() + c.settled_interest_recovery_ids.len()
                    })
                    .sum::<usize>(),
            )?;
            let cash_before = gl[0];
            let profit_before = -(gl[8] + gl[9] + gl[10] + gl[11]);
            let mut earned = 0.0;
            let mut defaults = Vec::new();
            let mut default_time = None;
            for op in ops.remove(&day).unwrap_or_default() {
                tick(&mut work, 1)?;
                match op {
                    Operation::Accrual {
                        year,
                        origin,
                        amount: interest,
                        slice,
                    } => {
                        state.live[origin].accrued_interest += interest;
                        earned += interest;
                        let intent = economic_intent(
                            binding,
                            day,
                            "realized_earned_interest",
                            &[
                                (AccountCode::LiveAccruedInterest, interest),
                                (AccountCode::InterestIncome, -interest),
                            ],
                        );
                        if !intent.postings.is_empty() {
                            append(
                                &mut intents,
                                intent,
                                Some(clock(request, year)?.0),
                                Vec::new(),
                                "exact_sojourn_ending",
                                &mut gl,
                                &mut metadata,
                                &mut intent_count,
                            )?;
                            intents.last_mut().ok_or("missing accrual journal")?.accrual =
                                Some(slice);
                        }
                    }
                    Operation::Event { index, .. } => {
                        let event = &economic.events[index];
                        let i = live_index(event.origin)?;
                        let rights = state.live[i];
                        match event.kind.as_str() {
                            "transition" if event.destination == Some(CreditState::Default) => {
                                let claim = economic
                                    .default
                                    .as_ref()
                                    .ok_or("default event without economic claim")?;
                                near(rights.principal + claim.contingent_draw, claim.principal)?;
                                near(rights.accrued_interest, claim.accrued_interest)?;
                                near(rights.book_adjustment, claim.book_adjustment)?;
                                let mut by_origin = [OriginDefaultDelta::default(); 3];
                                by_origin[i] = OriginDefaultDelta {
                                    principal: rights.principal,
                                    undrawn: rights.undrawn,
                                    contingent_draw: claim.contingent_draw,
                                    accrued_interest: rights.accrued_interest,
                                    book_adjustment: rights.book_adjustment,
                                };
                                let mut tail: BTreeMap<i32, f64> = BTreeMap::new();
                                for r in &complete {
                                    *tail.entry(r.recognition_ordinal).or_default() += r.principal;
                                }
                                defaults.push(DefaultInstruction {
                                    claim_id: binding.claim_id.clone(),
                                    economic_source: binding.economic_source.clone(),
                                    recovery_schedule_review: binding
                                        .recovery_schedule_review
                                        .clone(),
                                    by_origin,
                                    economic_principal_loss: claim.economic_principal_loss,
                                    complete_recovery_tail: tail
                                        .into_iter()
                                        .map(|(payment_ordinal, amount)| RecoveryPoint {
                                            payment_ordinal,
                                            amount,
                                        })
                                        .collect(),
                                });
                                probability = [0.0; 7];
                                probability[3 + i] = 1.0;
                                default_time = Some(clock(request, event.year)?.0);
                            }
                            "transition" => {
                                let j = live_index(
                                    event.destination.ok_or("migration missing destination")?,
                                )?;
                                state.live[j] = rights;
                                state.live[i] = LiveState::default();
                                probability = [0.0; 7];
                                probability[j] = 1.0;
                            }
                            "draw" => {
                                near(event.cash, -event.principal)?;
                                state.live[i].principal += event.principal;
                                state.live[i].undrawn -= event.principal;
                                let intent = economic_intent(
                                    binding,
                                    day,
                                    "realized_scheduled_draw",
                                    &[
                                        (AccountCode::LivePrincipal, event.principal),
                                        (AccountCode::Cash, -event.principal),
                                    ],
                                );
                                append(
                                    &mut intents,
                                    intent,
                                    Some(clock(request, event.year)?.0),
                                    vec![event.event_id.clone()],
                                    "exact_boundary",
                                    &mut gl,
                                    &mut metadata,
                                    &mut intent_count,
                                )?;
                            }
                            "repayment" | "prepayment" | "maturity" => {
                                state.live[i].principal -= event.principal;
                                state.live[i].accrued_interest -= event.interest;
                                state.live[i].book_adjustment -= event.book_adjustment_recognition;
                                if event.kind == "repayment" {
                                    let boundary = boundary_index
                                        .get(event.event_id.as_str())
                                        .ok_or("repayment boundary lineage missing")?;
                                    if let credit_path::BoundaryAction::Repayment {
                                        cancel_undrawn_fraction,
                                        ..
                                    } = &boundary.action
                                    {
                                        state.live[i].undrawn *= 1.0 - cancel_undrawn_fraction[i];
                                    } else {
                                        return Err("repayment boundary action mismatch".into());
                                    }
                                } else {
                                    state.live[i].undrawn = 0.0;
                                    probability = [0.0; 7];
                                    if event.kind == "prepayment" {
                                        probability[6] = 1.0;
                                    } else {
                                        matured = 1.0;
                                    }
                                }
                                let intent = economic_intent(
                                    binding,
                                    day,
                                    &format!("realized_{}", event.kind),
                                    &[
                                        (AccountCode::Cash, event.cash),
                                        (AccountCode::LivePrincipal, -event.principal),
                                        (AccountCode::LiveAccruedInterest, -event.interest),
                                        (
                                            AccountCode::LiveBookAdjustment,
                                            -event.book_adjustment_recognition,
                                        ),
                                        (
                                            AccountCode::BasisRecognitionExpense,
                                            event.book_adjustment_recognition,
                                        ),
                                    ],
                                );
                                append(
                                    &mut intents,
                                    intent,
                                    Some(clock(request, event.year)?.0),
                                    vec![event.event_id.clone()],
                                    "exact_boundary",
                                    &mut gl,
                                    &mut metadata,
                                    &mut intent_count,
                                )?;
                                label(&event.policy_id, &mut metadata)?;
                                intents
                                    .last_mut()
                                    .ok_or("missing boundary journal")?
                                    .contractual_policy_id = Some(event.policy_id.clone());
                            }
                            _ => return Err("unsupported trusted economic event kind".into()),
                        }
                    }
                }
            }
            total_earned += earned;
            let allowance_review = allowance_index.remove(&day);
            let after_review = allowance_review
                .as_ref()
                .filter(|r| r.phase == accounting::ReviewPhase::AfterCredit)
                .cloned();
            let chargeoffs = chargeoff_index.remove(&day).unwrap_or_default();
            let accounting_request = accounting::CreditAccountingRequest {
                schema: accounting::SCHEMA.into(),
                measure: CashflowMeasure::Physical,
                currency: binding.currency.clone(),
                monetary_unit: "currency_units".into(),
                position_id: binding.position_id.clone(),
                recognition_ordinal: day,
                reporting_end_ordinal: request.reporting_end_ordinal,
                policy: binding.policy.clone(),
                opening: state,
                defaults,
                chargeoffs,
                allowance_review: allowance_review
                    .filter(|r| r.phase == accounting::ReviewPhase::BeforeCredit),
            };
            let step = accounting::evaluate(&accounting_request)?;
            state = step.closing;
            for intent in step.intents {
                let recovery = intent.event == "modeled_physical_recovery_cash";
                let ids = if recovery {
                    mapped
                        .get(&day)
                        .into_iter()
                        .flatten()
                        .map(|i| complete[*i].recovery_id.id.clone())
                        .collect()
                } else {
                    Vec::new()
                };
                let exact = if intent.event.starts_with("default_")
                    || intent.event == "economic_default_transfer"
                {
                    default_time.clone()
                } else {
                    None
                };
                append(
                    &mut intents,
                    intent,
                    exact,
                    ids,
                    if recovery {
                        "daily_principal_recovery_coalesced"
                    } else if default_time.is_some() {
                        "reviewed_daily_credit_policy"
                    } else {
                        "reviewed_daily_policy"
                    },
                    &mut gl,
                    &mut metadata,
                    &mut intent_count,
                )?;
            }
            if let Some(indices) = mapped.get(&day) {
                let settlements = indices
                    .iter()
                    .map(|i| {
                        let r = &complete[*i];
                        ReviewedInterestRecovery {
                            recovery: r.recovery_id.clone(),
                            claim_id: binding.claim_id.clone(),
                            recognition_ordinal: day,
                            amount: r.interest,
                            policy_review: binding.interest_recovery_policy.clone(),
                            source: binding.recovery_schedule_review.clone(),
                            excess_cash: binding.interest_recovery_excess,
                        }
                    })
                    .collect();
                let interest = accounting::settle_interest_recoveries(&InterestRecoveryRequest {
                    schema: "credit-interest-recovery-1".into(),
                    measure: CashflowMeasure::Physical,
                    currency: binding.currency.clone(),
                    monetary_unit: "currency_units".into(),
                    position_id: binding.position_id.clone(),
                    recognition_ordinal: day,
                    reporting_end_ordinal: request.reporting_end_ordinal,
                    policy: binding.policy.clone(),
                    opening: state,
                    settlements,
                })?;
                state = interest.closing;
                for intent in interest.intents {
                    append(
                        &mut intents,
                        intent,
                        None,
                        indices
                            .iter()
                            .map(|i| complete[*i].recovery_id.id.clone())
                            .collect(),
                        "reviewed_daily_interest_recovery",
                        &mut gl,
                        &mut metadata,
                        &mut intent_count,
                    )?;
                }
            }
            if let Some(review) = after_review {
                tick(
                    &mut work,
                    1 + state
                        .claims
                        .iter()
                        .map(|c| {
                            1 + c.remaining_recoveries.len() + c.settled_interest_recovery_ids.len()
                        })
                        .sum::<usize>(),
                )?;
                let step = accounting::evaluate(&accounting::CreditAccountingRequest {
                    schema: accounting::SCHEMA.into(),
                    measure: CashflowMeasure::Physical,
                    currency: binding.currency.clone(),
                    monetary_unit: "currency_units".into(),
                    position_id: binding.position_id.clone(),
                    recognition_ordinal: day,
                    reporting_end_ordinal: request.reporting_end_ordinal,
                    policy: binding.policy.clone(),
                    opening: state,
                    defaults: Vec::new(),
                    chargeoffs: Vec::new(),
                    allowance_review: Some(review),
                })?;
                state = step.closing;
                for intent in step.intents {
                    append(
                        &mut intents,
                        intent,
                        None,
                        Vec::new(),
                        "reviewed_after_all_credit_cash",
                        &mut gl,
                        &mut metadata,
                        &mut intent_count,
                    )?;
                }
            }
            let closing = summed_state(&state);
            for (index, value) in closing.iter().take(7).enumerate() {
                near(*value, gl[index + 1])?;
            }
            near(probability.iter().sum::<f64>() + matured, 1.0)?;
            let profit = -(gl[8] + gl[9] + gl[10] + gl[11]);
            let net = gl[0] + closing[..7].iter().sum::<f64>();
            let rollforward_error = net - initial_net - profit;
            if rollforward_error.abs()
                > 1e-9_f64
                    .max(128.0 * f64::EPSILON * net.abs().max(initial_net.abs()).max(profit.abs()))
            {
                return Err("realized net-assets/profit roll-forward failed".into());
            }
            daily.push(DailyPathRow {
                recognition_ordinal: day,
                probability,
                matured_probability: matured,
                live: state.live,
                default_principal: closing[1],
                default_accrued_interest: closing[3],
                default_book_adjustment: closing[5],
                allowance: state.allowance,
                earned_interest: earned,
                cash_delta: gl[0] - cash_before,
                profit_delta: profit - profit_before,
                cumulative_cash: gl[0],
                cumulative_profit: profit,
                net_assets_including_cash: net,
            });
        }
        if !ops.is_empty() {
            return Err("unconsumed daily economic operations".into());
        }
        near(total_earned, economic.earned_interest)?;
        near(gl[0], economic.cash_received - economic.cash_funded)?;
        let closing = summed_state(&state);
        if let Some(claim) = state.claims.first() {
            near(
                economic.closing_principal,
                claim.outstanding_principal + claim.principal_charged_off
                    - (claim.recovery_cash_collected - claim.recovery_applied_to_principal),
            )?;
            near(
                economic.closing_accrued_interest,
                claim.outstanding_accrued_interest + claim.accrued_reversed
                    - (claim.interest_recovery_cash_collected - claim.interest_recovery_applied),
            )?;
            near(
                economic.closing_book_adjustment,
                claim.outstanding_book_adjustment + claim.basis_recognized,
            )?;
        } else {
            near(economic.closing_principal, closing[0])?;
            near(economic.closing_accrued_interest, closing[2])?;
            near(economic.closing_book_adjustment, closing[4])?;
        }
        near(economic.closing_undrawn_commitment, closing[7])?;
        paths.push(AccountedPath {
            binding: binding.clone(),
            economic,
            daily,
            intents,
            complete_recoveries: complete,
            closing: state,
        });
    }
    Ok(PathAccountingResult {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        monetary_unit: "currency_units".into(),
        source_id: request.path.source_id.clone(),
        source_revision: request.path.source_revision.clone(),
        day_mapping: request.day_mapping,
        reporting_end_ordinal: request.reporting_end_ordinal,
        work_units: work,
        paths,
    })
}
