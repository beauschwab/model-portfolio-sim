//! Original quote calibration on sealed, once-generated Pricing samples.
//! Evaluation outcomes never fit continuation. Every spread refits training.
use super::*;
use crate::products::commercial::generated_policy::{
    self as generated, EconomicSource, PricingRole,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub policy: String,
    pub lower_oas: f64,
    pub upper_oas: f64,
    pub max_trials: usize,
}
#[derive(Serialize)]
pub struct Trial {
    pub base_oas: f64,
    pub evaluation_value: f64,
}
#[derive(Serialize)]
pub struct Receipt {
    pub policy: Policy,
    pub target_cash_price: f64,
    pub trials: Vec<Trial>,
    pub selected_base_oas: f64,
    pub stopped_native_cash_value: f64,
    pub quote_residual: f64,
    pub pricing_training_samples_generated: usize,
    pub pricing_evaluation_samples_generated: usize,
    pub continuation_refitted_each_trial: bool,
    pub physical_outcomes_used_for_calibration: bool,
}
pub struct Solved {
    pub base_oas: f64,
    pub original_pricing: PricingRole,
    pub receipt: Receipt,
}
fn trial_upper(r: &Request) -> Result<(usize, usize), String> {
    let d = r.drivers.len();
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
    let periods = r.source.source.current.periods.len();
    let credit_random = r
        .source
        .credit
        .intervals
        .iter()
        .any(|p| p.hazards_per_year.iter().flatten().any(|h| *h > 0.));
    let active_upper = if !credit_random && r.annual_covariance.iter().all(|c| *c == 0.) {
        0
    } else if !credit_random
        && matches!(
            &r.source.source.current.principal_plan,
            term::indexed::PrincipalPlan::Supplied { .. }
        )
    {
        // Without credit jumps or indexed principal, P and origin-state labels
        // are constant across paths. Reserve OU/anchor levels, every observed
        // fixing, net legal interest and each canonical legal coupon.
        d + if r.hazard_loadings.is_empty() { 0 } else { d } + observations + 1 + periods
    } else {
        2 * d + 4 + observations + 2 * periods
    };
    let columns =
        (1 + active_upper + active_upper * (active_upper + 1) / 2).min(r.training.paths / 2);
    let n = r
        .training
        .paths
        .checked_add(r.evaluation.paths)
        .ok_or("calibration sample overflow")?;
    let recovery = r
        .source
        .credit
        .intervals
        .iter()
        .flat_map(|p| &p.complete_recovery_tails)
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    let cash = periods
        .checked_add(recovery + 4)
        .ok_or("calibration cash bound overflow")?;
    let fitting = n
        .checked_mul(r.policy.exercise_dates.len())
        .and_then(|v| v.checked_mul(columns.checked_mul(columns)?.checked_mul(16)?))
        .ok_or("calibration regression overflow")?;
    let targets = n
        .checked_mul(r.policy.exercise_dates.len() + 1)
        .and_then(|v| v.checked_mul(cash.checked_mul(1024)?.checked_add(512)?))
        .ok_or("calibration target work overflow")?;
    Ok((
        fitting
            .checked_add(targets)
            .ok_or("calibration trial work overflow")?,
        columns,
    ))
}

