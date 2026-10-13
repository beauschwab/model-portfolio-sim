//! Supplied-law joint Gaussian/credit generation and out-of-sample LSM.
//! Raw contractual owners construct every earned right and settlement. Regression
//! uses observable current state; fresh evaluation outcomes never fit a policy.
use crate::{
    commercial_cash::{economic_cash, EconomicCash},
    commercial_credit_compiler as owner, commercial_exercise as exercise,
    model_market::{curves::SuppliedCurve, joint::covariance_root},
    random::SharedDraws,
    term_enhancements::{
        self as term,
        exercise::{Decision, ExerciseTiming},
        indexed::{Coupon, RateSource},
    },
};
use nalgebra::{DMatrix, DVector};
use portfolio_model_core::{
    cashflow::{validate_ordinal, CashflowMeasure},
    credit::{CreditMeasure, CreditState},
    credit_path::DrawStream,
    strict_json,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const SCHEMA: &str = "commercial-generated-exercise-1";
pub mod calibration;
// Separate explicitly selected sampling tier; ordinary graphs/source budgets
// remain unchanged. Retained compact paths are capped independently below.
const MAX_WORK: usize = 4_000_000_000;
const MAX_BYTES: usize = 128 << 20;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    pub driver_id: String,
    pub initial_level: f64,
    pub long_run_mean: f64,
    pub mean_reversion: f64,
    /// All generated coordinates are observable at the declared midnight clock.
    pub observation_policy: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CouponLoading {
    pub period_id: String,
    pub observation_index: usize,
    pub driver_index: usize,
    pub loading: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HazardLoading {
    pub interval_index: usize,
    pub origin_index: usize,
    pub destination_index: usize,
    pub betas: Vec<f64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub paths: usize,
    pub market_seed: Vec<u32>,
    pub credit_seed: u64,
    pub randomness_domain: term::Identity,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub admission_tier: String,
    pub identity: term::Identity,
    pub source: Box<owner::Request>,
    pub policy: exercise::Policy,
    pub discount_curve: SuppliedCurve,
    pub fixed_base_oas: f64,
    pub parallel_bump: f64,
    pub drivers: Vec<Driver>,
    /// Instantaneous OU covariance, in driver units, row-major.
    pub annual_covariance: Vec<f64>,
    pub time_ordinals: Vec<i32>,
    pub coupon_loadings: Vec<CouponLoading>,
    pub hazard_loadings: Vec<HazardLoading>,
    pub training: Sample,
    pub evaluation: Sample,
    pub regression_policy: String,
}
#[derive(Clone, Serialize)]
pub struct Regression {
    pub ordinal: i32,
    pub means: Vec<f64>,
    pub scales: Vec<f64>,
    pub active_features: Vec<usize>,
    pub coefficients: Vec<f64>,
    pub observations: usize,
    pub rank: usize,
    pub singular_values: Vec<f64>,
    pub observed_live_states: Vec<CreditState>,
}
#[derive(Serialize)]
pub struct PathOutcome {
    pub path_key: u64,
    pub value: f64,
    pub first_exercise_ordinal: Option<i32>,
    pub first_exercise_decision: Decision,
    pub full_native_cash_replayed: bool,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
    pub regressions: Vec<Regression>,
    pub training_value: f64,
    pub evaluation_value: f64,
    /// Standard error of independent antithetic clusters, not independent paths.
    pub evaluation_standard_error: f64,
    pub evaluation_clusters: usize,
    pub evaluation_paths: Vec<PathOutcome>,
    pub generated_joint_market_credit: bool,
    pub evaluation_outcomes_used_for_fit: bool,
    pub finite_distribution: bool,
    pub convergence_established: bool,
    pub physical_journal_eligible: bool,
    pub empirical_validation: bool,
    pub disclosure: &'static str,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn amount(v: f64) -> Result<f64, String> {
    term::amount(v)
}
fn utc(ordinal: i32) -> i64 {
    i64::from(ordinal - 719163) * 86400
}
fn live(s: CreditState) -> bool {
    matches!(
        s,
        CreditState::Performing | CreditState::Watch | CreditState::Delinquent
    )
}
fn df(r: &Request, t: f64) -> Result<f64, String> {
    amount(
        r.discount_curve.value_validated(t)?.discount_factor
            * (-(r.fixed_base_oas + r.parallel_bump) * t).exp(),
    )
}
fn credit_start(r: &Request, index: usize) -> Result<i32, String> {
    let year = if index == 0 {
        0.
    } else {
        r.source.credit.intervals[index - 1].end_year
    };
    let day = (year * 365.).round() as i32;
    if (f64::from(day) / 365.).to_bits() != year.to_bits() {
        return Err(
            "generated credit coefficients require exact actual-day interval starts".into(),
        );
    }
    r.source
        .source
        .current
        .asof_ordinal
        .checked_add(day)
        .ok_or("generated credit date overflow".into())
}
pub fn admit(r: &Request) -> Result<(usize, usize), String> {
    admitted_law(r, CreditMeasure::Pricing)
}
pub(crate) fn admitted_law(r: &Request, measure: CreditMeasure) -> Result<(usize, usize), String> {
    check()?;
    let cash_measure = match measure {
        CreditMeasure::Physical => CashflowMeasure::Physical,
        CreditMeasure::Pricing => CashflowMeasure::Pricing,
    };
    let asof = r.source.source.current.asof_ordinal;
    let d = r.drivers.len();
    if r.schema != SCHEMA
        || r.admission_tier != "bounded_generated_1024_samples-1"
        || r.regression_policy != "native_observable_quadratic_svd_train_evaluate-1"
        || !(1..=4).contains(&d)
        || !(2..=129).contains(&r.time_ordinals.len())
        || r.time_ordinals[0] != asof
        || r.time_ordinals.windows(2).any(|v| v[0] >= v[1])
        || r.time_ordinals
            .last()
            .is_none_or(|t| *t > r.source.source.current.tail_end_ordinal)
        || r.source.credit.paths.len() != 1
        || r.source.source.current.measure != cash_measure
        || r.source.source.original.measure != cash_measure
        || !r.source.performing_draw_tranches.is_empty()
    {
        return Err("bounded explicit Pricing generated commercial contract and chronological grid required".into());
    }
    let mut metadata = 0;
    term::identity(&r.identity, &mut metadata)?;
    r.discount_curve.validate()?;
    if r.discount_curve.currency != r.source.source.current.currency {
        return Err("generated discount currency differs".into());
    }
    for v in [r.fixed_base_oas, r.parallel_bump] {
        amount(v)?;
    }
    if !(-5. ..=5.).contains(&r.fixed_base_oas) || !(-0.1..=0.1).contains(&r.parallel_bump) {
        return Err("generated OAS/bump domain exceeded".into());
    }
    let mut names = BTreeSet::new();
    for driver in &r.drivers {
        if driver.driver_id.is_empty()
            || driver.driver_id.len() > 128
            || driver.driver_id.chars().any(char::is_control)
            || !names.insert(&driver.driver_id)
            || driver.observation_policy != "observable_date_midnight_utc-1"
            || !driver.mean_reversion.is_finite()
            || !(0. ..=100.).contains(&driver.mean_reversion)
        {
            return Err("explicit unique observable OU driver required".into());
        }
        for level in [driver.initial_level, driver.long_run_mean] {
            if !level.is_finite() || !(-100. ..=100.).contains(&level) {
                return Err("OU level domain exceeded".into());
            }
        }
    }
    covariance_root(&r.annual_covariance, d, d)?;
    for &ordinal in &r.time_ordinals {
        validate_ordinal(ordinal)?;
    }
    let mut policy = r.policy.clone();
    if policy.information_policy != "native_observable_markov_state-1" {
        return Err(
            "generated exercise requires explicit observable Markov state assumption".into(),
        );
    }
    policy.information_policy = "native_full_observed_prefix-1".into();
    exercise::admitted_work_with_measure(&policy, &[(&r.source, 1.)], measure)?;
    if r.policy
        .exercise_dates
        .iter()
        .any(|date| !r.time_ordinals.contains(&date.ordinal))
    {
        return Err("every exercise date must be in the generated state grid".into());
    }
    let mut mapped = BTreeSet::new();
    for loading in &r.coupon_loadings {
        if loading.driver_index >= d
            || !loading.loading.is_finite()
            || loading.loading.abs() > 100.
            || !mapped.insert((loading.period_id.as_str(), loading.observation_index))
        {
            return Err("unique bounded coupon driver mapping required".into());
        }
        let period = r
            .source
            .source
            .current
            .periods
            .iter()
            .find(|p| p.period_id == loading.period_id)
            .ok_or("coupon loading period absent")?;
        let Coupon::Indexed { observations, .. } = &period.coupon else {
            return Err("generated rate loading requires indexed coupon".into());
        };
        let observation = observations
            .get(loading.observation_index)
            .ok_or("coupon loading observation absent")?;
        if observation.fixing_ordinal <= asof
            || !r.time_ordinals.contains(&observation.fixing_ordinal)
            || !matches!(observation.source, RateSource::ConditionalProjection { .. })
            || observation.selection_cutoff_unix_seconds != utc(observation.fixing_ordinal)
        {
            return Err(
                "future projected coupon must bind an exact generated midnight fixing".into(),
            );
        }
    }
    for period in &r.source.source.current.periods {
        if let Coupon::Indexed { observations, .. } = &period.coupon {
            for (i, observation) in observations.iter().enumerate() {
                if observation.fixing_ordinal > asof
                    && !mapped.contains(&(period.period_id.as_str(), i))
                {
                    return Err(
                        "all future coupon observations require native generated mappings".into(),
                    );
                }
            }
        }
    }
    let mut hazards = BTreeSet::new();
    for h in &r.hazard_loadings {
        if h.interval_index >= r.source.credit.intervals.len()
            || h.origin_index >= 3
            || h.destination_index >= 5
            || h.origin_index == h.destination_index
            || h.betas.len() != d
            || !hazards.insert((h.interval_index, h.origin_index, h.destination_index))
            || h.betas.iter().any(|v| !v.is_finite() || v.abs() > 100.)
        {
            return Err("explicit bounded joint hazard loading required".into());
        }
        if !r
            .time_ordinals
            .contains(&credit_start(r, h.interval_index)?)
        {
            return Err("credit interval start requires an observable generated state".into());
        }
        if r.source.credit.intervals[h.interval_index].hazards_per_year[h.origin_index]
            [h.destination_index]
            == 0.
            && h.betas.iter().any(|v| *v != 0.)
        {
            return Err("zero hazard cannot acquire implicit modeled credit".into());
        }
    }
    for s in [&r.training, &r.evaluation] {
        term::identity(&s.randomness_domain, &mut metadata)?;
        if !(128..=512).contains(&s.paths)
            || s.paths % 2 != 0
            || s.market_seed.is_empty()
            || s.market_seed.len() > 16
        {
            return Err("even 128..512 native seeded sample required".into());
        }
    }
    if r.training.market_seed == r.evaluation.market_seed
        || r.training.credit_seed == r.evaluation.credit_seed
        || strict_json::encode(&r.training.randomness_domain, 4096)?
            == strict_json::encode(&r.evaluation.randomness_domain, 4096)?
    {
        return Err("training and fresh evaluation randomness domains/seeds must differ".into());
    }
    let mut source = owner::admit_economic_with_measure(&r.source, measure)?;
    for date in &r.policy.exercise_dates {
        let stop = owner::NativeStop::from_compiled_policy_with_timing(
            r.source.credit.paths[0].path_key,
            format!("generated-exercise:{}:{}", r.identity.id, date.ordinal),
            date.ordinal,
            owner::NativeStopAction::Call,
            10.,
            date.call_fee.max(date.put_fee),
            r.policy.identity.clone(),
            match date.timing {
                ExerciseTiming::BeforeScheduledCash => owner::NativeStopTiming::BeforeScheduledCash,
                ExerciseTiming::AfterScheduledCash => owner::NativeStopTiming::AfterScheduledCash,
            },
            r.policy
                .rights_only_termination
                .as_ref()
                .map(|p| p.review.clone()),
        )?;
        let candidate = owner::admit_economic_with_stops(&r.source, measure, &[stop])?;
        source.joined_work_upper = source.joined_work_upper.max(candidate.joined_work_upper);
        source.joined_bytes_upper = source.joined_bytes_upper.max(candidate.joined_bytes_upper);
    }
    let n = r
        .training
        .paths
        .checked_add(r.evaluation.paths)
        .ok_or("sample overflow")?;
    let work = source
        .joined_work_upper
        .checked_mul(n)
        .and_then(|v| v.checked_mul(2))
        .ok_or("generated work overflow")?;
    let recovery_cash = r
        .source
        .credit
        .intervals
        .iter()
        .flat_map(|p| p.complete_recovery_tails.iter())
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    let cash_upper = r
        .source
        .source
        .current
        .periods
        .len()
        .checked_add(recovery_cash + 4)
        .ok_or("generated cash upper overflow")?;
    let observations = r
        .source
        .source
        .current
        .periods
        .iter()
        .map(|p| match &p.coupon {
            Coupon::Indexed { observations, .. } => observations.len(),
            _ => 0,
        })
        .sum::<usize>();
    let features = 2 * d + 4 + observations + 2 * r.source.source.current.periods.len();
    // Include observed reset history and gross coupon entitlements. Net A/current
    // OU state alone is not Markov for coupons fixed earlier or lagged hazards.
    if features > 64 {
        return Err("generated observable contractual state exceeds 64 features".into());
    }
    let compact_per_path = r.time_ordinals.len() * d * 8
        + r.policy.exercise_dates.len() * (512 + features * 8)
        + cash_upper * 1024;
    let tape_bytes = n * (r.time_ordinals.len() - 1) * (d + 2) * 8;
    let bytes = source
        .joined_bytes_upper
        .checked_add(n * compact_per_path)
        .and_then(|v| v.checked_add(tape_bytes))
        .ok_or("generated retained bytes overflow")?;
    let columns = (1 + features + features * (features + 1) / 2).min(r.training.paths / 2);
    let regress_work = n
        .checked_mul(r.policy.exercise_dates.len() + 1)
        .and_then(|v| v.checked_mul(columns * columns * 16))
        .ok_or("regression work overflow")?;
    let total = work
        .checked_add(regress_work)
        .ok_or("generated aggregate overflow")?;
    if total > MAX_WORK || bytes > MAX_BYTES || strict_json::serialized_size(r, 1 << 20)? > 1 << 20
    {
        return Err(format!("generated commercial aggregate work/byte admission exceeded: work={total}, bytes={bytes}, source_work={}", source.joined_work_upper));
    }
    Ok((total, bytes))
}
pub(crate) struct Tape {
    levels: Vec<f64>,
    pub(crate) d: usize,
    steps: usize,
}
impl Tape {
    fn at(&self, path: usize, step: usize) -> &[f64] {
        &self.levels[(path * self.steps + step) * self.d..(path * self.steps + step + 1) * self.d]
    }
}
pub(crate) fn generate_tape(r: &Request, sample: &Sample) -> Result<Tape, String> {
    let d = r.drivers.len();
    let steps = r.time_ordinals.len();
    let draws = SharedDraws::new(&sample.market_seed, [sample.paths, steps - 1, d])?;
    let mut roots = Vec::new();
    for grid in r.time_ordinals.windows(2) {
        check()?;
        let dt = f64::from(grid[1] - grid[0]) / 365.;
        let mut covariance = vec![0.; d * d];
        for i in 0..d {
            for j in 0..d {
                let a = r.drivers[i].mean_reversion + r.drivers[j].mean_reversion;
                covariance[i * d + j] = r.annual_covariance[i * d + j]
                    * if a == 0. { dt } else { -(-a * dt).exp_m1() / a };
            }
        }
        roots.push(covariance_root(&covariance, d, d)?.0);
    }
    let mut levels = vec![0.; sample.paths * steps * d];
    for path in 0..sample.paths {
        for i in 0..d {
            levels[(path * steps) * d + i] = r.drivers[i].initial_level;
        }
        for step in 1..steps {
            check()?;
            let dt = f64::from(r.time_ordinals[step] - r.time_ordinals[step - 1]) / 365.;
            for i in 0..d {
                let driver = &r.drivers[i];
                let old = levels[(path * steps + step - 1) * d + i];
                let innovation: f64 = (0..d)
                    .map(|j| {
                        roots[step - 1][i * d + j]
                            * draws.rates[(path * (steps - 1) + step - 1) * d + j]
                    })
                    .sum();
                levels[(path * steps + step) * d + i] = amount(
                    driver.long_run_mean
                        + (old - driver.long_run_mean) * (-driver.mean_reversion * dt).exp()
                        + innovation,
                )?;
            }
        }
    }
    Ok(Tape { levels, d, steps })
}
fn path_request(
    r: &Request,
    sample: &Sample,
    tape: &Tape,
    path: usize,
) -> Result<owner::Request, String> {
    generated_path(r, sample, tape, path, CreditMeasure::Pricing)
}
pub(crate) fn generated_path(
    r: &Request,
    sample: &Sample,
    tape: &Tape,
    path: usize,
    measure: CreditMeasure,
) -> Result<owner::Request, String> {
    check()?;
    let mut raw = (*r.source).clone();
    // Stable path keys preserve paired base/bump CRN; the two sample streams remain independent.
    raw.credit.paths[0].path_key = path as u64 + 1;
    raw.credit.paths[0].path_id = format!("generated-{path}");
    raw.credit.draw_stream = DrawStream::CounterSplitmix64V1 {
        stream_id: sample.randomness_domain.id.clone(),
        stream_revision: sample.randomness_domain.revision.clone(),
        seed: sample.credit_seed,
    };
    for loading in &r.coupon_loadings {
        let period = raw
            .source
            .current
            .periods
            .iter_mut()
            .find(|p| p.period_id == loading.period_id)
            .ok_or("generated coupon absent")?;
        let Coupon::Indexed { observations, .. } = &mut period.coupon else {
            return Err("generated indexed coupon absent".into());
        };
        let observation = &mut observations[loading.observation_index];
        let step = r
            .time_ordinals
            .binary_search(&observation.fixing_ordinal)
            .map_err(|_| "generated fixing grid absent")?;
        observation.rate = amount(
            observation.rate
                + loading.loading
                    * (tape.at(path, step)[loading.driver_index]
                        - r.drivers[loading.driver_index].initial_level),
        )?;
        observation.source = RateSource::ConditionalProjection {
            model: r.identity.clone(),
            path: raw.source.current.path.clone(),
            information_unix_seconds: utc(observation.fixing_ordinal),
            measure: match measure {
                CreditMeasure::Physical => CashflowMeasure::Physical,
                CreditMeasure::Pricing => CashflowMeasure::Pricing,
            },
        };
    }
    for loading in &r.hazard_loadings {
        let ordinal = credit_start(r, loading.interval_index)?;
        let step = r
            .time_ordinals
            .binary_search(&ordinal)
            .map_err(|_| "generated credit grid absent")?;
        let exponent: f64 = loading
            .betas
            .iter()
            .enumerate()
            .map(|(i, b)| b * (tape.at(path, step)[i] - r.drivers[i].initial_level))
            .sum();
        let base = raw.credit.intervals[loading.interval_index].hazards_per_year
            [loading.origin_index][loading.destination_index];
        let value = if base == 0. {
            0.
        } else {
            (base.ln() + exponent).exp()
        };
        if !value.is_finite() || !(0. ..=100.).contains(&value) {
            return Err("generated joint hazard outside native domain; no clipping".into());
        }
        raw.credit.intervals[loading.interval_index].hazards_per_year[loading.origin_index]
            [loading.destination_index] = value;
    }
    Ok(raw)
}
pub(crate) struct Point {
    pub(crate) features: Vec<f64>,
    pub(crate) state: CreditState,
    pub(crate) principal: f64,
    pub(crate) legal: f64,
    pub(crate) surviving_rights: bool,
}
struct Path {
    cash: Vec<EconomicCash>,
    points: Vec<Point>,
}
fn collect(r: &Request, sample: &Sample, tape: &Tape) -> Result<Vec<Path>, String> {
    let opening = r.source.source.current.opening_principal;
    if opening <= 0. {
        return Err("generated source requires positive original entitlement principal".into());
    }
    let mut paths = Vec::new();
    for i in 0..sample.paths {
        let raw = path_request(r, sample, tape, i)?;
        let generated = owner::evaluate_economic_with_measure(&raw, CreditMeasure::Pricing)?;
        let cash = economic_cash(generated.carry())?;
        let mut points = Vec::new();
        for date in &r.policy.exercise_dates {
            points.push(compiled_point_features(
                r,
                &raw,
                generated.carry(),
                tape,
                i,
                date,
            )?);
        }
        paths.push(Path { cash, points });
    }
    Ok(paths)
}
pub(crate) fn compiled_point_features(
    r: &Request,
    raw: &owner::Request,
    carry: &portfolio_model_core::credit_path_income::ResultData,
    tape: &Tape,
    path_index: usize,
    date: &term::exercise::ExerciseDate,
) -> Result<Point, String> {
    let asof = r.source.source.current.asof_ordinal;
    let opening = r.source.source.current.opening_principal;
    let t = f64::from(date.ordinal - asof) / 365.;
    let (state, principal, legal) = exercise::state(raw, carry, t, date.timing)?;
    let step = r
        .time_ordinals
        .binary_search(&date.ordinal)
        .map_err(|_| "generated decision grid absent")?;
    let mut features = tape.at(path_index, step).to_vec();
    features.extend([
        principal / opening,
        legal / opening,
        if state == CreditState::Watch { 1. } else { 0. },
        if state == CreditState::Delinquent {
            1.
        } else {
            0.
        },
    ]);
    if !r.hazard_loadings.is_empty() {
        let interval = r
            .source
            .credit
            .intervals
            .iter()
            .position(|p| t < p.end_year)
            .unwrap_or(r.source.credit.intervals.len() - 1);
        if r.hazard_loadings
            .iter()
            .any(|h| h.interval_index == interval && h.betas.iter().any(|b| *b != 0.))
        {
            let start = credit_start(r, interval)?;
            let anchor = r
                .time_ordinals
                .binary_search(&start)
                .map_err(|_| "observable current hazard anchor absent")?;
            features.extend_from_slice(tape.at(path_index, anchor));
        } else {
            features.extend(vec![0.; tape.d]);
        }
    }
    for period in &raw.source.current.periods {
        if let Coupon::Indexed { observations, .. } = &period.coupon {
            for observation in observations {
                features.push(
                    if observation.fixing_ordinal <= date.ordinal
                        && observation.selection_cutoff_unix_seconds <= utc(date.ordinal)
                    {
                        observation.rate
                    } else {
                        0.
                    },
                );
            }
        }
    }
    let point = exercise::point(carry, raw.credit.paths[0].path_key, t, date.timing)?;
    for period in &raw.source.current.periods {
        let coupon = point.canonical_legal_coupons.iter().find(|p| {
            p.coupon_id == period.period_id
                && exercise::coupon_entitlement_observed(period, date.ordinal, date.timing)
        });
        features.extend(coupon.map_or([0., 0.], |p| {
            [
                p.opening_entitlement_principal / opening,
                p.legal_remaining / opening,
            ]
        }));
    }
    Ok(Point {
        features,
        state,
        principal,
        legal,
        surviving_rights: exercise::surviving_contractual_rights(
            raw,
            carry,
            date.ordinal,
            date.timing,
        )?,
    })
}
fn basis(features: &[f64], means: &[f64], scales: &[f64], active: &[usize]) -> Vec<f64> {
    let x: Vec<_> = active
        .iter()
        .map(|&i| (features[i] - means[i]) / scales[i])
        .collect();
    let mut b = vec![1.];
    b.extend_from_slice(&x);
    for i in 0..x.len() {
        for j in i..x.len() {
            b.push(x[i] * x[j]);
        }
    }
    b
}
fn fit(ordinal: i32, rows: &[&Point], targets: &[f64]) -> Result<Regression, String> {
    check()?;
    let features = rows
        .first()
        .ok_or("no live training state for exercise regression")?
        .features
        .len();
    let mut means = vec![0.; features];
    let mut scales = vec![0.; features];
    for row in rows {
        for (i, x) in row.features.iter().enumerate() {
            means[i] += x / rows.len() as f64;
        }
    }
    for row in rows {
        for (i, x) in row.features.iter().enumerate() {
            scales[i] += (x - means[i]).powi(2) / rows.len() as f64;
        }
    }
    let mut active_features = Vec::new();
    for (i, scale) in scales.iter_mut().enumerate() {
        *scale = scale.sqrt();
        if *scale <= 8. * f64::EPSILON * means[i].abs().max(1.) {
            *scale = 1.;
        } else {
            active_features.push(i);
        }
    }
    let active = active_features.len();
    let columns = 1 + active + active * (active + 1) / 2;
    if rows.len() < columns * 2 {
        return Err("insufficient live training paths for quadratic continuation fit".into());
    }
    let data: Vec<_> = rows
        .iter()
        .flat_map(|p| basis(&p.features, &means, &scales, &active_features))
        .collect();
    let decomposition = DMatrix::from_row_slice(rows.len(), columns, &data).svd(true, true);
    let cutoff = f64::EPSILON * rows.len().max(columns) as f64 * decomposition.singular_values[0];
    let rank = decomposition
        .singular_values
        .iter()
        .filter(|v| **v > cutoff)
        .count();
    let coefficients = decomposition
        .solve(&DVector::from_column_slice(targets), cutoff)
        .map_err(|e| e.to_string())?;
    let mut observed_live_states = Vec::new();
    for row in rows {
        if !observed_live_states.contains(&row.state) {
            observed_live_states.push(row.state);
        }
    }
    Ok(Regression {
        ordinal,
        means,
        scales,
        active_features,
        coefficients: coefficients.iter().copied().collect(),
        observations: rows.len(),
        rank,
        singular_values: decomposition.singular_values.iter().copied().collect(),
        observed_live_states,
    })
}
fn forecast(regression: &Regression, point: &Point) -> Result<f64, String> {
    if !regression.observed_live_states.contains(&point.state) {
        return Err("evaluation live credit state lacks training coverage".into());
    }
    for (i, &value) in point.features.iter().enumerate() {
        if !regression.active_features.contains(&i)
            && (value - regression.means[i]).abs() > 1e-9 + 1e-12 * regression.means[i].abs()
        {
            return Err("evaluation contractual state lacks varying training coverage".into());
        }
    }
    amount(
        basis(
            &point.features,
            &regression.means,
            &regression.scales,
            &regression.active_features,
        )
        .iter()
        .zip(&regression.coefficients)
        .map(|(x, c)| x * c)
        .sum(),
    )
}

fn targets(r: &Request, paths: &[Path], index: usize, next: &[f64]) -> Result<Vec<f64>, String> {
    let date = &r.policy.exercise_dates[index];
    let asof = r.source.source.current.asof_ordinal;
    let t = f64::from(date.ordinal - asof) / 365.;
    let discount = df(r, t)?;
    let end = r
        .policy
        .exercise_dates
        .get(index + 1)
        .map(|d| (f64::from(d.ordinal - asof) / 365., d.timing));
    paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut value = if let Some((year, _)) = end {
                next[i] * df(r, year)? / discount
            } else {
                0.
            };
            for cash in &p.cash {
                if exercise::cash_after(cash.year, t, date.timing)
                    && end
                        .is_none_or(|(year, timing)| exercise::cash_before(cash.year, year, timing))
                {
                    value = amount(value + cash.total()? * df(r, cash.year)? / discount)?;
                }
            }
            Ok(value)
        })
        .collect()
}
fn decide(
    r: &Request,
    index: usize,
    p: &Point,
    continuation: f64,
    _cash: &[EconomicCash],
) -> Result<(Decision, f64), String> {
    if !live(p.state) {
        return Ok((Decision::Continue, continuation));
    }
    let date = &r.policy.exercise_dates[index];
    let rights = p.principal == 0. && p.surviving_rights;
    if rights && r.policy.rights_only_termination.is_none() {
        return Err("generated rights-only termination requires reviewed owner".into());
    }
    if p.principal == 0. && !rights {
        return Ok((Decision::Continue, continuation));
    }
    exercise::select(
        date,
        r.policy.simultaneous_priority,
        continuation,
        p.principal,
        p.legal,
    )
}
fn root_values(r: &Request, paths: &[Path], next: &[f64]) -> Result<Vec<f64>, String> {
    let first = r.policy.exercise_dates.first().map(|d| {
        (
            f64::from(d.ordinal - r.source.source.current.asof_ordinal) / 365.,
            d.timing,
        )
    });
    paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut value = if let Some((year, _)) = first {
                next[i] * df(r, year)?
            } else {
                0.
            };
            for c in &p.cash {
                if c.year > 0.
                    && first
                        .is_none_or(|(year, timing)| exercise::cash_before(c.year, year, timing))
                {
                    value = amount(value + c.total()? * df(r, c.year)?)?;
                }
            }
            Ok(value)
        })
        .collect()
}
/// Sealed native training owner. Caller JSON cannot fabricate fitted policies.
#[derive(Serialize)]
pub struct NativeTrainedPolicy {
    request: Request,
    regressions: Vec<Regression>,
    training_value: f64,
}
impl NativeTrainedPolicy {
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn regressions(&self) -> &[Regression] {
        &self.regressions
    }
    pub fn training_value(&self) -> f64 {
        self.training_value
    }
}
pub(crate) fn fitted_decision(
    policy: &NativeTrainedPolicy,
    index: usize,
    point: &Point,
    cash: &[EconomicCash],
) -> Result<(Decision, f64), String> {
    let continuation = if live(point.state) {
        forecast(
            policy
                .regressions
                .get(index)
                .ok_or("native trained date absent")?,
            point,
        )?
    } else {
        0.
    };
    decide(&policy.request, index, point, continuation, cash)
}
pub fn fit_training(r: &Request) -> Result<NativeTrainedPolicy, String> {
    admit(r)?;
    let train_tape = generate_tape(r, &r.training)?;
    let training = collect(r, &r.training, &train_tape)?;
    fit_paths(r, &training)
}
fn fit_paths(r: &Request, training: &[Path]) -> Result<NativeTrainedPolicy, String> {
    let mut train_values = vec![0.; training.len()];
    let mut regressions = Vec::new();
    for index in (0..r.policy.exercise_dates.len()).rev() {
        let target = targets(r, training, index, &train_values)?;
        let ids: Vec<_> = training
            .iter()
            .enumerate()
            .filter(|(_, p)| live(p.points[index].state))
            .map(|(i, _)| i)
            .collect();
        if ids.is_empty() {
            return Err(
                "exercise date has no live training coverage; no invented continuation".into(),
            );
        }
        let rows: Vec<_> = ids.iter().map(|i| &training[*i].points[index]).collect();
        let responses: Vec<_> = ids.iter().map(|i| target[*i]).collect();
        let regression = fit(r.policy.exercise_dates[index].ordinal, &rows, &responses)?;
        for (i, p) in training.iter().enumerate() {
            let continuation = if live(p.points[index].state) {
                forecast(&regression, &p.points[index])?
            } else {
                target[i]
            };
            let (decision, exercise) = decide(r, index, &p.points[index], continuation, &p.cash)?;
            train_values[i] = if decision == Decision::Continue {
                target[i]
            } else {
                exercise
            };
        }
        regressions.push(regression);
    }
    regressions.sort_by_key(|v| v.ordinal);
    let train_root = root_values(r, training, &train_values)?;
    let training_value = train_root.iter().sum::<f64>() / train_root.len() as f64;
    Ok(NativeTrainedPolicy {
        request: r.clone(),
        regressions,
        training_value,
    })
}
pub fn analyze(r: &Request) -> Result<ResultData, String> {
    let (work, bytes) = admit(r)?;
    let trained = fit_training(r)?;
    let training_value = trained.training_value;
    let eval_tape = generate_tape(r, &r.evaluation)?;
    let evaluation = collect(r, &r.evaluation, &eval_tape)?;
    let mut eval_values = vec![0.; evaluation.len()];
    let mut decisions =
        vec![vec![Decision::Continue; r.policy.exercise_dates.len()]; evaluation.len()];
    for index in (0..r.policy.exercise_dates.len()).rev() {
        let target = targets(r, &evaluation, index, &eval_values)?;
        for (i, p) in evaluation.iter().enumerate() {
            let (decision, exercise) = fitted_decision(&trained, index, &p.points[index], &p.cash)?;
            decisions[i][index] = decision;
            eval_values[i] = if decision == Decision::Continue {
                target[i]
            } else {
                exercise
            };
        }
    }
    let values = root_values(r, &evaluation, &eval_values)?;
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let half = values.len() / 2;
    let clusters: Vec<_> = (0..half)
        .map(|i| (values[i] + values[i + half]) * 0.5)
        .collect();
    let variance = clusters.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (half - 1) as f64;
    let mut outcomes = Vec::new();
    for (i, path) in evaluation.iter().enumerate() {
        check()?;
        let first = decisions[i].iter().position(|d| *d != Decision::Continue);
        let raw = path_request(r, &r.evaluation, &eval_tape, i)?;
        let stops = if let Some(index) = first {
            let date = &r.policy.exercise_dates[index];
            let action = decisions[i][index];
            let (kind, factor, fee) = match action {
                Decision::Call => (
                    owner::NativeStopAction::Call,
                    date.call_clean_price_factor
                        .ok_or("generated call absent")?,
                    date.call_fee,
                ),
                Decision::Put => (
                    owner::NativeStopAction::Put,
                    date.put_clean_price_factor.ok_or("generated put absent")?,
                    date.put_fee,
                ),
                Decision::Continue => unreachable!(),
            };
            vec![owner::NativeStop::from_compiled_policy_with_timing(
                i as u64 + 1,
                format!("generated-exercise:{}:{}", r.identity.id, date.ordinal),
                date.ordinal,
                kind,
                factor,
                fee,
                r.policy.identity.clone(),
                match date.timing {
                    ExerciseTiming::BeforeScheduledCash => {
                        owner::NativeStopTiming::BeforeScheduledCash
                    }
                    ExerciseTiming::AfterScheduledCash => {
                        owner::NativeStopTiming::AfterScheduledCash
                    }
                },
                if path.points[index].principal == 0. {
                    r.policy
                        .rights_only_termination
                        .as_ref()
                        .map(|p| p.review.clone())
                } else {
                    None
                },
            )?]
        } else {
            Vec::new()
        };
        let stopped = owner::evaluate_economic_with_stops(&raw, CreditMeasure::Pricing, &stops)?;
        let mut replayed = 0.;
        for c in economic_cash(stopped.carry())? {
            if c.year > 0. {
                replayed = amount(replayed + c.total()? * df(r, c.year)?)?;
            }
        }
        if (replayed - values[i]).abs() > 1e-9 + 1e-12 * replayed.abs() {
            return Err(
                "generated stopped native cash differs from retained LSM policy replay".into(),
            );
        }
        outcomes.push(PathOutcome {
            path_key: i as u64 + 1,
            value: values[i],
            first_exercise_ordinal: first.map(|j| r.policy.exercise_dates[j].ordinal),
            first_exercise_decision: first.map_or(Decision::Continue, |j| decisions[i][j]),
            full_native_cash_replayed: true,
        });
    }
    let result = ResultData { schema: SCHEMA, raw_source: r.clone(), admitted_work_upper: work, retained_bytes_upper: bytes,
        regressions: trained.regressions, training_value, evaluation_value: mean, evaluation_standard_error: (variance / half as f64).sqrt(), evaluation_clusters: half,
        evaluation_paths: outcomes, generated_joint_market_credit: true, evaluation_outcomes_used_for_fit: false, finite_distribution: false,
        convergence_established: false, physical_journal_eligible: false, empirical_validation: false,
        disclosure: "Supplied observable Gaussian OU law and piecewise-start loglinear credit hazards; deterministic supplied collateral discount. Quadratic LSM fit uses training only, fresh evaluation native cash is replayed. Sampling diagnostics do not establish model/basis convergence, observed accuracy, no-arbitrage calibration, or journal adoption." };
    strict_json::serialized_size(&result, 32 << 20)?;
    check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        serde_json::from_slice(include_bytes!(
            "../../../../../../examples/models/commercial-generated-exercise-1.json"
        ))
        .unwrap()
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
    #[test]
    fn generated_gaussian_call_matches_exact_normal_integral_on_fresh_paths() {
        let mut r = request();
        let sigma = r.annual_covariance[0].sqrt();
        let exact = 100.
            - 100. * 15. / 365. * sigma * (15_f64 / 365.).sqrt()
                / (2. * std::f64::consts::PI).sqrt();
        for n in [128, 256, 512] {
            r.training.paths = n;
            r.evaluation.paths = n;
            let out = analyze(&r).unwrap();
            assert!(
                (out.evaluation_value - exact).abs() <= 5. * out.evaluation_standard_error + 1e-9
            );
            assert_eq!(out.evaluation_clusters, n / 2);
            assert!(out
                .evaluation_paths
                .iter()
                .all(|p| p.full_native_cash_replayed));
            assert!(!out.evaluation_outcomes_used_for_fit);
            assert!(!out.finite_distribution);
            assert!(out.evaluation_value <= 100.);
            assert!(out.regressions[0].rank >= 2);
        }
    }
    #[test]
    fn evaluation_seed_and_outcomes_cannot_change_trained_policy() {
        let r = request();
        let first = analyze(&r).unwrap();
        let mut other = r.clone();
        other.evaluation.market_seed = vec![19];
        other.evaluation.credit_seed += 1;
        let second = analyze(&other).unwrap();
        assert_eq!(
            strict_json::encode(&first.regressions, 1 << 20).unwrap(),
            strict_json::encode(&second.regressions, 1 << 20).unwrap()
        );
        assert_eq!(
            first.training_value.to_bits(),
            second.training_value.to_bits()
        );
        let repeated = analyze(&r).unwrap();
        assert_eq!(
            strict_json::encode(&first, 32 << 20).unwrap(),
            strict_json::encode(&repeated, 32 << 20).unwrap()
        );
        let mut same = r;
        same.evaluation.market_seed = same.training.market_seed.clone();
        assert!(admit(&same).unwrap_err().contains("must differ"));
    }
    #[test]
    fn option_free_zero_volatility_is_native_cash_identity_and_joint_credit_is_generated() {
        let mut r = request();
        r.annual_covariance[0] = 0.;
        r.policy.exercise_dates.clear();
        let out = analyze(&r).unwrap();
        near(out.evaluation_value, 100.);
        near(out.evaluation_standard_error, 0.);
        assert!(out.regressions.is_empty());
        let mut joint = request();
        joint.policy.exercise_dates[0].call_clean_price_factor = Some(10.);
        joint.source.credit.intervals[1].hazards_per_year[0][3] = 3.;
        joint.hazard_loadings.push(HazardLoading {
            interval_index: 1,
            origin_index: 0,
            destination_index: 3,
            betas: vec![1.],
        });
        joint.source.credit.intervals[1].loss_given_default = [0.4; 3];
        joint.source.credit.intervals[1].interest_recovery_fraction = [0.5; 3];
        // Positive source coupon baseline keeps the default legal claim one-sided.
        for source in [
            &mut joint.source.source.original,
            &mut joint.source.source.current,
        ] {
            if let Coupon::Indexed { observations, .. } = &mut source.periods[1].coupon {
                observations[0].rate = 0.1;
            }
        }
        let credit = analyze(&joint).unwrap();
        assert!(credit.generated_joint_market_credit);
        assert!(credit.evaluation_value < 100.);
        assert!(credit
            .evaluation_paths
            .iter()
            .all(|p| p.full_native_cash_replayed));
    }
    #[test]
    fn incomplete_future_mapping_and_budget_and_unobservable_states_reject() {
        let mut r = request();
        r.coupon_loadings.clear();
        assert!(admit(&r).is_err());
        let mut r = request();
        r.drivers[0].observation_policy = "future_hidden".into();
        assert!(admit(&r).is_err());
        let mut r = request();
        r.training.paths = 4096;
        r.evaluation.paths = 4096;
        assert!(admit(&r).is_err());
    }
    #[test]
    fn gaussian_put_bound_and_simultaneous_exercise_priority_replay() {
        let mut r = request();
        r.policy.exercise_dates[0].call_clean_price_factor = None;
        r.policy.exercise_dates[0].put_clean_price_factor = Some(1.);
        let out = analyze(&r).unwrap();
        let exact = 100.
            + 100. * 15. / 365. * 0.2 * (15_f64 / 365.).sqrt() / (2. * std::f64::consts::PI).sqrt();
        assert!((out.evaluation_value - exact).abs() <= 5. * out.evaluation_standard_error + 1e-9);
        assert!(out.evaluation_value >= 100.);
        r.policy.exercise_dates[0].call_clean_price_factor = Some(1.);
        for priority in [
            term::exercise::SimultaneousPriority::Issuer,
            term::exercise::SimultaneousPriority::Holder,
        ] {
            r.policy.simultaneous_priority = priority;
            near(analyze(&r).unwrap().evaluation_value, 100.);
        }
    }
    #[test]
    fn economically_valid_negative_dv01_retains_base_oas_and_shared_streams() {
        let mut r = request();
        r.annual_covariance[0] = 0.;
        r.policy.exercise_dates.clear();
        r.fixed_base_oas = 0.01337;
        let base_bits = r.fixed_base_oas.to_bits();
        let mut values = Vec::new();
        for bump in [-0.0001, 0.0001] {
            r.parallel_bump = bump;
            if let Coupon::Indexed { observations, .. } =
                &mut r.source.source.current.periods[1].coupon
            {
                observations[0].rate = 0.1 + 3. * bump;
            }
            let out = analyze(&r).unwrap();
            assert_eq!(out.raw_source.fixed_base_oas.to_bits(), base_bits);
            let expected = (100. + 100. * (0.1 + 3. * bump) * 15. / 365.)
                * (-(r.fixed_base_oas + bump) * 35. / 365.).exp();
            near(out.evaluation_value, expected);
            values.push(out.evaluation_value);
        }
        assert!((values[0] - values[1]) / 2. < 0.);
    }
    #[test]
    fn rights_only_policy_cannot_observe_future_physical_cash() {
        let mut r = request();
        r.policy.rights_only_termination = Some(exercise::RightsOnlyTermination {
            review: r.policy.identity.clone(),
            disposition: "settle_earned_cancel_future-1".into(),
        });
        let observed = Point {
            features: Vec::new(),
            state: CreditState::Performing,
            principal: 0.,
            legal: 0.,
            surviving_rights: true,
        };
        // Same observed future coupon right and fitted Q continuation. One
        // Physical outcome later pays zero; another pays a positive coupon.
        let future = [EconomicCash {
            event_id: "future".into(),
            kind: "repayment".into(),
            year: 1.,
            principal: 0.,
            interest: 20.,
            fees: 0.,
        }];
        for cash in [&[][..], &future[..]] {
            let (decision, value) = decide(&r, 0, &observed, 0.5, cash).unwrap();
            assert_eq!(decision, Decision::Call);
            assert_eq!(value, 0.);
        }
        r.policy.rights_only_termination = None;
        for cash in [&[][..], &future[..]] {
            assert!(decide(&r, 0, &observed, 0.5, cash).is_err());
        }
    }
    #[test]
    fn future_coupon_entitlement_metadata_is_hidden_until_observed() {
        let mut r = request();
        r.policy.exercise_dates[0].ordinal -= 1;
        r.time_ordinals.push(r.policy.exercise_dates[0].ordinal);
        r.time_ordinals.sort_unstable();
        let tape = generate_tape(&r, &r.evaluation).unwrap();
        let paths = collect(&r, &r.evaluation, &tape).unwrap();
        assert!(
            r.source
                .source
                .current
                .periods
                .last()
                .unwrap()
                .start_ordinal
                > r.policy.exercise_dates[0].ordinal
        );
        for path in paths {
            let features = &path.points[0].features;
            assert_eq!(&features[features.len() - 2..], &[0., 0.]);
        }
    }
}
