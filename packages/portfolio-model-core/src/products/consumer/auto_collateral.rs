//! Collateral-linked auto loss given default and equity-driven competing hazards.
//!
//! The collateral value follows a supplied depreciation curve and a supplied used-vehicle index.
//! Loss given default and recovery timing come from that value and a reviewed recovery policy.
//! Borrower equity is the one derived hazard covariate; every other driver is supplied. Hazards are
//! projected by the unchanged `credit-covariate-1` estimator and evaluated by `credit-model-1`.
use crate::accounting::credit::expected::Identity;
use crate::models::credit::covariate::{
    self, CovariateProjectionRequest, DriverInterval, DriverKind, FittedModel, ProjectionUse,
};
use crate::models::credit::transition::{
    self as credit, CreditMeasure, CreditPeriod, CreditRequest, CreditResult, ExposureAtDefault,
    RecoveryPoint,
};
use crate::models::observation::ObservationTime;
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "consumer-auto-collateral-1";
pub const MAX_INTERVALS: usize = 240;
pub const MAX_KNOTS: usize = 64;
const SECONDS_PER_YEAR: f64 = 31_536_000.;
const MAX_AMOUNT: f64 = 1e15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleSegment {
    New,
    Used,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepreciationKnot {
    /// Months since origination, ACT/365 years times twelve.
    pub age_months: f64,
    /// Value retained relative to the origination value, before any index movement.
    pub retained_fraction: f64,
}

/// Supplied segment depreciation curve. Values between knots are linear; none is extrapolated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepreciationCurve {
    pub review: Identity,
    pub segment: VehicleSegment,
    pub vehicle_class: Option<String>,
    pub knots: Vec<DepreciationKnot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collateral {
    pub origination_value: f64,
    pub origination: ObservationTime,
    pub segment: VehicleSegment,
    pub vehicle_class: Option<String>,
    pub depreciation: DepreciationCurve,
    /// Used-vehicle index level observed at origination; later levels are relative to it.
    pub index_at_origination: f64,
    /// Supplied value reductions for mileage and condition, each in [0, 1).
    pub mileage_haircut: f64,
    pub condition_haircut: f64,
    pub haircut_review: Identity,
}

/// Reviewed recovery policy. Every term is supplied or separately fitted, never inferred here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPolicy {
    pub review: Identity,
    /// Share of collateral value realised at sale, in (0, 1].
    pub recovery_haircut: f64,
    pub repossession_cost: f64,
    pub remarketing_cost: f64,
    /// Days from default recognition to the collateral sale cash.
    pub recovery_lag_days: u32,
    /// Share of the deficiency balance eventually collected, in [0, 1].
    pub deficiency_recovery_fraction: f64,
    /// Days from default recognition to the deficiency collection.
    pub deficiency_lag_days: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LossGivenDefault {
    pub exposure: f64,
    pub collateral_value: f64,
    pub gross_sale_proceeds: f64,
    pub costs: f64,
    /// Sale proceeds net of costs applied to the loan. Negative when costs exceed proceeds.
    pub collateral_recovery: f64,
    /// `max(0, exposure - collateral value x recovery haircut + costs)`.
    pub deficiency_balance: f64,
    pub deficiency_recovery: f64,
    pub loss: f64,
    /// Loss over exposure. `None` for a zero exposure.
    pub lgd: Option<f64>,
    /// Net proceeds above the exposure belong to the borrower and are not lender income.
    pub surplus_to_borrower: f64,
}

/// `lower < value <= upper`; false for NaN.
fn positive(value: f64, upper: f64) -> bool {
    value > 0. && value <= upper
}

fn seconds(t: ObservationTime) -> Result<i64, String> {
    t.seconds()
}

fn text(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{name} must be 1..256 control-free bytes"));
    }
    Ok(())
}

fn identity(value: &Identity, name: &str) -> Result<(), String> {
    text(&value.id, name)?;
    text(&value.revision, name)
}

