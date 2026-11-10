//! Explicit consumer panel mapping into the existing exact/panel likelihoods.
//! DPD snapshots never become exact events, and omitted drivers remain assumptions.
use crate::accounting::credit::expected::Identity;
use crate::models::credit::calibration::snapshot;
use crate::models::credit::{covariate, transition::CreditState};
use crate::models::observation::ObservationTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

pub const SCHEMA: &str = "consumer-behaviour-spec-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Product {
    Auto,
    Personal,
    CreditCard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exit {
    Bankruptcy,
    Repossession,
    Settlement,
    ChargeOff,
    VoluntaryPayoff,
    ClosedZeroBalance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub days_past_due: u32,
    pub exit: Option<Exit>,
    pub balance: f64,
    pub line_open: bool,
    pub modified_or_reaged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub definition: covariate::Column,
    pub availability_lag_days: u32,
    pub scaling_training_end: ObservationTime,
    /// Empty for numeric values. Categorical values use a declared frozen coding.
    pub categories: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Numeric(f64),
    Category(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub value: Value,
    pub observed_at: ObservationTime,
    pub published_at: ObservationTime,
    pub source: Identity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub name: String,
    pub first_month: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifiedPolicy {
    Reject,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermConvention {
    HoldStartLevelWithinInterval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Specification {
    pub schema: String,
    pub identity: Identity,
    pub product: Product,
    pub watch_dpd: u32,
    pub delinquent_dpd: u32,
    pub default_dpd: u32,
    pub permitted_default_causes: Vec<Exit>,
    pub modified_policy: ModifiedPolicy,
    pub columns: Vec<Column>,
    pub terms: Vec<Term>,
    pub term_convention: TermConvention,
    pub minimum_training_support_years: [f64; 3],
    pub omitted_driver_assumptions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation {
    ExactTransition,
    ExactBoundary,
    ExactCensor,
    Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub record_id: String,
    pub account_id: String,
    pub source: Identity,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub start_status: Status,
    pub end_status: Status,
    pub observation: Observation,
    /// Supplied age at interval start. The selected level is held over the interval.
    pub loan_age_months: u32,
    pub features: BTreeMap<String, Feature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FitTemplate {
    Exact {
        request: Box<covariate::CovariateFitRequest>,
    },
    Snapshot {
        request: Box<snapshot::SnapshotCalibrationRequest>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub timeout_ms: u64,
    pub specification: Specification,
    pub fit_template: FitTemplate,
    pub records: Vec<Record>,
}

#[derive(Debug, Serialize)]
pub struct Prepared {
    pub schema: &'static str,
    pub specification: Specification,
    pub specification_sha256: String,
    pub fit_request: FitTemplate,
    pub training_origin_support_years: [f64; 3],
    pub training_support_definition: &'static str,
    pub source_records: Vec<Record>,
    pub adoption_eligible: bool,
}

fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("consumer behaviour requires bounded identities".into())
    } else {
        Ok(())
    }
}
fn identity(i: &Identity) -> Result<(), String> {
    label(&i.id)?;
    label(&i.revision)
}
fn index(s: CreditState) -> usize {
    match s {
        CreditState::Performing => 0,
        CreditState::Watch => 1,
        CreditState::Delinquent => 2,
        CreditState::Default => 3,
        CreditState::Prepaid => 4,
    }
}
fn permitted_column(p: Product, name: &str) -> Option<covariate::FeatureRole> {
    use covariate::FeatureRole::{Borrower, Macro};
    match name {
        "orig_score_band" | "curr_score_band" | "channel" | "vintage" | "apr_spread" => {
            Some(Borrower)
        }
        "orig_term" if p != Product::CreditCard => Some(Borrower),
        "orig_ltv" | "new_used" | "equity_ratio" | "trade_in_cycle" if p == Product::Auto => {
            Some(Borrower)
        }
        "dti" | "payment_to_income" => Some(Borrower),
        "utilisation_band" | "behaviour_state" | "promo_flag" if p == Product::CreditCard => {
            Some(Borrower)
        }
        "unemp_level"
        | "unemp_chg_12m"
        | "short_rate"
        | "consumer_rate_chg_12m"
        | "hpi_chg_12m" => Some(Macro),
        "used_vehicle_idx_chg_12m" if p == Product::Auto => Some(Macro),
        "prime_minus_policy" | "real_dpi_growth" if p != Product::Auto => Some(Macro),
        _ => None,
    }
}

pub fn state(spec: &Specification, s: &Status) -> Result<CreditState, String> {
    if spec.watch_dpd == 0
        || spec.watch_dpd >= spec.delinquent_dpd
        || spec.delinquent_dpd >= spec.default_dpd
        || spec.default_dpd > 36500
        || s.modified_or_reaged
    {
        return Err("consumer behaviour invalid DPD policy or modified/reaged status".into());
    }
    if !s.balance.is_finite() || !(0. ..=1e15).contains(&s.balance) || s.days_past_due > 36500 {
        return Err("consumer behaviour status domain".into());
    }
    if let Some(exit) = s.exit {
        if matches!(exit, Exit::VoluntaryPayoff | Exit::ClosedZeroBalance) {
            if s.balance != 0.
                || s.line_open
                || s.days_past_due != 0
                || (spec.product == Product::CreditCard) != (exit == Exit::ClosedZeroBalance)
            {
                return Err(
                    "consumer behaviour payoff requires product-specific zero-balance closure"
                        .into(),
                );
            }
            return Ok(CreditState::Prepaid);
        }
        if !spec.permitted_default_causes.contains(&exit)
            || exit == Exit::Repossession && spec.product != Product::Auto
        {
            return Err("consumer behaviour undeclared default cause".into());
        }
        return Ok(CreditState::Default);
    }
    // Zero card balance with an open line remains a live observation.
    if !s.line_open && spec.product == Product::CreditCard {
        return Err("consumer behaviour closed card requires an explicit exit".into());
    }
    Ok(if s.days_past_due >= spec.default_dpd {
        CreditState::Default
    } else if s.days_past_due >= spec.delinquent_dpd {
        CreditState::Delinquent
    } else if s.days_past_due >= spec.watch_dpd {
        CreditState::Watch
    } else {
        CreditState::Performing
    })
}

/// Construct native fit inputs only after complete source and availability validation.
/// Existing likelihoods own estimation; this mapper never manufactures intermediate states.
fn deadline(timeout_ms: u64) -> Result<impl FnMut() -> Result<(), String>, String> {
    if !(1..=120000).contains(&timeout_ms) {
        return Err("consumer behaviour timeout domain".into());
    }
    let started = Instant::now();
    Ok(move || {
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            Err("consumer behaviour original deadline exceeded".into())
        } else {
            Ok(())
        }
    })
}
pub fn prepare(r: &Request) -> Result<Prepared, String> {
    prepare_checked(r, &mut deadline(r.timeout_ms)?)
}
pub fn prepare_checked(
    r: &Request,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<Prepared, String> {
    let external = check;
    let mut clock = deadline(r.timeout_ms)?;
    let mut check = || {
        external()?;
        clock()
    };
    check()?;
    if !(1..=120000).contains(&r.timeout_ms) {
        return Err("consumer behaviour timeout domain".into());
    }
    crate::strict_json::serialized_size(r, 4 << 20)?;
    let s = &r.specification;
    identity(&s.identity)?;
    if s.schema != SCHEMA
        || s.watch_dpd == 0
        || s.watch_dpd >= s.delinquent_dpd
        || s.delinquent_dpd >= s.default_dpd
        || s.default_dpd > 36500
        || s.columns.len() > 8
        || s.terms.is_empty()
        || s.terms.len() > 32
        || s.terms[0].first_month != 0
        || r.records.is_empty()
        || r.records.len() > 20000
        || s.omitted_driver_assumptions.is_empty()
        || s.omitted_driver_assumptions.len() > 64
    {
        return Err("consumer behaviour specification/count domain".into());
    }
    let (train_start, train_end, holdout_end, exact_mode) = match &r.fit_template {
        FitTemplate::Exact { request: q } => {
            if !q.segments.is_empty()
                || !q.columns.is_empty()
                || !q.term_levels.is_empty()
                || q.model_policy != s.identity
            {
                return Err(
                    "consumer behaviour empty exact template and matching policy required".into(),
                );
            }
            (q.train_start, q.train_end, q.holdout_end, true)
        }
        FitTemplate::Snapshot { request: q } => {
            if !q.snapshots.is_empty()
                || !q.default_outcomes.is_empty()
                || !q.covariate_names.is_empty()
                || !q.term_levels.is_empty()
                || q.model_policy_id != s.identity.id
            {
                return Err(
                    "consumer behaviour empty snapshot template and matching policy required"
                        .into(),
                );
            }
            (q.train_start, q.train_end, q.holdout_end, false)
        }
    };
    let a = train_start.seconds()?;
    let b = train_end.seconds()?;
    let end = holdout_end.seconds()?;
    if !(a < b && b < end) {
        return Err("consumer behaviour chronological split".into());
    }
    let mut names = HashSet::new();
    for c in &s.columns {
        check()?;
        let d = &c.definition;
        label(&d.name)?;
        label(&d.unit)?;
        identity(&d.scaling_review)?;
        let role = permitted_column(s.product, &d.name)
            .ok_or("consumer behaviour unknown product covariate")?;
        if std::mem::discriminant(&role) != std::mem::discriminant(&d.role)
            || !names.insert(&d.name)
            || c.availability_lag_days > 36500
            || c.scaling_training_end.seconds()? > b
            || !d.offset.is_finite()
            || !d.scale.is_finite()
            || d.scale <= 0.
            || !d.raw_lower.is_finite()
            || !d.raw_upper.is_finite()
            || d.raw_lower > d.raw_upper
            || c.categories.len() > 64
        {
            return Err("consumer behaviour feature role/scaling/domain".into());
        }
        for (k, v) in &c.categories {
            label(k)?;
            if !v.is_finite() || *v < d.raw_lower || *v > d.raw_upper {
                return Err("consumer behaviour category domain".into());
            }
        }
    }
    let mut terms = HashSet::new();
    for (i, t) in s.terms.iter().enumerate() {
        label(&t.name)?;
        if !terms.insert(&t.name)
            || t.first_month > 2400
            || (i > 0 && t.first_month <= s.terms[i - 1].first_month)
        {
            return Err("consumer behaviour term grid".into());
        }
    }
    for x in &s.omitted_driver_assumptions {
        label(x)?;
    }
    for x in s.minimum_training_support_years {
        if !x.is_finite() || x <= 0. || x > 2e6 {
            return Err("consumer behaviour minimum exposure".into());
        }
    }
    if s.permitted_default_causes.is_empty() || s.permitted_default_causes.len() > 4 {
        return Err("consumer behaviour default policy".into());
    }
    for (i, cause) in s.permitted_default_causes.iter().enumerate() {
        if matches!(cause, Exit::VoluntaryPayoff | Exit::ClosedZeroBalance)
            || (*cause == Exit::Repossession && s.product != Product::Auto)
            || s.permitted_default_causes[..i].contains(cause)
        {
            return Err("consumer behaviour default cause policy".into());
        }
    }
    for row in &r.records {
        check()?;
        if row.start_status.modified_or_reaged || row.end_status.modified_or_reaged {
            return Err("consumer behaviour modified/reaged account rejected".into());
        }
    }
    let mut prepared = r.fit_template.clone();
    match &mut prepared {
        FitTemplate::Exact { request: q } => {
            q.columns = s.columns.iter().map(|c| c.definition.clone()).collect();
            q.term_levels = s.terms.iter().map(|t| t.name.clone()).collect();
        }
        FitTemplate::Snapshot { request: q } => {
            q.covariate_names = s
                .columns
                .iter()
                .map(|c| c.definition.name.clone())
                .collect();
            q.term_levels = s.terms.iter().map(|t| t.name.clone()).collect();
        }
    }
    let mut exposure = [0.; 3];
    let mut ids = HashSet::new();
    for row in &r.records {
        check()?;
        label(&row.record_id)?;
        label(&row.account_id)?;
        identity(&row.source)?;
        let start = row.start.seconds()?;
        let stop = row.end.seconds()?;
        if !ids.insert(&row.record_id)
            || start >= stop
            || start < a
            || stop > end
            || row.loan_age_months > 2400
            || exact_mode == (row.observation == Observation::Snapshot)
            || row.features.len() != s.columns.len()
        {
            return Err("consumer behaviour record identity/mode/chronology".into());
        }
        let origin = state(s, &row.start_status)?;
        let destination = state(s, &row.end_status)?;
        if index(origin) >= 3 {
            return Err(
                "consumer behaviour absorbing origin requires a separate recovery owner".into(),
            );
        }
        let term = &s.terms[s
            .terms
            .partition_point(|t| t.first_month <= row.loan_age_months)
            - 1]
        .name;
        let mut values = Vec::with_capacity(s.columns.len());
        let mut available = a;
        for c in &s.columns {
            let f = row
                .features
                .get(&c.definition.name)
                .ok_or("consumer behaviour missing/unknown feature")?;
            identity(&f.source)?;
            let observed = f.observed_at.seconds()?;
            let published = f.published_at.seconds()?;
            let usable = published.max(observed + i64::from(c.availability_lag_days) * 86400);
            if observed > published || usable > start {
                return Err("consumer behaviour unavailable future feature".into());
            }
            available = available.max(usable);
            let v = match &f.value {
                Value::Numeric(v) if c.categories.is_empty() => *v,
                Value::Category(k) => *c
                    .categories
                    .get(k)
                    .ok_or("consumer behaviour unknown category")?,
                _ => return Err("consumer behaviour declared feature encoding differs".into()),
            };
            if !v.is_finite() || v < c.definition.raw_lower || v > c.definition.raw_upper {
                return Err("consumer behaviour raw feature domain".into());
            }
            let projected = (v - c.definition.offset) / c.definition.scale;
            if !projected.is_finite() || (!exact_mode && projected.abs() > 10.) {
                return Err("consumer behaviour transformed feature domain".into());
            }
            values.push(if exact_mode { v } else { projected });
        }

        if exact_mode && row.observation != Observation::ExactTransition && origin != destination {
            return Err("consumer behaviour exact boundary/censor cannot change state".into());
        }
        if row.observation == Observation::ExactTransition && origin == destination {
            return Err("consumer behaviour exact transition must change state".into());
        }
        let included_training = if exact_mode { start < b } else { stop <= b };
        if included_training {
            exposure[index(origin)] += (stop.min(b) - start) as f64 / (365. * 86400.);
        }
        match &mut prepared {
            FitTemplate::Exact { request: q } => q.segments.push(covariate::Segment {
                segment_id: row.record_id.clone(),
                account_id: row.account_id.clone(),
                source: row.source.clone(),
                start: row.start,
                end: row.end,
                origin,
                ending: match row.observation {
                    Observation::ExactTransition => covariate::Ending::Transition { destination },
                    Observation::ExactBoundary => covariate::Ending::CovariateBoundary {},
                    Observation::ExactCensor => covariate::Ending::RightCensored {},
                    Observation::Snapshot => unreachable!(),
                },
                available_time: ObservationTime {
                    ordinal: (available / 86400) as i32,
                    seconds_of_day: (available % 86400) as u32,
                },
                covariates: values,
                term_level: term.clone(),
            }),
            FitTemplate::Snapshot { request: q } => q.snapshots.push(snapshot::SnapshotInterval {
                snapshot_id: row.record_id.clone(),
                account_id: row.account_id.clone(),
                source_id: row.source.id.clone(),
                start: row.start,
                end: row.end,
                origin,
                destination,
                covariates: values,
                term_level: term.clone(),
            }),
        }
    }
    for (i, support) in exposure.iter().enumerate() {
        let fitted = match &prepared {
            FitTemplate::Exact { request: q } => {
                matches!(q.rows[i], covariate::OriginModel::LogLinear { .. })
            }
            FitTemplate::Snapshot { request: q } => {
                matches!(q.rows[i], snapshot::RowModel::LogLinear { .. })
            }
        };
        if fitted && *support < s.minimum_training_support_years[i] {
            return Err("consumer behaviour insufficient origin exposure; explicit reviewed fixed row required".into());
        }
    }

    check()?;
    let result = Prepared {
        schema: SCHEMA,
        specification: s.clone(),
        specification_sha256: format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(s).map_err(|e| e.to_string())?)
        ),
        fit_request: prepared,
        training_origin_support_years: exposure,
        training_support_definition: if exact_mode {
            "known_origin_exposure"
        } else {
            "observed_start_state_interval_duration"
        },
        source_records: r.records.clone(),
        adoption_eligible: false,
    };
    check()?;
    Ok(result)
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fitted {
    Exact {
        result: Box<covariate::CovariateFitResult>,
    },
    Snapshot {
        result: Box<snapshot::SnapshotCalibrationResult>,
    },
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub prepared: Prepared,
    pub fitted: Fitted,
    pub adoption_eligible: bool,
}

pub fn fit(r: &Request) -> Result<ResultData, String> {
    fit_checked(r, &mut deadline(r.timeout_ms)?)
}
pub fn fit_checked(
    r: &Request,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let external = check;
    let mut clock = deadline(r.timeout_ms)?;
    let mut check = || {
        external()?;
        clock()
    };
    let prepared = prepare_checked(r, &mut check)?;
    check()?;
    let fitted = match &prepared.fit_request {
        FitTemplate::Exact { request } => Fitted::Exact {
            result: Box::new(covariate::fit(request)?),
        },
        FitTemplate::Snapshot { request } => Fitted::Snapshot {
            result: Box::new(snapshot::fit(request)?),
        },
    };
    check()?;
    Ok(ResultData {
        prepared,
        fitted,
        adoption_eligible: false,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    pub model_revision: Identity,
    pub review: Identity,
    pub scenario_id: String,
    pub timeout_ms: u64,
    /// Raw, ordered features use the retained specification, with explicit P provenance.
    pub intervals: Vec<covariate::DriverInterval>,
    /// Complete monetary laws and draws are supplied independently. Hazard slots must be zero.
    pub path_source: crate::credit_path::CreditPathRequest,
}
#[derive(Debug, Serialize)]
pub struct ProjectionResult {
    pub product: Product,
    pub specification_sha256: String,
    pub model_revision: Identity,
    pub review: Identity,
    pub scenario_id: String,
    pub driver_intervals: Vec<covariate::DriverInterval>,
    pub projected_path_source: crate::credit_path::CreditPathRequest,
    pub paths: crate::credit_path::CreditPathsResult,
    pub adoption_eligible: bool,
}

/// Raw cardinality reservation, usable by a coordinator before fitting/generation.
pub fn projection_event_upper(r: &Projection) -> Result<usize, String> {
    crate::strict_json::serialized_size(r, 4 << 20)?;
    let source = &r.path_source;
    if r.intervals.is_empty()
        || r.intervals.len() > 64
        || r.intervals.len() != source.intervals.len()
        || source.paths.is_empty()
        || source.paths.len() > 128
        || source.boundaries.len() > 128
        || source.max_jumps_per_path > 128
    {
        return Err("consumer behaviour projection cardinality admission".into());
    }
    let recoveries = source
        .intervals
        .iter()
        .flat_map(|i| &i.complete_recovery_tails)
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    let events = source
        .max_jumps_per_path
        .checked_add(source.boundaries.len())
        .and_then(|n| n.checked_add(recoveries))
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(source.paths.len()))
        .ok_or("consumer behaviour path event overflow")?;
    if events > 4096 {
        return Err("consumer behaviour complete path output admission".into());
    }
    Ok(events)
}

impl ResultData {
    /// Bounded analysis projection into the complete shared credit-path operator.
    /// Daily installment/card action ownership is deliberately a separate adapter.
    pub fn project_paths(&self, r: &Projection) -> Result<ProjectionResult, String> {
        self.project_paths_checked(r, &mut deadline(r.timeout_ms)?)
    }
    pub fn project_paths_checked(
        &self,
        r: &Projection,
        check: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<ProjectionResult, String> {
        let external = check;
        let mut clock = deadline(r.timeout_ms)?;
        let mut check = || {
            external()?;
            clock()
        };
        check()?;
        use crate::credit::CreditMeasure;
        crate::strict_json::serialized_size(r, 4 << 20)?;
        identity(&r.model_revision)?;
        identity(&r.review)?;
        label(&r.scenario_id)?;
        let source = &r.path_source;
        let spec = &self.prepared.specification;
        let origin = source.origin_time.seconds()?;
        let train_end = match &self.prepared.fit_request {
            FitTemplate::Exact { request } => request.train_end,
            FitTemplate::Snapshot { request } => request.train_end,
        };
        if source.measure != CreditMeasure::Physical
            || source.origin_time <= train_end
            || r.intervals.is_empty()
            || r.intervals.len() > 64
            || r.intervals.len() != source.intervals.len()
            || source.paths.is_empty()
            || source.paths.len() > 128
            || source.boundaries.len() > 128
            || source.max_jumps_per_path > 128
            || r.timeout_ms == 0
            || r.timeout_ms > 120000
        {
            return Err("consumer behaviour Physical projection/count/chronology".into());
        }
        projection_event_upper(r)?;
        crate::credit_path::validate_request_with_check(source, &mut check)?;
        let mut previous = source.origin_time;
        let mut ids = HashSet::new();
        for (i, interval) in r.intervals.iter().enumerate() {
            check()?;
            label(&interval.interval_id)?;
            identity(&interval.source)?;
            interval.start.seconds()?;
            let end = interval.end.seconds()?;
            let available = interval.available_time.seconds()?;
            if !ids.insert(&interval.interval_id)
                || interval.start != previous
                || interval.end <= interval.start
                || available > origin
                || interval.covariates.len() != spec.columns.len()
                || !spec.terms.iter().any(|t| t.name == interval.term_level)
                || matches!(interval.kind, covariate::DriverKind::SuppliedObserved)
                    && interval.start.seconds()? > available
                || ((end - origin) as f64 / (365. * 86400.)).to_bits()
                    != source.intervals[i].end_year.to_bits()
                || source.intervals[i]
                    .hazards_per_year
                    .iter()
                    .flatten()
                    .any(|x| *x != 0.)
            {
                return Err(
                    "consumer behaviour projection grid/availability/zero hazard slots".into(),
                );
            }
            for (value, column) in interval.covariates.iter().zip(&spec.columns) {
                let d = &column.definition;
                if !value.is_finite() || *value < d.raw_lower || *value > d.raw_upper {
                    return Err("consumer behaviour projection raw driver domain".into());
                }
            }
            previous = interval.end;
        }
        let hazards = match &self.fitted {
            Fitted::Exact { result } => {
                covariate::project(&covariate::CovariateProjectionRequest {
                    schema: covariate::SCHEMA.into(),
                    measure: CreditMeasure::Physical,
                    model: result.model.clone(),
                    model_revision: r.model_revision.clone(),
                    review: r.review.clone(),
                    use_policy: covariate::ProjectionUse::AnalyticalOnly,
                    position_id: source.source_id.clone(),
                    scenario_id: r.scenario_id.clone(),
                    origin_time: source.origin_time,
                    end_time: previous,
                    timeout_ms: r.timeout_ms,
                    intervals: r.intervals.clone(),
                })?
                .intervals
                .into_iter()
                .map(|i| i.hazards_per_year)
                .collect::<Vec<_>>()
            }
            Fitted::Snapshot { result } => r
                .intervals
                .iter()
                .map(|interval| {
                    check()?;
                    let values = interval
                        .covariates
                        .iter()
                        .zip(&spec.columns)
                        .map(|(v, c)| (v - c.definition.offset) / c.definition.scale)
                        .collect::<Vec<_>>();
                    result.project_hazards(&values, &interval.term_level)
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let mut projected = source.clone();
        for (interval, hazard) in projected.intervals.iter_mut().zip(hazards) {
            interval.hazards_per_year = hazard;
        }
        check()?;
        let paths = crate::credit_path::evaluate(&projected)?;
        check()?;
        let result = ProjectionResult {
            product: spec.product,
            specification_sha256: self.prepared.specification_sha256.clone(),
            model_revision: r.model_revision.clone(),
            review: r.review.clone(),
            scenario_id: r.scenario_id.clone(),
            driver_intervals: r.intervals.clone(),
            projected_path_source: projected,
            paths,
            adoption_eligible: false,
        };
        crate::strict_json::serialized_size(&result, 16 << 20)?;
        check()?;
        Ok(result)
    }
}