pub fn admitted(r: &Request, p: &Policy) -> Result<(usize, usize), String> {
    check()?;
    if p.policy != "native_retained_q_refit_bracket-1"
        || !p.lower_oas.is_finite()
        || !p.upper_oas.is_finite()
        || p.lower_oas < -5.
        || p.upper_oas > 5.
        || p.lower_oas >= p.upper_oas
        || !(3..=128).contains(&p.max_trials)
        || r.parallel_bump.to_bits() != 0_f64.to_bits()
    {
        return Err("explicit bounded generated original OAS calibration policy required".into());
    }
    let (generation, bytes) = admit(r)?;
    // Source generation/final stopped replay are reserved once by the existing
    // component. Additional trials only discount cash and refit retained rows.
    let (trial, _) = trial_upper(r)?;
    let work = trial
        .checked_mul(p.max_trials - 1)
        .and_then(|extra| generation.checked_add(extra))
        .ok_or("calibration work overflow")?;
    // Retained train/evaluation paths and the evaluation tape coexist with one
    // refitted policy/SVD. Reserve two complete existing component envelopes;
    // raw identity, receipt and trial vectors have a separate bounded allowance.
    let retained_recipes = generated::raw_recipe_upper(&r.source)?
        .checked_mul(r.evaluation.paths)
        .ok_or("calibration source capacity overflow")?;
    let bytes = bytes
        .checked_mul(2)
        .and_then(|v| v.checked_add(retained_recipes))
        .and_then(|v| {
            v.checked_add(
                strict_json::serialized_size(r, 1 << 20)
                    .ok()?
                    .checked_mul(20)?,
            )
        })
        .and_then(|v| v.checked_add(1 << 20))
        .ok_or("calibration capacity overflow")?;
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err(format!(
            "generated OAS calibration exceeds4B/128MiB: work={work},bytes={bytes}"
        ));
    }
    Ok((work, bytes))
}