fn validate_collateral(c: &Collateral) -> Result<(), String> {
    identity(&c.depreciation.review, "depreciation review")?;
    identity(&c.haircut_review, "haircut review")?;
    seconds(c.origination)?;
    let valid = positive(c.origination_value, MAX_AMOUNT)
        && positive(c.index_at_origination, 1e9)
        && (0. ..1.).contains(&c.mileage_haircut)
        && (0. ..1.).contains(&c.condition_haircut);
    if !valid {
        return Err(
            "collateral value, origination index and haircuts are outside their domains".into(),
        );
    }
    if c.depreciation.segment != c.segment || c.depreciation.vehicle_class != c.vehicle_class {
        return Err("depreciation curve is for a different vehicle segment or class".into());
    }
    if let Some(class) = &c.vehicle_class {
        text(class, "vehicle class")?;
    }
    let knots = &c.depreciation.knots;
    let ordered = (2..=MAX_KNOTS).contains(&knots.len())
        && knots[0].age_months == 0.
        && knots.windows(2).all(|w| w[1].age_months > w[0].age_months)
        && knots
            .iter()
            .all(|k| k.age_months.is_finite() && positive(k.retained_fraction, 2.));
    if !ordered {
        return Err("depreciation curve needs 2..64 knots from age zero with increasing ages and positive retained fractions".into());
    }
    Ok(())
}

fn validate_policy(p: &RecoveryPolicy) -> Result<(), String> {
    identity(&p.review, "recovery review")?;
    let valid = positive(p.recovery_haircut, 1.)
        && (0. ..=MAX_AMOUNT).contains(&p.repossession_cost)
        && (0. ..=MAX_AMOUNT).contains(&p.remarketing_cost)
        && (0. ..=1.).contains(&p.deficiency_recovery_fraction)
        && p.recovery_lag_days <= 36_600
        && p.deficiency_lag_days <= 36_600;
    if !valid {
        return Err(
            "recovery haircut, costs, deficiency fraction or lags are outside their domains".into(),
        );
    }
    Ok(())
}

/// Age in months of the collateral at `at`: ACT/365 years since origination times twelve.
pub fn age_months(collateral: &Collateral, at: ObservationTime) -> Result<f64, String> {
    let elapsed = seconds(at)? - seconds(collateral.origination)?;
    if elapsed < 0 {
        return Err("collateral cannot be valued before origination".into());
    }
    Ok(elapsed as f64 / SECONDS_PER_YEAR * 12.)
}

/// Linear interpolation on the supplied curve. Ages beyond the last knot reject.
pub fn retained_fraction(curve: &DepreciationCurve, age_months: f64) -> Result<f64, String> {
    let knots = &curve.knots;
    let covered = knots
        .last()
        .is_some_and(|k| (0. ..=k.age_months).contains(&age_months));
    if !covered {
        return Err("collateral age is outside the supplied depreciation curve".into());
    }
    let upper = knots
        .iter()
        .position(|k| k.age_months >= age_months)
        .expect("age within curve");
    if upper == 0 {
        return Ok(knots[0].retained_fraction);
    }
    let (a, b) = (knots[upper - 1], knots[upper]);
    let weight = (age_months - a.age_months) / (b.age_months - a.age_months);
    Ok(a.retained_fraction + weight * (b.retained_fraction - a.retained_fraction))
}

/// Origination value x depreciation x index relative to origination x mileage and condition haircuts.
pub fn collateral_value(
    collateral: &Collateral,
    at: ObservationTime,
    used_vehicle_index: f64,
) -> Result<f64, String> {
    validate_collateral(collateral)?;
    if !positive(used_vehicle_index, 1e9) {
        return Err("used-vehicle index level must be positive and finite".into());
    }
    let retained = retained_fraction(&collateral.depreciation, age_months(collateral, at)?)?;
    Ok(collateral.origination_value
        * retained
        * (used_vehicle_index / collateral.index_at_origination)
        * (1. - collateral.mileage_haircut)
        * (1. - collateral.condition_haircut))
}

