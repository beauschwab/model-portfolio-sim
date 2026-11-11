//! Fitted card payment, purchase and behavioural-state layer.
//!
//! This module turns observed statement panels into behaviour coefficients and turns those
//! coefficients into ordinary supplied `consumer-card-1` actions. The statement mechanics are
//! unchanged and remain the only owner of balances, interest, grace and default cash.
use crate::accounting::credit::expected::Identity;
use crate::models::credit::covariate::{
    self, Column, CovariateFitRequest, CovariateFitResult, FittedModel, OriginModel, Parameter,
    ResolvedRow,
};
use crate::models::credit::transition::CreditState;
use crate::models::observation::{DataKind, ObservationTime};
use crate::products::consumer::contracts::ConsumerState;
use crate::products::revolving::state::{
    self as card, BalanceClass, CardAction, CardRates, CardRequest, CardResult, CardStep,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "consumer-card-behaviour-1";
pub const MAX_PANEL_RECORDS: usize = 20_000;
pub const MAX_CYCLES: usize = 60;
pub const MAX_FRACTION_ITERATIONS: usize = 200;
/// Fitted fractions never request the last representable unit of the line.
const FRACTION_CAP: f64 = 1.0 - 1e-12;
const PUBLICATION_REFUSAL: &str =
    "card pricing publication requires an observed account panel with reviewed holdouts";

/// Statement behaviour, determined from an observed payment against its statement.
/// Slot order is fixed: it is also the origin order of the transition estimator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviourState {
    Transactor,
    Revolver,
    MinimumPayer,
}
pub const STATES: [BehaviourState; 3] = [
    BehaviourState::Transactor,
    BehaviourState::Revolver,
    BehaviourState::MinimumPayer,
];
impl BehaviourState {
    pub fn slot(self) -> usize {
        self as usize
    }
    /// The transition estimator is typed on three live origins; behaviour reuses those slots only.
    fn estimator_state(self) -> CreditState {
        [
            CreditState::Performing,
            CreditState::Watch,
            CreditState::Delinquent,
        ][self.slot()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationPolicy {
    /// A payment within this currency amount of the statement balance is a full payment.
    pub full_payment_tolerance: f64,
    /// A payment at or below minimum due plus this currency amount is a minimum payment.
    pub minimum_payment_tolerance: f64,
}

/// Classify one statement. No label is read: the class is a function of observed cash.
pub fn classify(
    statement_balance: f64,
    minimum_due: f64,
    payment: f64,
    policy: &ClassificationPolicy,
) -> Result<BehaviourState, String> {
    let values = [
        statement_balance,
        minimum_due,
        payment,
        policy.full_payment_tolerance,
        policy.minimum_payment_tolerance,
    ];
    if values.iter().any(|v| !v.is_finite() || *v < 0.)
        || minimum_due > statement_balance
        || payment > statement_balance + policy.full_payment_tolerance
    {
        return Err(
            "statement, minimum and payment must be finite, nonnegative and ordered".into(),
        );
    }
    Ok(
        if payment >= statement_balance - policy.full_payment_tolerance {
            BehaviourState::Transactor
        } else if payment <= minimum_due + policy.minimum_payment_tolerance {
            BehaviourState::MinimumPayer
        } else {
            BehaviourState::Revolver
        },
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelRecord {
    pub record_id: String,
    pub account_id: String,
    /// Statement close that fixed `statement_balance` and `minimum_due`.
    pub statement: ObservationTime,
    /// Next statement close. Records of one account must be contiguous.
    pub next_statement: ObservationTime,
    pub available_time: ObservationTime,
    pub statement_balance: f64,
    pub minimum_due: f64,
    /// Cash received against this statement by its due date.
    pub payment: f64,
    pub limit: f64,
    /// New purchase principal between this statement's payment and the next statement.
    pub purchases: f64,
    pub covariates: Vec<f64>,
    pub term_level: String,
    /// Principal outstanding at default when the account defaulted in this cycle.
    pub default_balance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FractionSpec {
    pub intercept: Parameter,
    pub coefficients: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FitRequest {
    pub schema: String,
    pub classification: ClassificationPolicy,
    /// `credit-covariate-1` request with no segments. Origins are behaviour slots
    /// (performing = transactor, watch = revolver, delinquent = minimum payer); default and
    /// prepaid edges are not behaviour states and reject.
    pub transition_template: CovariateFitRequest,
    /// Revolver payment above minimum due as a fraction of the statement balance above it.
    pub payment: FractionSpec,
    /// Purchases as a fraction of open-to-buy, one model per behaviour state.
    pub draw: [FractionSpec; 3],
    /// Reference statement this many cycles before default (3 to 6).
    pub ccf_lookback_cycles: u32,
    /// Reviewed credit conversion factors for states with no usable training default.
    pub supplied_ccf: [Option<f64>; 3],
    pub fraction_gradient_tolerance: f64,
    pub records: Vec<PanelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FractionModel {
    pub intercept: f64,
    pub coefficients: Vec<f64>,
    pub observations: usize,
    pub iterations: usize,
    pub gradient_norm: f64,
    pub converged: bool,
    pub full_rank: bool,
    pub active_bounds: Vec<String>,
}
impl FractionModel {
    pub fn fraction(&self, z: &[f64]) -> f64 {
        logistic(
            self.intercept
                + self
                    .coefficients
                    .iter()
                    .zip(z)
                    .map(|(b, x)| b * x)
                    .sum::<f64>(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CcfSource {
    FittedTrainingDefaults,
    SuppliedReviewedAssumption,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FittedCardBehaviour {
    pub schema: String,
    pub data_kind: DataKind,
    pub classification: ClassificationPolicy,
    pub columns: Vec<Column>,
    pub term_levels: Vec<String>,
    pub transition: FittedModel,
    pub payment: FractionModel,
    pub draw: [FractionModel; 3],
    pub ccf: [f64; 3],
    pub ccf_source: [CcfSource; 3],
    pub ccf_lookback_cycles: u32,
    pub numerical_eligible: bool,
    /// Synthetic panels are never adoption eligible.
    pub adoption_eligible: bool,
    /// Always false: this component cannot authorize card pricing publication.
    pub publication_eligible: bool,
    pub eligibility_reasons: Vec<String>,
    pub identity_sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FractionDiagnostics {
    pub observations: usize,
    pub mean_observed: f64,
    pub mean_predicted: f64,
    pub mean_absolute_error: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitDiagnostics {
    pub records: usize,
    pub state_counts: [usize; 3],
    pub payment: FractionDiagnostics,
    pub draw: [FractionDiagnostics; 3],
    pub defaults: usize,
    pub defaults_without_reference: usize,
    /// Records with no open-to-buy carry no purchase information and are not fitted.
    pub records_without_open_to_buy: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FitResult {
    pub schema: String,
    pub model: FittedCardBehaviour,
    pub transition_fit: CovariateFitResult,
    pub training: SplitDiagnostics,
    /// Chronological holdout. It enters no objective, scaling or selection.
    pub holdout: SplitDiagnostics,
}

fn logistic(eta: f64) -> f64 {
    if eta >= 0. {
        1. / (1. + (-eta).exp())
    } else {
        let e = eta.exp();
        e / (1. + e)
    }
}

fn seconds(t: ObservationTime) -> i64 {
    i64::from(t.ordinal) * 86_400 + i64::from(t.seconds_of_day)
}

fn transform(columns: &[Column], raw: &[f64]) -> Result<Vec<f64>, String> {
    if raw.len() != columns.len() {
        return Err("covariate count differs from declared columns".into());
    }
    columns
        .iter()
        .zip(raw)
        .map(|(c, x)| {
            if !x.is_finite() || *x < c.raw_lower || *x > c.raw_upper {
                Err(format!("covariate {} outside its declared domain", c.name))
            } else {
                Ok((x - c.offset) / c.scale)
            }
        })
        .collect()
}

/// Solve a small symmetric positive system. `None` when a pivot is not resolved.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    let scale = (0..n).map(|i| a[i][i].abs()).fold(0., f64::max);
    for i in 0..n {
        let pivot = (i..n).max_by(|x, y| a[*x][i].abs().total_cmp(&a[*y][i].abs()))?;
        if a[pivot][i].abs() <= scale * 1e-12 || a[pivot][i] == 0. {
            return None;
        }
        a.swap(i, pivot);
        b.swap(i, pivot);
        let (head, tail) = a.split_at_mut(i + 1);
        let (pivot_row, pivot_value) = (&head[i], b[i]);
        for (row, value) in tail.iter_mut().zip(&mut b[i + 1..]) {
            let m = row[i] / pivot_row[i];
            for (x, y) in row[i..].iter_mut().zip(&pivot_row[i..]) {
                *x -= m * y;
            }
            *value -= m * pivot_value;
        }
    }
    let mut x = vec![0.; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - (i + 1..n).map(|c| a[i][c] * x[c]).sum::<f64>()) / a[i][i];
    }
    Some(x)
}

/// Quasi-binomial fractional logit: deterministic Newton steps with halving inside the box.
fn fit_fraction(
    spec: &FractionSpec,
    names: &[String],
    rows: &[(Vec<f64>, f64)],
    tolerance: f64,
) -> Result<FractionModel, String> {
    let parameters: Vec<&Parameter> = std::iter::once(&spec.intercept)
        .chain(&spec.coefficients)
        .collect();
    let n = parameters.len();
    if n != names.len() + 1 {
        return Err("fraction coefficients must match declared columns".into());
    }
    for p in &parameters {
        if ![p.initial, p.lower, p.upper, p.ridge_weight, p.ridge_center]
            .iter()
            .all(|v| v.is_finite())
            || p.lower > p.initial
            || p.initial > p.upper
            || p.lower < -100.
            || p.upper > 100.
            || p.ridge_weight < 0.
        {
            return Err("invalid fraction parameter box or ridge".into());
        }
    }
    if rows.is_empty() {
        return Err(
            "fraction model has no training observations; supply a fixed box instead".into(),
        );
    }
    let objective = |theta: &[f64]| -> f64 {
        let mut value = 0.;
        for (z, y) in rows {
            let eta = theta[0] + z.iter().zip(&theta[1..]).map(|(x, b)| x * b).sum::<f64>();
            // -(y log p + (1-y) log(1-p)) written without cancellation.
            value += (1. + eta.exp())
                .ln()
                .min(eta.max(0.) + (1. + (-eta.abs()).exp()).ln())
                - y * eta;
        }
        value
            + parameters
                .iter()
                .zip(theta)
                .map(|(p, t)| 0.5 * p.ridge_weight * (t - p.ridge_center).powi(2))
                .sum::<f64>()
    };
    let mut theta: Vec<f64> = parameters.iter().map(|p| p.initial).collect();
    let (mut iterations, mut norm, mut converged, mut full_rank) = (0, f64::INFINITY, false, true);
    let free = |theta: &[f64], g: &[f64], i: usize| {
        !((theta[i] <= parameters[i].lower && g[i] > 0.)
            || (theta[i] >= parameters[i].upper && g[i] < 0.))
            && parameters[i].lower < parameters[i].upper
    };
    while iterations < MAX_FRACTION_ITERATIONS {
        let (mut g, mut h) = (vec![0.; n], vec![vec![0.; n]; n]);
        for (z, y) in rows {
            let x: Vec<f64> = std::iter::once(1.).chain(z.iter().copied()).collect();
            let p = logistic(x.iter().zip(&theta).map(|(a, b)| a * b).sum::<f64>());
            for i in 0..n {
                g[i] += (p - y) * x[i];
                for j in 0..n {
                    h[i][j] += p * (1. - p) * x[i] * x[j];
                }
            }
        }
        let active: Vec<usize> = (0..n).filter(|i| free(&theta, &g, *i)).collect();
        // Identification uses the unpenalized information of the free parameters.
        full_rank = solve(
            active
                .iter()
                .map(|i| active.iter().map(|j| h[*i][*j]).collect())
                .collect(),
            vec![0.; active.len()],
        )
        .is_some();
        for i in 0..n {
            g[i] += parameters[i].ridge_weight * (theta[i] - parameters[i].ridge_center);
            h[i][i] += parameters[i].ridge_weight;
        }
        // Gradient of the mean objective, so the tolerance does not depend on panel size.
        norm = active.iter().map(|i| g[*i] * g[*i]).sum::<f64>().sqrt() / rows.len() as f64;
        if norm <= tolerance {
            converged = true;
            break;
        }
        iterations += 1;
        let step = solve(
            active
                .iter()
                .map(|i| active.iter().map(|j| h[*i][*j]).collect())
                .collect(),
            active.iter().map(|i| g[*i]).collect(),
        )
        .unwrap_or_else(|| active.iter().map(|i| g[*i]).collect());
        let current = objective(&theta);
        let (mut length, mut moved) = (1., false);
        for _ in 0..60 {
            let mut trial = theta.clone();
            for (k, i) in active.iter().enumerate() {
                trial[*i] = (theta[*i] - length * step[k])
                    .clamp(parameters[*i].lower, parameters[*i].upper);
            }
            if objective(&trial) <= current {
                theta = trial;
                moved = true;
                break;
            }
            length *= 0.5;
        }
        if !moved {
            break;
        }
    }
    let active_bounds = (0..n)
        .filter(|i| {
            parameters[*i].lower < parameters[*i].upper
                && (theta[*i] <= parameters[*i].lower || theta[*i] >= parameters[*i].upper)
        })
        .map(|i| {
            if i == 0 {
                "intercept".to_string()
            } else {
                names[i - 1].clone()
            }
        })
        .collect();
    Ok(FractionModel {
        intercept: theta[0],
        coefficients: theta[1..].to_vec(),
        observations: rows.len(),
        iterations,
        gradient_norm: norm,
        converged,
        full_rank,
        active_bounds,
    })
}

struct Prepared {
    state: BehaviourState,
    z: Vec<f64>,
    training: bool,
    payment_fraction: Option<f64>,
    draw_fraction: Option<f64>,
}

fn diagnostics(model: &FractionModel, rows: &[(Vec<f64>, f64)]) -> FractionDiagnostics {
    if rows.is_empty() {
        return FractionDiagnostics::default();
    }
    let n = rows.len() as f64;
    let predicted: Vec<f64> = rows.iter().map(|(z, _)| model.fraction(z)).collect();
    FractionDiagnostics {
        observations: rows.len(),
        mean_observed: rows.iter().map(|(_, y)| y).sum::<f64>() / n,
        mean_predicted: predicted.iter().sum::<f64>() / n,
        mean_absolute_error: rows
            .iter()
            .zip(&predicted)
            .map(|((_, y), p)| (y - p).abs())
            .sum::<f64>()
            / n,
    }
}

impl FittedCardBehaviour {
    /// Stamp the identity of reviewed supplied coefficients. Fitted models are already stamped.
    pub fn sealed(mut self) -> Result<Self, String> {
        self.identity_sha256 = identity_hash(&self)?;
        Ok(self)
    }
}

fn identity_hash(model: &FittedCardBehaviour) -> Result<String, String> {
    let mut clean = model.clone();
    clean.identity_sha256.clear();
    let bytes = serde_json::to_vec(&clean).map_err(|e| e.to_string())?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn validate_template(template: &CovariateFitRequest) -> Result<(), String> {
    if !template.segments.is_empty() {
        return Err("transition template must not carry its own segments".into());
    }
    for row in &template.rows {
        let bad = match row {
            OriginModel::FixedRow {
                hazards_per_year, ..
            } => hazards_per_year[3] != 0. || hazards_per_year[4] != 0.,
            OriginModel::LogLinear { edges } => edges
                .iter()
                .any(|e| matches!(e.destination, CreditState::Default | CreditState::Prepaid)),
        };
        if bad {
            return Err("behaviour transitions cannot target default or prepaid; credit exits belong to the credit model".into());
        }
    }
    Ok(())
}

pub fn fit(request: &FitRequest) -> Result<FitResult, String> {
    fit_with_check(request, &mut || Ok(()))
}

pub fn fit_with_check(
    request: &FitRequest,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<FitResult, String> {
    check()?;
    let template = &request.transition_template;
    if request.schema != SCHEMA {
        return Err("unsupported card behaviour schema".into());
    }
    if request.records.is_empty() || request.records.len() > MAX_PANEL_RECORDS {
        return Err("card behaviour panel requires 1..20000 records".into());
    }
    if !(3..=6).contains(&request.ccf_lookback_cycles) {
        return Err("CCF reference must be 3 to 6 cycles before default".into());
    }
    if !(request.fraction_gradient_tolerance > 0. && request.fraction_gradient_tolerance <= 1e-3)
        || request
            .supplied_ccf
            .iter()
            .flatten()
            .any(|c| !(0. ..=1.).contains(c))
    {
        return Err("invalid fraction tolerance or supplied CCF".into());
    }
    validate_template(template)?;
    let (cut, columns) = (seconds(template.train_end), &template.columns);
    let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

    // Order every account chronologically; panels are contiguous statement cycles.
    let mut accounts = BTreeMap::<&str, Vec<usize>>::new();
    for (i, r) in request.records.iter().enumerate() {
        check()?;
        let money = [
            r.statement_balance,
            r.minimum_due,
            r.payment,
            r.limit,
            r.purchases,
        ];
        if money.iter().any(|v| !v.is_finite() || *v < 0. || *v > 1e15)
            || r.default_balance
                .is_some_and(|b| !(0. ..=1e15).contains(&b))
            || seconds(r.next_statement) <= seconds(r.statement)
            || seconds(r.available_time) > seconds(r.statement)
            || r.record_id.is_empty()
            || r.account_id.is_empty()
        {
            return Err(format!(
                "panel record {} has invalid amounts, clocks or identity",
                r.record_id
            ));
        }
        if seconds(r.statement) < cut && seconds(r.next_statement) > cut {
            return Err("a statement cycle crosses the chronological training cut".into());
        }
        accounts.entry(&r.account_id).or_default().push(i);
    }
    let mut prepared = Vec::with_capacity(request.records.len());
    let mut segments = Vec::new();
    let (mut training, mut holdout) = (SplitDiagnostics::default(), SplitDiagnostics::default());
    let (mut additional, mut undrawn) = ([0.; 3], [0.; 3]);
    for path in accounts.values_mut() {
        path.sort_by_key(|i| seconds(request.records[*i].statement));
        let states: Vec<BehaviourState> = path
            .iter()
            .map(|i| {
                let r = &request.records[*i];
                classify(
                    r.statement_balance,
                    r.minimum_due,
                    r.payment,
                    &request.classification,
                )
            })
            .collect::<Result<_, _>>()?;
        for (k, i) in path.iter().enumerate() {
            check()?;
            let r = &request.records[*i];
            if k > 0 && request.records[path[k - 1]].next_statement != r.statement {
                return Err("account statement cycles must be contiguous".into());
            }
            if k > 0 && request.records[path[k - 1]].default_balance.is_some() {
                return Err("an account has records after its default".into());
            }
            let z = transform(columns, &r.covariates)?;
            let is_training = seconds(r.next_statement) <= cut;
            let split = if is_training {
                &mut training
            } else {
                &mut holdout
            };
            split.records += 1;
            split.state_counts[states[k].slot()] += 1;
            let above = r.statement_balance - r.minimum_due;
            let payment_fraction = (states[k] == BehaviourState::Revolver && above > 0.)
                .then(|| ((r.payment - r.minimum_due) / above).clamp(0., 1.));
            let open = r.limit - (r.statement_balance - r.payment);
            let draw_fraction = if r.default_balance.is_some() {
                None
            } else if open > 0. {
                if r.purchases > open * (1. + 1e-9) {
                    return Err(format!(
                        "record {} purchases exceed its open-to-buy",
                        r.record_id
                    ));
                }
                Some((r.purchases / open).min(1.))
            } else {
                split.records_without_open_to_buy += 1;
                None
            };
            if let Some(balance) = r.default_balance {
                split.defaults += 1;
                let back = request.ccf_lookback_cycles as usize;
                if k >= back {
                    let reference = &request.records[path[k - back]];
                    if is_training {
                        let slot = states[k - back].slot();
                        additional[slot] += (balance - reference.statement_balance).max(0.);
                        undrawn[slot] += (reference.limit - reference.statement_balance).max(0.);
                    }
                } else {
                    split.defaults_without_reference += 1;
                }
            }
            // The class of the next statement is known at that statement: an exact transition time.
            if k + 1 < path.len() {
                let next = states[k + 1];
                segments.push(covariate::Segment {
                    segment_id: r.record_id.clone(),
                    account_id: r.account_id.clone(),
                    source: if is_training {
                        template.training_source.clone()
                    } else {
                        template.holdout_source.clone()
                    },
                    start: r.statement,
                    end: r.next_statement,
                    origin: states[k].estimator_state(),
                    ending: if next != states[k] {
                        covariate::Ending::Transition {
                            destination: next.estimator_state(),
                        }
                    } else if k + 2 < path.len() {
                        covariate::Ending::CovariateBoundary {}
                    } else {
                        covariate::Ending::RightCensored {}
                    },
                    available_time: r.available_time,
                    covariates: r.covariates.clone(),
                    term_level: r.term_level.clone(),
                });
            }
            prepared.push(Prepared {
                state: states[k],
                z,
                training: is_training,
                payment_fraction,
                draw_fraction,
            });
        }
    }
    let mut transition_request = template.clone();
    transition_request.segments = segments;
    let transition_fit = covariate::fit_with_check(&transition_request, check)?;

    let rows = |training: bool, pick: &dyn Fn(&Prepared) -> Option<f64>| -> Vec<(Vec<f64>, f64)> {
        prepared
            .iter()
            .filter(|p| p.training == training)
            .filter_map(|p| pick(p).map(|y| (p.z.clone(), y)))
            .collect()
    };
    let payment_pick = |p: &Prepared| p.payment_fraction;
    let payment = fit_fraction(
        &request.payment,
        &names,
        &rows(true, &payment_pick),
        request.fraction_gradient_tolerance,
    )?;
    training.payment = diagnostics(&payment, &rows(true, &payment_pick));
    holdout.payment = diagnostics(&payment, &rows(false, &payment_pick));
    let mut draw = Vec::new();
    for state in STATES {
        check()?;
        let pick = move |p: &Prepared| {
            if p.state == state {
                p.draw_fraction
            } else {
                None
            }
        };
        let model = fit_fraction(
            &request.draw[state.slot()],
            &names,
            &rows(true, &pick),
            request.fraction_gradient_tolerance,
        )?;
        training.draw[state.slot()] = diagnostics(&model, &rows(true, &pick));
        holdout.draw[state.slot()] = diagnostics(&model, &rows(false, &pick));
        draw.push(model);
    }
    let draw: [FractionModel; 3] = draw
        .try_into()
        .map_err(|_| "three draw models required".to_string())?;

    let (mut ccf, mut ccf_source) = ([0.; 3], [CcfSource::FittedTrainingDefaults; 3]);
    for state in STATES {
        let s = state.slot();
        if undrawn[s] > 0. {
            ccf[s] = additional[s] / undrawn[s];
            if ccf[s] > 1. {
                return Err(
                    "observed draws before default exceed the undrawn reference line".into(),
                );
            }
        } else if let Some(value) = request.supplied_ccf[s] {
            (ccf[s], ccf_source[s]) = (value, CcfSource::SuppliedReviewedAssumption);
        } else {
            return Err(format!(
                "no training default with undrawn line for {state:?}; supply a reviewed CCF"
            ));
        }
    }
    let mut reasons = transition_fit.eligibility_reasons.clone();
    for (name, m) in std::iter::once(("payment", &payment)).chain(
        ["draw transactor", "draw revolver", "draw minimum payer"]
            .into_iter()
            .zip(&draw),
    ) {
        if !m.converged {
            reasons.push(format!("{name} model did not converge"));
        }
        if !m.full_rank {
            reasons.push(format!(
                "{name} model is not identified by its training rows"
            ));
        }
        if !m.active_bounds.is_empty() {
            reasons.push(format!("{name} model stopped on a parameter bound"));
        }
    }
    let numerical_eligible = transition_fit.model.numerical_eligible
        && reasons.len() == transition_fit.eligibility_reasons.len();
    if template.data_kind == DataKind::Synthetic {
        reasons.push("synthetic panel".into());
    }
    reasons.push(PUBLICATION_REFUSAL.into());
    let mut model = FittedCardBehaviour {
        schema: SCHEMA.into(),
        data_kind: template.data_kind,
        classification: request.classification,
        columns: columns.clone(),
        term_levels: template.term_levels.clone(),
        transition: transition_fit.model.clone(),
        payment,
        draw,
        ccf,
        ccf_source,
        ccf_lookback_cycles: request.ccf_lookback_cycles,
        numerical_eligible,
        adoption_eligible: numerical_eligible && template.data_kind == DataKind::Observed,
        publication_eligible: false,
        eligibility_reasons: reasons,
        identity_sha256: String::new(),
    };
    model.identity_sha256 = identity_hash(&model)?;
    Ok(FitResult {
        schema: SCHEMA.into(),
        model,
        transition_fit,
        training,
        holdout,
    })
}

/// Cycle transition probabilities from the fitted annual rates.
///
/// A behaviour class is determined once per statement, so each training record exposes one full
/// cycle and the estimator's rate is transitions per cycle-year. The cycle probability is that
/// rate times the cycle length; the remainder stays. Rates too large for a probability reject.
pub fn transition_probabilities(
    model: &FittedCardBehaviour,
    origin: BehaviourState,
    covariates: &[f64],
    term_level: &str,
    cycle_years: f64,
) -> Result<[f64; 3], String> {
    if !(cycle_years > 0. && cycle_years <= 1.) {
        return Err("cycle length must lie in (0, 1] ACT/365 years".into());
    }
    let z = transform(&model.columns, covariates)?;
    let term = model
        .term_levels
        .iter()
        .position(|t| t == term_level)
        .ok_or("unknown term level")?;
    let mut out = [0.; 3];
    match &model.transition.rows[origin.slot()] {
        ResolvedRow::FixedRow {
            hazards_per_year, ..
        } => {
            for s in 0..3 {
                out[s] = hazards_per_year[s] * cycle_years;
            }
        }
        ResolvedRow::LogLinear { edges } => {
            for e in edges {
                let slot = [
                    CreditState::Performing,
                    CreditState::Watch,
                    CreditState::Delinquent,
                ]
                .iter()
                .position(|s| *s == e.destination)
                .ok_or("behaviour transition model has a credit exit edge")?;
                let eta = e.intercept.value
                    + e.covariate_coefficients
                        .iter()
                        .zip(&z)
                        .map(|(b, x)| b.value * x)
                        .sum::<f64>()
                    + if term > 0 {
                        e.term_coefficients[term - 1].value
                    } else {
                        0.
                    };
                out[slot] = eta.exp() * cycle_years;
            }
        }
    }
    out[origin.slot()] = 0.;
    let leaving: f64 = out.iter().sum();
    if !leaving.is_finite() || out.iter().any(|p| *p < 0.) || leaving > 1. {
        return Err("behaviour transition rate is not a cycle probability".into());
    }
    out[origin.slot()] = 1. - leaving;
    Ok(out)
}

/// State-conditional exposure at default: outstanding plus one conversion of the undrawn line.
pub fn exposure_at_default(
    model: &FittedCardBehaviour,
    state: BehaviourState,
    outstanding: f64,
    limit: f64,
) -> Result<f64, String> {
    if !outstanding.is_finite() || !limit.is_finite() || outstanding < 0. || limit < 0. {
        return Err("outstanding and limit must be finite and nonnegative".into());
    }
    Ok(outstanding + model.ccf[state.slot()] * (limit - outstanding).max(0.))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionUse {
    Adoption,
    AnalyticalOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitAction {
    pub cycle_index: usize,
    /// Covariate column whose supplied value selects the band, for example a score.
    pub column: String,
    pub lower: f64,
    pub upper: f64,
    /// New limit as a multiple of the limit in force. Decreases never force a payment.
    pub multiplier: f64,
}

/// Issuer actions are supplied policies. Nothing here is fitted or inferred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerPolicy {
    pub review: Identity,
    pub limit_actions: Vec<LimitAction>,
    /// Close the line after this many consecutive cycles with no purchase and no rights.
    pub inactivity_closure_cycles: Option<u32>,
    /// Rates applied from the first cycle whose supplied credit state is delinquent.
    pub delinquency_reprice: Option<CardRates>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CycleDriver {
    pub covariates: Vec<f64>,
    pub term_level: String,
    /// Shared uniform in [0, 1) that selects this cycle's behaviour transition.
    pub transition_uniform: f64,
    /// Credit state from the credit model; the behaviour layer never changes it.
    pub credit_state: ConsumerState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionRequest {
    pub schema: String,
    pub model: FittedCardBehaviour,
    pub use_policy: ProjectionUse,
    /// Complete mechanics request with no steps. Its cycles define statements and due dates.
    pub card: CardRequest,
    pub initial_state: BehaviourState,
    /// Column replaced each cycle by simulated billed debt over limit. Purchases then drift
    /// toward the utilisation at which the fitted draw and payment equations balance.
    pub utilisation_column: Option<String>,
    /// Purchases smaller than this currency amount are not made.
    pub minimum_purchase: f64,
    /// One driver per statement cycle.
    pub drivers: Vec<CycleDriver>,
    /// Cycle whose due date is the supplied default date, if the credit model defaults the account.
    pub default_cycle: Option<usize>,
    pub policy: IssuerPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CycleOutcome {
    pub cycle_index: usize,
    pub state: BehaviourState,
    pub transition_probabilities: [f64; 3],
    pub utilisation: Option<f64>,
    pub draw_fraction: f64,
    pub purchase: f64,
    pub statement_total: f64,
    pub minimum_payment: f64,
    pub payment: f64,
    /// Statement balance left unpaid at the due date.
    pub revolving_balance: f64,
    pub undrawn: f64,
    /// Outstanding plus the state's conversion of the undrawn line, as of this due date.
    pub exposure_at_default: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectionResult {
    pub schema: String,
    pub model_identity_sha256: String,
    pub adoption_output: bool,
    pub analytical_only: bool,
    pub publication_eligible: bool,
    /// The generated ordinary mechanics request; replaying it reproduces `card`.
    pub generated: CardRequest,
    pub closure_ordinal: Option<i32>,
    pub card: CardResult,
    pub cycles: Vec<CycleOutcome>,
    /// The single conversion of undrawn line drawn immediately before default.
    pub default_conversion_draw: f64,
    pub exposure_at_default: Option<f64>,
}

pub fn project(request: &ProjectionRequest) -> Result<ProjectionResult, String> {
    project_with_check(request, &mut || Ok(()))
}

pub fn project_with_check(
    request: &ProjectionRequest,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ProjectionResult, String> {
    check()?;
    let (model, base) = (&request.model, &request.card);
    if request.schema != SCHEMA || model.schema != SCHEMA {
        return Err("unsupported card behaviour schema".into());
    }
    if identity_hash(model)? != model.identity_sha256 {
        return Err("fitted card behaviour identity mismatch".into());
    }
    if request.use_policy == ProjectionUse::Adoption && !model.adoption_eligible {
        return Err("adoption requires an eligible observed fit".into());
    }
    let n = base.cycles.len();
    if !base.steps.is_empty() || n == 0 || n > MAX_CYCLES || request.drivers.len() != n {
        return Err(
            "projection needs a step-free card request and one driver per cycle (1..60)".into(),
        );
    }
    if !(request.minimum_purchase.is_finite() && request.minimum_purchase >= 0.) {
        return Err("minimum purchase must be finite and nonnegative".into());
    }
    if request.default_cycle.is_some_and(|c| c >= n) {
        return Err("default cycle outside the supplied cycles".into());
    }
    let column = |name: &str| {
        model
            .columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| format!("unknown covariate column {name}"))
    };
    let utilisation_column = request
        .utilisation_column
        .as_deref()
        .map(column)
        .transpose()?;
    for a in &request.policy.limit_actions {
        if Some(column(&a.column)?) == utilisation_column {
            return Err(
                "issuer limit bands use supplied columns, not simulated utilisation".into(),
            );
        }
        if a.cycle_index >= n
            || a.lower.is_nan()
            || a.upper.is_nan()
            || a.lower >= a.upper
            || !(0. ..=100.).contains(&a.multiplier)
        {
            return Err("invalid issuer limit action".into());
        }
    }
    for d in &request.drivers {
        if !(0. ..1.).contains(&d.transition_uniform)
            || matches!(
                d.credit_state,
                ConsumerState::Default | ConsumerState::PaidOff
            )
        {
            return Err("drivers need a uniform in [0,1) and a live credit state; default is supplied by default_cycle".into());
        }
    }
    let mut generated = base.clone();
    let replay = |steps: &[CardStep],
                  closure: Option<i32>,
                  check: &mut dyn FnMut() -> Result<(), String>| {
        let mut r = base.clone();
        r.steps = steps.to_vec();
        card::simulate_with_check(&r, closure, check)
    };
    let point = |result: &CardResult, day: i32| {
        result
            .points
            .iter()
            .find(|p| p.ordinal == day)
            .cloned()
            .ok_or_else(|| "card point missing for a projected date".to_string())
    };
    let (mut steps, mut cycles) = (Vec::<CardStep>::new(), Vec::<CycleOutcome>::new());
    let (mut state, mut credit, mut repriced, mut idle) =
        (request.initial_state, base.initial_state, false, 0u32);
    let (mut closure, mut conversion, mut ead) = (None, 0., None);
    for c in 0..n {
        check()?;
        let (cycle, driver) = (&base.cycles[c], &request.drivers[c]);
        let draw_day = if c == 0 {
            base.as_of_ordinal
        } else {
            base.cycles[c - 1].due_ordinal + 1
        };
        if draw_day > cycle.statement_ordinal {
            return Err(
                "each statement must follow the previous due date by at least one day".into(),
            );
        }
        // Rights before today's actions: the previous day's close, or the opening on the first day.
        let before = if c == 0 {
            None
        } else {
            Some(point(&replay(&steps, None, check)?, draw_day - 1)?)
        };
        let opening_debt: f64 = base
            .opening_principal
            .iter()
            .chain(&base.opening_billed_interest)
            .chain(&base.opening_fees)
            .sum();
        let (mut limit, debt) = before
            .as_ref()
            .map_or((base.initial_limit, opening_debt), |p| {
                (
                    p.limit,
                    p.principal
                        .iter()
                        .chain(&p.billed_interest)
                        .chain(&p.fees)
                        .sum::<f64>(),
                )
            });
        let mut covariates = driver.covariates.clone();
        if covariates.len() != model.columns.len() {
            return Err("driver covariate count differs from the fitted columns".into());
        }
        if driver.credit_state != credit {
            steps.push(CardStep {
                ordinal: draw_day,
                action: CardAction::SetState {
                    state: driver.credit_state,
                },
            });
            credit = driver.credit_state;
        }
        if credit == ConsumerState::Delinquent && !repriced {
            if let Some(rates) = &request.policy.delinquency_reprice {
                steps.push(CardStep {
                    ordinal: draw_day,
                    action: CardAction::SetRates {
                        rates: rates.clone(),
                    },
                });
                repriced = true;
            }
        }
        for a in request
            .policy
            .limit_actions
            .iter()
            .filter(|a| a.cycle_index == c)
        {
            let value = covariates[column(&a.column)?];
            if value >= a.lower && value < a.upper {
                limit *= a.multiplier;
                steps.push(CardStep {
                    ordinal: draw_day,
                    action: CardAction::SetLimit { amount: limit },
                });
            }
        }
        let utilisation = (limit > 0.).then(|| debt / limit);
        if let Some(k) = utilisation_column {
            let c = &model.columns[k];
            covariates[k] = utilisation
                .unwrap_or(c.raw_upper)
                .clamp(c.raw_lower, c.raw_upper);
        }
        let z = transform(&model.columns, &covariates)?;
        let mut probabilities = [0.; 3];
        probabilities[state.slot()] = 1.;
        if c > 0 {
            let years =
                f64::from(cycle.statement_ordinal - base.cycles[c - 1].statement_ordinal) / 365.;
            probabilities =
                transition_probabilities(model, state, &covariates, &driver.term_level, years)?;
            // Stay first, then the other two states in slot order; the last takes the remainder.
            let others: Vec<BehaviourState> = STATES.into_iter().filter(|s| *s != state).collect();
            let stay = probabilities[state.slot()];
            if driver.transition_uniform >= stay {
                state = if driver.transition_uniform < stay + probabilities[others[0].slot()] {
                    others[0]
                } else {
                    others[1]
                };
            }
        } else if !model.term_levels.contains(&driver.term_level) {
            return Err("unknown term level".into());
        }
        let open = (limit - debt).max(0.);
        let draw_fraction = if credit == ConsumerState::Performing {
            model.draw[state.slot()].fraction(&z).min(FRACTION_CAP)
        } else {
            0.
        };
        let purchase = Some(draw_fraction * open)
            .filter(|a| *a > 0. && *a >= request.minimum_purchase)
            .unwrap_or(0.);
        if purchase > 0. {
            steps.push(CardStep {
                ordinal: draw_day,
                action: CardAction::Draw {
                    balance_class: BalanceClass::Purchase,
                    amount: purchase,
                },
            });
        }
        let statement = replay(&steps, None, check)?
            .statements
            .into_iter()
            .find(|s| s.ordinal == cycle.statement_ordinal)
            .ok_or("statement missing for a supplied cycle")?;
        let (total, minimum) = (statement.statement_total, statement.minimum_payment);
        let payment = match state {
            BehaviourState::Transactor => total,
            BehaviourState::MinimumPayer => minimum,
            BehaviourState::Revolver => minimum + model.payment.fraction(&z) * (total - minimum),
        };
        let defaulting = request.default_cycle == Some(c);
        if defaulting {
            // The supplied default occurs at this due date, in place of the payment. Draws need a
            // performing line, so the conversion applies only when the credit state still allows it.
            let at = point(&replay(&steps, None, check)?, cycle.due_ordinal - 1)?;
            let outstanding: f64 = at.principal.iter().sum();
            if credit == ConsumerState::Performing {
                conversion = model.ccf[state.slot()].min(FRACTION_CAP) * at.undrawn;
                if conversion > 0. {
                    steps.push(CardStep {
                        ordinal: cycle.due_ordinal,
                        action: CardAction::Draw {
                            balance_class: BalanceClass::Purchase,
                            amount: conversion,
                        },
                    });
                }
            }
            ead = Some(outstanding + conversion);
            steps.push(CardStep {
                ordinal: cycle.due_ordinal,
                action: CardAction::Default {},
            });
        } else if payment > 0. {
            steps.push(CardStep {
                ordinal: cycle.due_ordinal,
                action: CardAction::Payment { amount: payment },
            });
        }
        let after = point(&replay(&steps, None, check)?, cycle.due_ordinal)?;
        let outstanding: f64 = after.principal.iter().sum();
        cycles.push(CycleOutcome {
            cycle_index: c,
            state,
            transition_probabilities: probabilities,
            utilisation,
            draw_fraction,
            purchase,
            statement_total: total,
            minimum_payment: minimum,
            payment: if defaulting { 0. } else { payment },
            revolving_balance: if defaulting {
                0.
            } else {
                (total - payment).max(0.)
            },
            undrawn: after.undrawn,
            exposure_at_default: if defaulting {
                ead.unwrap_or(0.)
            } else {
                exposure_at_default(model, state, outstanding, after.limit.max(outstanding))?
            },
        });
        if defaulting {
            break;
        }
        idle = if purchase == 0. && total == 0. {
            idle + 1
        } else {
            0
        };
        if request
            .policy
            .inactivity_closure_cycles
            .is_some_and(|limit| limit > 0 && idle >= limit)
        {
            closure = Some(cycle.due_ordinal);
            break;
        }
    }
    let card = replay(&steps, closure, check)?;
    generated.steps = steps;
    let adoption = request.use_policy == ProjectionUse::Adoption;
    Ok(ProjectionResult {
        schema: SCHEMA.into(),
        model_identity_sha256: model.identity_sha256.clone(),
        adoption_output: adoption,
        analytical_only: !adoption,
        publication_eligible: false,
        generated,
        closure_ordinal: closure,
        card,
        cycles,
        default_conversion_draw: conversion,
        exposure_at_default: ead,
    })
}