struct Samples {
    raw: Request,
    training: Vec<Path>,
    evaluation: Vec<Path>,
    evaluation_tape: Tape,
}
struct TrialEvaluation {
    trained: NativeTrainedPolicy,
    values: Vec<f64>,
    decisions: Vec<Vec<Decision>>,
}
impl Samples {
    fn generate(r: &Request) -> Result<Self, String> {
        let tape = generate_tape(r, &r.training)?;
        let training = collect(r, &r.training, &tape)?;
        drop(tape);
        let evaluation_tape = generate_tape(r, &r.evaluation)?;
        let evaluation = collect(r, &r.evaluation, &evaluation_tape)?;
        Ok(Self {
            raw: r.clone(),
            training,
            evaluation,
            evaluation_tape,
        })
    }
    fn trial(&self, oas: f64) -> Result<TrialEvaluation, String> {
        check()?;
        let mut raw = self.raw.clone();
        raw.fixed_base_oas = oas;
        let trained = fit_paths(&raw, &self.training)?;
        let (_, columns) = trial_upper(&raw)?;
        if trained
            .regressions()
            .iter()
            .any(|f| f.coefficients.len() > columns)
        {
            // The first fit is covered by the full generic source admission.
            // An unexpected varying state cannot enter any additional trial.
            return Err("generated OAS retained state exceeds admitted regression columns".into());
        }
        let mut values = vec![0.; self.evaluation.len()];
        let mut decisions =
            vec![vec![Decision::Continue; raw.policy.exercise_dates.len()]; self.evaluation.len()];
        for index in (0..raw.policy.exercise_dates.len()).rev() {
            check()?;
            let target = targets(&raw, &self.evaluation, index, &values)?;
            for (i, path) in self.evaluation.iter().enumerate() {
                let (decision, exercise) =
                    fitted_decision(&trained, index, &path.points[index], &path.cash)?;
                decisions[i][index] = decision;
                values[i] = if decision == Decision::Continue {
                    target[i]
                } else {
                    exercise
                };
            }
        }
        Ok(TrialEvaluation {
            trained,
            values: root_values(&raw, &self.evaluation, &values)?,
            decisions,
        })
    }
    fn replay(
        &self,
        trained: &NativeTrainedPolicy,
        values: &[f64],
        decisions: &[Vec<Decision>],
    ) -> Result<(f64, Vec<EconomicSource>), String> {
        let raw = trained.request();
        let mut sum = 0.;
        let mut sources = Vec::new();
        for (i, path) in self.evaluation.iter().enumerate() {
            check()?;
            let request = path_request(raw, &raw.evaluation, &self.evaluation_tape, i)?;
            let first = decisions[i].iter().position(|d| *d != Decision::Continue);
            let stops = if let Some(index) = first {
                let date = &raw.policy.exercise_dates[index];
                let (kind, factor, fee) = match decisions[i][index] {
                    Decision::Call => (
                        owner::NativeStopAction::Call,
                        date.call_clean_price_factor
                            .ok_or("calibration call absent")?,
                        date.call_fee,
                    ),
                    Decision::Put => (
                        owner::NativeStopAction::Put,
                        date.put_clean_price_factor
                            .ok_or("calibration put absent")?,
                        date.put_fee,
                    ),
                    Decision::Continue => unreachable!(),
                };
                vec![owner::NativeStop::from_compiled_policy_with_timing(
                    i as u64 + 1,
                    format!("generated-policy:{}:{}", raw.identity.id, date.ordinal),
                    date.ordinal,
                    kind,
                    factor,
                    fee,
                    raw.policy.identity.clone(),
                    match date.timing {
                        ExerciseTiming::BeforeScheduledCash => {
                            owner::NativeStopTiming::BeforeScheduledCash
                        }
                        ExerciseTiming::AfterScheduledCash => {
                            owner::NativeStopTiming::AfterScheduledCash
                        }
                    },
                    if path.points[index].principal == 0. {
                        raw.policy
                            .rights_only_termination
                            .as_ref()
                            .map(|r| r.review.clone())
                    } else {
                        None
                    },
                )?]
            } else {
                Vec::new()
            };
            let source =
                EconomicSource::replay_pricing(request, stops, 1. / self.evaluation.len() as f64)?;
            let mut value = 0.;
            for cash in source.cash() {
                if cash.year > 0. {
                    value = amount(value + cash.total()? * df(raw, cash.year)?)?;
                }
            }
            if !close(value, values[i]) {
                return Err("calibration stopped native cash differs from retained policy".into());
            }
            sum = amount(sum + value)?;
            sources.push(source);
        }
        Ok((amount(sum / self.evaluation.len() as f64)?, sources))
    }
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
fn finish(
    samples: &Samples,
    policy: &Policy,
    target: f64,
    trials: Vec<Trial>,
    evaluated: TrialEvaluation,
) -> Result<Solved, String> {
    let TrialEvaluation {
        trained,
        values,
        decisions,
    } = evaluated;
    let (actual, sources) = samples.replay(&trained, &values, &decisions)?;
    if !close(actual, target) {
        return Err("generated OAS final stopped cash quote residual unresolved".into());
    }
    let oas = trained.request().fixed_base_oas;
    Ok(Solved {
        base_oas: oas,
        original_pricing: PricingRole {
            raw_law: trained.request().clone(),
            fitted_policy: trained,
            sources,
        },
        receipt: Receipt {
            policy: policy.clone(),
            target_cash_price: target,
            trials,
            selected_base_oas: oas,
            stopped_native_cash_value: actual,
            quote_residual: actual - target,
            pricing_training_samples_generated: samples.training.len(),
            pricing_evaluation_samples_generated: samples.evaluation.len(),
            continuation_refitted_each_trial: true,
            physical_outcomes_used_for_calibration: false,
        },
    })
}
pub fn solve(r: &Request, policy: &Policy, target: f64) -> Result<Solved, String> {
    admitted(r, policy)?;
    if !target.is_finite()
        || target <= 0.
        || target > portfolio_model_core::limits::MAX_MONETARY_AMOUNT
    {
        return Err("positive original generated quote required".into());
    }
    let samples = Samples::generate(r)?;
    let mut trials = Vec::new();
    let mut low = policy.lower_oas;
    let mut high = policy.upper_oas;
    let mut low_value = None;
    let mut high_value = None;
    let mut endpoint = None;
    for index in 0..policy.max_trials {
        let oas = match index {
            0 => low,
            1 => high,
            _ => low + (high - low) * 0.5,
        };
        let evaluated = samples.trial(oas)?;
        let value = amount(evaluated.values.iter().sum::<f64>() / evaluated.values.len() as f64)?;
        trials.push(Trial {
            base_oas: oas,
            evaluation_value: value,
        });
        if index == 0 && close(value, target) {
            low_value = Some(value);
            endpoint = Some(evaluated);
            continue;
        }
        if index == 1 && low_value.is_some_and(|low| value > low + 1e-9) {
            return Err("generated OAS fitted-policy bracket is nonmonotone".into());
        }
        if close(value, target) {
            if endpoint.is_some() {
                return Err("generated OAS quote has ambiguous matching bracket endpoints".into());
            }
            return finish(&samples, policy, target, trials, evaluated);
        }
        if index == 1 {
            if let Some(evaluated) = endpoint.take() {
                return finish(&samples, policy, target, trials, evaluated);
            }
        }
        match index {
            0 => low_value = Some(value),
            1 => {
                high_value = Some(value);
                if low_value.ok_or("calibration lower sample absent")? <= target || value >= target
                {
                    return Err("generated original quote outside decreasing OAS bracket".into());
                }
            }
            _ => {
                // Fitted-policy switching can produce a nonmonotone/discontinuous
                // sample objective. Never pretend bisection has solved that case.
                if value > low_value.ok_or("calibration lower value absent")? + 1e-9
                    || value < high_value.ok_or("calibration upper value absent")? - 1e-9
                {
                    return Err("generated OAS fitted-policy objective is nonmonotone".into());
                }
                if value > target {
                    low = oas;
                    low_value = Some(value);
                } else {
                    high = oas;
                    high_value = Some(value);
                }
                if low == high {
                    break;
                }
            }
        }
    }
    Err("generated OAS quote residual unresolved within bounded trials; policy discontinuity or insufficient bracket resolution".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> Request {
        serde_json::from_str(include_str!(
            "../../../../../../examples/models/commercial-generated-exercise-1.json"
        ))
        .unwrap()
    }
    fn policy() -> Policy {
        Policy {
            policy: "native_retained_q_refit_bracket-1".into(),
            lower_oas: -0.02,
            upper_oas: 0.06,
            max_trials: 48,
        }
    }
    #[test]
    fn known_spread_is_recovered_from_once_generated_option_free_pricing_samples() {
        let mut r = raw();
        r.policy.exercise_dates.clear();
        r.annual_covariance[0] = 0.;
        let spread: f64 = 0.017;
        // Accrual ends on day30; the contractual principal actually settles on
        // day35. OAS calibration must retain that complete settlement tail.
        let target = 100. * (-spread * 35. / 365.).exp();
        let solved = solve(&r, &policy(), target).unwrap();
        assert!(
            (solved.base_oas - spread).abs() < 2e-10,
            "oas={}, target={}, actual={}, cash={:?}",
            solved.base_oas,
            target,
            solved.receipt.stopped_native_cash_value,
            solved.original_pricing.sources[0]
                .cash()
                .iter()
                .map(|c| (c.year, c.total().unwrap()))
                .collect::<Vec<_>>()
        );
        assert!(close(solved.receipt.stopped_native_cash_value, target));
        assert!(solved.receipt.trials.len() > 3);
        assert_eq!(solved.receipt.pricing_training_samples_generated, 128);
        assert_eq!(solved.receipt.pricing_evaluation_samples_generated, 128);
        assert_eq!(solved.original_pricing.sources.len(), 128);
        assert!(solved
            .original_pricing
            .fitted_policy
            .regressions()
            .is_empty());
        assert!(solved
            .original_pricing
            .sources
            .iter()
            .all(|s| s.stops().is_empty()));
    }
    #[test]
    fn actual_exercise_refits_targets_on_identical_features_without_evaluation_fit() {
        let r = raw();
        let retained = Samples::generate(&r).unwrap();
        let TrialEvaluation {
            trained: lo,
            decisions: ld,
            ..
        } = retained.trial(-0.02).unwrap();
        let TrialEvaluation {
            trained: hi,
            decisions: hd,
            ..
        } = retained.trial(0.06).unwrap();
        assert_eq!(lo.regressions()[0].means, hi.regressions()[0].means);
        assert_eq!(lo.regressions()[0].scales, hi.regressions()[0].scales);
        assert_ne!(
            lo.regressions()[0].coefficients,
            hi.regressions()[0].coefficients
        );
        assert_ne!(ld, hd);
        let values = retained.trial(0.02).unwrap().values;
        let target = values.iter().sum::<f64>() / values.len() as f64;
        let solved = solve(&r, &policy(), target).unwrap();
        assert!(close(solved.base_oas, 0.02));
        assert!(close(solved.receipt.stopped_native_cash_value, target));
        assert!(solved
            .original_pricing
            .sources
            .iter()
            .any(|s| !s.stops().is_empty()));
        assert!(solved
            .original_pricing
            .sources
            .iter()
            .any(|s| s.stops().is_empty()));
        assert!(!solved.receipt.physical_outcomes_used_for_calibration);
        let repeated = solve(&r, &policy(), target).unwrap();
        assert_eq!(
            strict_json::encode(&solved.receipt, 1 << 20).unwrap(),
            strict_json::encode(&repeated.receipt, 1 << 20).unwrap()
        );
    }
    #[test]
    fn unreachable_quotes_insufficient_trials_and_excess_work_reject() {
        let mut r = raw();
        r.policy.exercise_dates.clear();
        r.annual_covariance[0] = 0.;
        assert!(solve(&r, &policy(), 200.)
            .err()
            .unwrap()
            .contains("outside"));
        let mut p = policy();
        p.max_trials = 3;
        assert!(solve(&r, &p, 100. * (-0.017_f64 * 30. / 365.).exp())
            .err()
            .unwrap()
            .contains("residual unresolved"));
        p.lower_oas = p.upper_oas;
        assert!(admitted(&r, &p).is_err());
        p = policy();
        p.max_trials = 128;
        r = raw();
        r.training.paths = 512;
        r.evaluation.paths = 512;
        assert!(admitted(&r, &p).unwrap_err().contains("4B/128MiB"));
    }
    #[test]
    fn matching_endpoint_ambiguity_and_real_unobserved_cash_policy_jump_reject() {
        let mut deterministic = raw();
        deterministic.policy.exercise_dates.clear();
        deterministic.annual_covariance[0] = 0.;
        let mut p = policy();
        p.lower_oas = 0.017 - 1e-14;
        p.upper_oas = 0.017 + 1e-14;
        assert!(
            solve(&deterministic, &p, 100. * (-0.017_f64 * 35. / 365.).exp())
                .err()
                .unwrap()
                .contains("ambiguous")
        );

        // Exercise precedes the future coupon fixing. Continuation fits use Q
        // training expectations; fresh realized cash can jump at a policy switch.
        let mut r = raw();
        let early = r.time_ordinals[0] + 10;
        r.time_ordinals.insert(1, early);
        r.policy.exercise_dates[0].ordinal = early;
        let samples = Samples::generate(&r).unwrap();
        let zero = samples.trial(0.).unwrap();
        let mut tested = false;
        for path in &samples.evaluation {
            let expected = forecast(&zero.trained.regressions()[0], &path.points[0]).unwrap();
            let boundary = (expected / 100.).ln() / (25. / 365.);
            if !(-1. ..=1.).contains(&boundary) {
                continue;
            }
            let left = boundary - 1e-8;
            let right = boundary + 1e-8;
            let l = samples.trial(left).unwrap().values;
            let h = samples.trial(right).unwrap().values;
            let lv = l.iter().sum::<f64>() / l.len() as f64;
            let hv = h.iter().sum::<f64>() / h.len() as f64;
            if lv - hv <= 1e-5 {
                continue;
            }
            let mut control = policy();
            control.lower_oas = left;
            control.upper_oas = right;
            let error = solve(&r, &control, (lv + hv) * 0.5)
                .err()
                .expect("a gap quote cannot become a converged price");
            assert!(
                error.contains("residual unresolved") || error.contains("nonmonotone"),
                "{error}"
            );
            tested = true;
            break;
        }
        assert!(
            tested,
            "control must contain an actual descending policy-switch price gap"
        );
    }
}