/// Loss on a defaulted exposure. Recovery lags are cash timing only and do not enter the loss.
pub fn loss_given_default(
    exposure: f64,
    collateral_value: f64,
    policy: &RecoveryPolicy,
) -> Result<LossGivenDefault, String> {
    validate_policy(policy)?;
    if !(0. ..=MAX_AMOUNT).contains(&exposure) || !(0. ..=MAX_AMOUNT).contains(&collateral_value) {
        return Err("exposure and collateral value must be finite and nonnegative".into());
    }
    let gross = collateral_value * policy.recovery_haircut;
    let costs = policy.repossession_cost + policy.remarketing_cost;
    let deficiency = (exposure - gross + costs).max(0.);
    let deficiency_recovery = policy.deficiency_recovery_fraction * deficiency;
    let loss = deficiency - deficiency_recovery;
    Ok(LossGivenDefault {
        exposure,
        collateral_value,
        gross_sale_proceeds: gross,
        costs,
        collateral_recovery: exposure - deficiency,
        deficiency_balance: deficiency,
        deficiency_recovery,
        loss,
        lgd: (exposure > 0.).then(|| loss / exposure),
        surplus_to_borrower: (gross - costs - exposure).max(0.),
    })
}

/// Borrower equity as a hazard covariate: collateral value over balance, capped by declared policy.
pub fn equity_ratio(collateral_value: f64, balance: f64, cap: f64) -> Result<f64, String> {
    let valid = (0. ..=MAX_AMOUNT).contains(&collateral_value)
        && positive(balance, MAX_AMOUNT)
        && positive(cap, 1e6);
    if !valid {
        return Err(
            "equity needs a nonnegative value, a positive balance and a positive cap".into(),
        );
    }
    Ok((collateral_value / balance).min(cap))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoInterval {
    pub interval_id: String,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub available_time: ObservationTime,
    pub kind: DriverKind,
    pub source: Identity,
    /// Scheduled principal over this interval from the installment schedule; exposure at default.
    pub balance: f64,
    /// Used-vehicle index level for this interval, from the supplied driver tape.
    pub used_vehicle_index: f64,
    /// One value per fitted column in column order. The equity column's entry must be zero:
    /// it is derived here from collateral value and balance.
    pub covariates: Vec<f64>,
    pub term_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionRequest {
    pub schema: String,
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    /// Fitted `credit-covariate-1` model for the auto population.
    pub model: FittedModel,
    pub model_revision: Identity,
    pub review: Identity,
    pub use_policy: ProjectionUse,
    pub position_id: String,
    pub scenario_id: String,
    pub timeout_ms: u64,
    pub initial_live_probability: [f64; 3],
    pub collateral: Collateral,
    pub recovery: RecoveryPolicy,
    /// Fitted column that carries borrower equity, if the model uses it.
    pub equity_column: Option<String>,
    /// Declared cap on the equity ratio; required because equity is unbounded near payoff.
    pub equity_cap: f64,
    pub intervals: Vec<AutoInterval>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntervalOutcome {
    pub interval_id: String,
    pub end_year: f64,
    pub balance: f64,
    /// Value at interval start; with the balance it gives the equity covariate.
    pub collateral_value_start: f64,
    pub equity: f64,
    /// Value at interval end, where the credit model recognises default.
    pub collateral_value_at_default: f64,
    pub hazards_per_year: [[f64; 5]; 3],
    pub loss: LossGivenDefault,
    pub recovery_distribution: Vec<RecoveryPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectionResult {
    pub schema: String,
    pub position_id: String,
    pub scenario_id: String,
    pub coefficient_identity_sha256: String,
    pub adoption_output: bool,
    pub analytical_only: bool,
    /// Always false: observed repossession and recovery histories with holdouts are required.
    pub empirical_claim: bool,
    pub intervals: Vec<IntervalOutcome>,
    /// The assembled `credit-model-1` request: term LGD, recovery timing and hazards.
    pub credit_request: CreditRequest,
    pub credit: CreditResult,
}

fn recovery_points(
    loss: &LossGivenDefault,
    policy: &RecoveryPolicy,
) -> Result<Vec<RecoveryPoint>, String> {
    if loss.collateral_recovery < 0. {
        return Err("repossession and remarketing costs exceed sale proceeds; the excess is an expense the credit model cannot carry as recovery".into());
    }
    let lag = |days: u32| f64::from(days) / 365.;
    let total = loss.collateral_recovery + loss.deficiency_recovery;
    // The credit model needs weights that sum to one even when nothing is recovered.
    if total == 0. || loss.deficiency_recovery == 0. {
        return Ok(vec![RecoveryPoint {
            lag_years: lag(policy.recovery_lag_days),
            weight: 1.,
        }]);
    }
    if loss.collateral_recovery == 0. {
        return Ok(vec![RecoveryPoint {
            lag_years: lag(policy.deficiency_lag_days),
            weight: 1.,
        }]);
    }
    let first = loss.collateral_recovery / total;
    Ok(vec![
        RecoveryPoint {
            lag_years: lag(policy.recovery_lag_days),
            weight: first,
        },
        RecoveryPoint {
            lag_years: lag(policy.deficiency_lag_days),
            weight: 1. - first,
        },
    ])
}

pub fn project(request: &ProjectionRequest) -> Result<ProjectionResult, String> {
    project_with_check(request, &mut || Ok(()))
}

pub fn project_with_check(
    request: &ProjectionRequest,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ProjectionResult, String> {
    check()?;
    if request.schema != SCHEMA {
        return Err("unsupported auto collateral schema".into());
    }
    if request.intervals.is_empty() || request.intervals.len() > MAX_INTERVALS {
        return Err("auto collateral projection requires 1..240 intervals".into());
    }
    text(&request.monetary_unit, "monetary unit")?;
    validate_collateral(&request.collateral)?;
    validate_policy(&request.recovery)?;
    let equity_column = request
        .equity_column
        .as_deref()
        .map(|name| {
            request
                .model
                .columns
                .iter()
                .position(|c| c.name == name)
                .ok_or_else(|| format!("unknown equity column {name}"))
        })
        .transpose()?;
    let (origin, end) = (
        request.intervals[0].start,
        request.intervals[request.intervals.len() - 1].end,
    );
    let mut drivers = Vec::with_capacity(request.intervals.len());
    let mut derived = Vec::with_capacity(request.intervals.len());
    for interval in &request.intervals {
        check()?;
        if interval.covariates.len() != request.model.columns.len() {
            return Err("interval covariate count differs from the fitted columns".into());
        }
        if !positive(interval.balance, MAX_AMOUNT) {
            return Err(
                "interval balance must be positive; a repaid loan has no exposure interval".into(),
            );
        }
        let start_value = collateral_value(
            &request.collateral,
            interval.start,
            interval.used_vehicle_index,
        )?;
        let default_value = collateral_value(
            &request.collateral,
            interval.end,
            interval.used_vehicle_index,
        )?;
        let equity = equity_ratio(start_value, interval.balance, request.equity_cap)?;
        let mut covariates = interval.covariates.clone();
        if let Some(k) = equity_column {
            if covariates[k] != 0. {
                return Err("the equity column is derived; supply zero in its place".into());
            }
            covariates[k] = equity;
        }
        let loss = loss_given_default(interval.balance, default_value, &request.recovery)?;
        let recovery_distribution = recovery_points(&loss, &request.recovery)?;
        derived.push((
            start_value,
            equity,
            default_value,
            loss,
            recovery_distribution,
        ));
        drivers.push(DriverInterval {
            interval_id: interval.interval_id.clone(),
            start: interval.start,
            end: interval.end,
            available_time: interval.available_time,
            kind: interval.kind.clone(),
            source: interval.source.clone(),
            covariates,
            term_level: interval.term_level.clone(),
        });
    }
    let hazards = covariate::project_with_check(
        &CovariateProjectionRequest {
            schema: covariate::SCHEMA.into(),
            measure: request.measure,
            model: request.model.clone(),
            model_revision: request.model_revision.clone(),
            review: request.review.clone(),
            use_policy: request.use_policy.clone(),
            position_id: request.position_id.clone(),
            scenario_id: request.scenario_id.clone(),
            origin_time: origin,
            end_time: end,
            timeout_ms: request.timeout_ms,
            intervals: drivers,
        },
        check,
    )?;
    let (mut periods, mut intervals) = (Vec::new(), Vec::new());
    for (
        (interval, projected),
        (start_value, equity, default_value, loss, recovery_distribution),
    ) in request
        .intervals
        .iter()
        .zip(&hazards.intervals)
        .zip(derived)
    {
        let lgd = loss.lgd.expect("positive balance");
        let exposure = || ExposureAtDefault {
            drawn: interval.balance,
            undrawn: 0.,
            credit_conversion_factor: 0.,
        };
        periods.push(CreditPeriod {
            duration_years: (seconds(interval.end)? - seconds(interval.start)?) as f64
                / SECONDS_PER_YEAR,
            transition_hazards_per_year: projected.hazards_per_year,
            exposure_at_default: [exposure(), exposure(), exposure()],
            loss_given_default: [lgd; 3],
            recovery_distribution: recovery_distribution.clone(),
        });
        intervals.push(IntervalOutcome {
            interval_id: interval.interval_id.clone(),
            end_year: projected.end_year,
            balance: interval.balance,
            collateral_value_start: start_value,
            equity,
            collateral_value_at_default: default_value,
            hazards_per_year: projected.hazards_per_year,
            loss,
            recovery_distribution,
        });
    }
    let credit_request = CreditRequest {
        schema: credit::SCHEMA.into(),
        measure: request.measure,
        monetary_unit: request.monetary_unit.clone(),
        initial_live_probability: request.initial_live_probability,
        periods,
    };
    check()?;
    let credit = credit::evaluate(&credit_request)?;
    Ok(ProjectionResult {
        schema: SCHEMA.into(),
        position_id: request.position_id.clone(),
        scenario_id: request.scenario_id.clone(),
        coefficient_identity_sha256: hazards.coefficient_identity_sha256,
        adoption_output: hazards.adoption_output,
        analytical_only: hazards.analytical_only,
        empirical_claim: false,
        intervals,
        credit_request,
        credit,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitCause {
    Default,
    Prepaid,
}

/// First exit of a performing loan under one shared uniform pair.
///
/// The exposure threshold `-ln(1 - u_time)` is carried across intervals, so changed hazards move
/// the exit without a new draw; `u_cause` picks default with the default share of the hazard in the
/// exit interval. Models with performing-to-watch or -delinquent edges need the full path engine.
pub fn shared_draw_exit(
    intervals: &[IntervalOutcome],
    time_uniform: f64,
    cause_uniform: f64,
) -> Result<Option<(usize, ExitCause)>, String> {
    if !(0. ..1.).contains(&time_uniform) || !(0. ..1.).contains(&cause_uniform) {
        return Err("shared uniforms must lie in [0, 1)".into());
    }
    let threshold = -(-time_uniform).ln_1p();
    let (mut integrated, mut previous_year) = (0., 0.);
    for (index, interval) in intervals.iter().enumerate() {
        let row = interval.hazards_per_year[0];
        if row[1] != 0. || row[2] != 0. {
            return Err(
                "shared_draw_exit supports direct default and prepayment from performing only"
                    .into(),
            );
        }
        let total = row[3] + row[4];
        integrated += total * (interval.end_year - previous_year);
        previous_year = interval.end_year;
        if total > 0. && integrated > threshold {
            let cause = if cause_uniform < row[3] / total {
                ExitCause::Default
            } else {
                ExitCause::Prepaid
            };
            return Ok(Some((index, cause)));
        }
    }
    Ok(None)
}
