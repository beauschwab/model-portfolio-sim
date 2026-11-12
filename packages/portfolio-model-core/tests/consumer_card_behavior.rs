//! Controlled synthetic fixtures for `consumer-card-behaviour-1`. None is calibration evidence.
use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::consumer_card::{
    simulate, BalanceClass, CardAction, CardRates, CardRequest, MinimumPayment, StatementCycle,
    SCHEMA as CARD_SCHEMA,
};
use portfolio_model_core::consumer_card_behavior::*;
use portfolio_model_core::consumer_contract::{ConsumerIdentity, ConsumerState};
use portfolio_model_core::credit::{CreditMeasure, CreditState};
use portfolio_model_core::credit_accounting::Identity;
use portfolio_model_core::credit_calibration::{DataKind, ObservationMode, ObservationTime};
use portfolio_model_core::credit_covariate::{
    Column, CovariateFitRequest, Edge, FeatureRole, FittedModel, Optimizer, OriginModel, Parameter,
    ResolvedRow, SCHEMA as COVARIATE_SCHEMA,
};

const BASE: i32 = 738_000;
const CYCLE_DAYS: i32 = 30;
const TAU: f64 = 30. / 365.;

fn id(name: &str) -> Identity {
    Identity {
        id: name.into(),
        revision: "1".into(),
    }
}
fn at(day: i32) -> ObservationTime {
    ObservationTime {
        ordinal: BASE + day,
        seconds_of_day: 0,
    }
}
fn p(initial: f64, lower: f64, upper: f64) -> Parameter {
    Parameter {
        initial,
        lower,
        upper,
        ridge_weight: 0.,
        ridge_center: 0.,
    }
}
fn near(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() <= tolerance, "{a} != {b} within {tolerance}");
}
fn column(name: &str, lower: f64, upper: f64) -> Column {
    Column {
        name: name.into(),
        role: FeatureRole::Borrower,
        unit: "index".into(),
        offset: 0.,
        scale: 1.,
        raw_lower: lower,
        raw_upper: upper,
        scaling_review: id("declared-scaling"),
    }
}
fn logit(x: f64) -> f64 {
    (x / (1. - x)).ln()
}
fn fraction(intercept: f64, coefficients: Vec<f64>) -> FractionModel {
    FractionModel {
        intercept,
        coefficients,
        observations: 0,
        iterations: 0,
        gradient_norm: 0.,
        converged: true,
        full_rank: true,
        active_bounds: vec![],
    }
}
fn fixed_row(hazards: [f64; 3]) -> ResolvedRow {
    ResolvedRow::FixedRow {
        hazards_per_year: [hazards[0], hazards[1], hazards[2], 0., 0.],
        review: id("supplied-behaviour-rates"),
        rationale: "Reviewed supplied rates for an analytical fixture".into(),
    }
}

/// Reviewed supplied coefficients: draw fraction per state, revolver payment fraction, fixed rates.
fn supplied(
    columns: Vec<Column>,
    draw: [FractionModel; 3],
    payment: FractionModel,
    rows: [[f64; 3]; 3],
    ccf: [f64; 3],
) -> FittedCardBehaviour {
    FittedCardBehaviour {
        schema: SCHEMA.into(),
        data_kind: DataKind::Synthetic,
        classification: ClassificationPolicy {
            full_payment_tolerance: 0.005,
            minimum_payment_tolerance: 0.005,
        },
        columns: columns.clone(),
        term_levels: vec!["reference".into()],
        transition: FittedModel {
            schema: COVARIATE_SCHEMA.into(),
            measure: CreditMeasure::Physical,
            data_kind: DataKind::Synthetic,
            population: id("fixture"),
            model_policy: id("fixture-policy"),
            training_source: id("none"),
            training_split: id("none"),
            columns,
            term_levels: vec!["reference".into()],
            rows: rows.map(fixed_row),
            training_digest_sha256: String::new(),
            numerical_eligible: true,
            adoption_eligible: false,
            coefficient_identity_sha256: String::new(),
        },
        payment,
        draw,
        ccf,
        ccf_source: [CcfSource::SuppliedReviewedAssumption; 3],
        ccf_lookback_cycles: 3,
        numerical_eligible: true,
        adoption_eligible: false,
        publication_eligible: false,
        eligibility_reasons: vec!["synthetic panel".into()],
        identity_sha256: String::new(),
    }
    .sealed()
    .unwrap()
}

/// Statements on day 10 + 30c, due 21 days later; APR 36.5% ACT/365F is 0.1% of principal a day.
fn card(cycles: usize, opening: f64, limit: f64, grace: bool) -> CardRequest {
    let asof = ordinal_from_ymd(2024, 1, 31).unwrap();
    CardRequest {
        schema: CARD_SCHEMA.into(),
        identity: ConsumerIdentity {
            position_id: "card-1".into(),
            original_loan_id: "account-1".into(),
            cohort_id: "cohort-card".into(),
            currency: "USD".into(),
            source_revision: "fixture".into(),
            model_version: CARD_SCHEMA.into(),
            assumption_id: "behaviour-fixture".into(),
            run_id: "base".into(),
            measure: CashflowMeasure::Physical,
        },
        as_of_ordinal: asof,
        horizon_days: 30 * cycles as u32 + 60,
        opening_principal: [opening, 0., 0.],
        opening_billed_interest: [0.; 3],
        opening_unbilled_interest: [0.; 3],
        opening_unrecognized_billed_interest: [0.; 3],
        opening_fees: [0.; 3],
        opening_contingent_purchase_interest: 0.,
        opening_statement_remaining: [0.; 3],
        opening_minimum_remaining: 0.,
        opening_due_ordinal: None,
        initial_limit: limit,
        initial_state: ConsumerState::Performing,
        rates: CardRates {
            purchase: 0.365,
            cash_advance: 0.73,
            promotional: 0.,
            promotional_expiry_ordinal: asof,
            post_promotion: 0.365,
        },
        day_count: ActualDayCount::Act365Fixed,
        initial_purchase_grace: grace,
        regain_grace_after_full_statement_payment: true,
        compound_billed_interest: false,
        minimum: MinimumPayment {
            floor: 25.,
            principal_fraction: 0.01,
            include_billed_interest: true,
            include_fees: true,
            allocation_order: vec![
                BalanceClass::Purchase,
                BalanceClass::CashAdvance,
                BalanceClass::Promotional,
            ],
        },
        cycles: (0..cycles as i32)
            .map(|c| StatementCycle {
                statement_ordinal: asof + 10 + 30 * c,
                due_ordinal: asof + 31 + 30 * c,
            })
            .collect(),
        steps: vec![],
        collection_fraction: 0.4,
        collection_cost: 10.,
        recovery_delay_days: 10,
    }
}
fn driver(covariates: Vec<f64>) -> CycleDriver {
    CycleDriver {
        covariates,
        term_level: "reference".into(),
        transition_uniform: 0.5,
        credit_state: ConsumerState::Performing,
    }
}
fn projection(
    model: FittedCardBehaviour,
    card: CardRequest,
    state: BehaviourState,
) -> ProjectionRequest {
    let n = card.cycles.len();
    let width = model.columns.len();
    ProjectionRequest {
        schema: SCHEMA.into(),
        model,
        use_policy: ProjectionUse::AnalyticalOnly,
        card,
        initial_state: state,
        utilisation_column: None,
        minimum_purchase: 0.01,
        drivers: (0..n).map(|_| driver(vec![0.; width])).collect(),
        default_cycle: None,
        policy: IssuerPolicy {
            review: id("issuer-policy"),
            limit_actions: vec![],
            inactivity_closure_cycles: None,
            delinquency_reprice: None,
        },
    }
}
const STAY: [[f64; 3]; 3] = [[0.; 3]; 3];
fn flat(draw: f64) -> [FractionModel; 3] {
    [
        fraction(logit(draw), vec![]),
        fraction(logit(draw), vec![]),
        fraction(logit(draw), vec![]),
    ]
}

#[test]
fn classification_reads_cash_not_labels() {
    let policy = ClassificationPolicy {
        full_payment_tolerance: 0.01,
        minimum_payment_tolerance: 0.01,
    };
    assert_eq!(
        classify(500., 25., 500., &policy).unwrap(),
        BehaviourState::Transactor
    );
    assert_eq!(
        classify(500., 25., 499.995, &policy).unwrap(),
        BehaviourState::Transactor
    );
    assert_eq!(
        classify(500., 25., 25., &policy).unwrap(),
        BehaviourState::MinimumPayer
    );
    assert_eq!(
        classify(500., 25., 10., &policy).unwrap(),
        BehaviourState::MinimumPayer
    );
    assert_eq!(
        classify(500., 25., 200., &policy).unwrap(),
        BehaviourState::Revolver
    );
    assert_eq!(
        classify(0., 0., 0., &policy).unwrap(),
        BehaviourState::Transactor
    );
    assert!(classify(500., 600., 100., &policy).is_err());
    assert!(classify(500., 25., 700., &policy).is_err());
    assert!(classify(f64::NAN, 25., 10., &policy).is_err());
}

#[test]
fn pure_transactor_book_earns_no_interest_and_carries_no_revolving_balance() {
    let model = supplied(vec![], flat(0.2), fraction(0., vec![]), STAY, [0.; 3]);
    let out = project(&projection(
        model,
        card(6, 0., 5000., true),
        BehaviourState::Transactor,
    ))
    .unwrap();
    assert_eq!(out.cycles.len(), 6);
    for c in &out.cycles {
        assert_eq!(c.state, BehaviourState::Transactor);
        near(c.purchase, 1000., 1e-9); // 20% of an untouched 5000 line every cycle
        near(c.payment, c.statement_total, 0.);
        near(c.statement_total, 1000., 1e-9);
        assert_eq!(c.revolving_balance, 0.);
        near(c.undrawn, 5000., 1e-9);
        near(c.exposure_at_default, 0., 1e-9); // nothing outstanding and a zero conversion factor
    }
    assert!(
        out.card.recognition.is_empty(),
        "grace purchases paid in full earn no interest"
    );
    assert_eq!(
        out.card.cashflows.iter().map(|e| e.interest).sum::<f64>(),
        0.
    );
    near(out.card.closing_principal, 0., 1e-9);
    near(out.card.drawn_principal, 6000., 1e-8);
    near(out.card.collected_principal, 6000., 1e-8);
    assert_eq!(out.card.closing_grace_contingent_interest, 0.);
}

#[test]
fn pure_minimum_payer_book_reproduces_hand_amortisation() {
    // No purchases: the draw fraction is far below the minimum purchase.
    let model = supplied(vec![], flat(1e-12), fraction(0., vec![]), STAY, [0.; 3]);
    let request = projection(
        model,
        card(5, 1000., 1000., false),
        BehaviourState::MinimumPayer,
    );
    let asof = request.card.as_of_ordinal;
    let out = project(&request).unwrap();
    // Hand recurrence: 0.1% of principal a day; statement posts the accrued interest; the minimum is
    // max(25, 1% of principal + billed interest); cash settles billed interest, then principal.
    let (mut principal, mut unbilled, mut last) = (1000_f64, 0_f64, asof);
    for (c, cycle) in out.cycles.iter().enumerate() {
        let (statement, due) = (asof + 10 + 30 * c as i32, asof + 31 + 30 * c as i32);
        unbilled += f64::from(statement - last) * principal * 0.001;
        let billed = unbilled;
        let minimum = (0.01 * principal + billed).max(25.);
        near(cycle.minimum_payment, minimum, 1e-9);
        near(cycle.statement_total, principal + billed, 1e-9);
        near(cycle.payment, minimum, 1e-9);
        assert_eq!(cycle.purchase, 0.);
        // Interest keeps accruing on the old principal through the due date, then on the new one.
        unbilled = f64::from(due - statement) * principal * 0.001;
        principal -= minimum - billed;
        last = due;
        near(
            cycle.revolving_balance,
            cycle.statement_total - minimum,
            1e-9,
        );
    }
    near(out.card.closing_principal, principal, 1e-8);
    assert!(principal < 1000. && principal > 900.);
    // The generated request is an ordinary supplied-action request: replaying it is identical.
    let replayed = simulate(&out.generated).unwrap();
    assert_eq!(replayed.closing_principal, out.card.closing_principal);
    assert_eq!(replayed.statements, out.card.statements);
    assert!(out
        .generated
        .steps
        .iter()
        .all(|s| matches!(s.action, CardAction::Payment { .. })));
}

/// Deterministic 48-bit LCG; fixtures never depend on a platform random source.
struct Lcg(u64);
impl Lcg {
    fn uniform(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1 << 48) - 1);
        (self.0 >> 16) as f64 / (1u64 << 32) as f64
    }
}

struct Truth {
    rates: [[f64; 3]; 3],
    transactor_revolver_slope: f64,
    payment: (f64, f64),
    draw: [(f64, f64); 3],
    ccf: [f64; 3],
}
const TRUTH: Truth = Truth {
    rates: [[0., 1.2, 0.3], [1.0, 0., 0.8], [0.2, 1.0, 0.]],
    transactor_revolver_slope: 0.5,
    payment: (-0.4, 0.6),
    draw: [(-1.2, 0.3), (-0.6, -0.4), (-2.0, 0.2)],
    ccf: [0.1, 0.4, 0.7],
};
fn logistic(x: f64) -> f64 {
    1. / (1. + (-x).exp())
}

/// 300 accounts, 24 monthly statements; the first 16 cycles are training.
fn panel() -> Vec<PanelRecord> {
    let (mut rng, mut records) = (Lcg(20_261_111), Vec::new());
    for account in 0..300 {
        let z = [-1., 0., 1.][account % 3];
        let (mut state, mut balance, limit) =
            (account % 3, 800. + 10. * (account % 50) as f64, 5000.);
        let defaults_at = (account % 10 == 0).then_some(8 + account % 5);
        for cycle in 0..24 {
            let minimum = (0.02 * balance).max(25.).min(balance);
            let noise = 0.1 * (rng.uniform() - 0.5);
            let payment = match state {
                0 => balance,
                2 => minimum,
                _ => {
                    minimum
                        + (logistic(TRUTH.payment.0 + TRUTH.payment.1 * z) + noise)
                            * (balance - minimum)
                }
            };
            let open = limit - (balance - payment);
            let defaulting = defaults_at == Some(cycle);
            let (a, b) = TRUTH.draw[state];
            let purchases = if defaulting {
                0.
            } else {
                (logistic(a + b * z) + 0.1 * (rng.uniform() - 0.5)) * open
            };
            // The reference statement three cycles earlier fixes the undrawn line being converted.
            let default_balance = defaulting.then(|| {
                let reference: &PanelRecord = &records[records.len() - 3];
                let slot = classify(
                    reference.statement_balance,
                    reference.minimum_due,
                    reference.payment,
                    &ClassificationPolicy {
                        full_payment_tolerance: 0.005,
                        minimum_payment_tolerance: 0.005,
                    },
                )
                .unwrap()
                .slot();
                reference.statement_balance
                    + TRUTH.ccf[slot] * (reference.limit - reference.statement_balance)
            });
            records.push(PanelRecord {
                record_id: format!("a{account}-c{cycle}"),
                account_id: format!("a{account}"),
                statement: at(CYCLE_DAYS * cycle as i32),
                next_statement: at(CYCLE_DAYS * (cycle as i32 + 1)),
                available_time: at(CYCLE_DAYS * cycle as i32),
                statement_balance: balance,
                minimum_due: minimum,
                payment,
                limit,
                purchases,
                covariates: vec![z],
                term_level: "reference".into(),
                default_balance,
            });
            if defaulting {
                break;
            }
            balance = balance - payment + purchases;
            let mut rates = TRUTH.rates[state];
            if state == 0 {
                rates[1] *= (TRUTH.transactor_revolver_slope * z).exp();
            }
            let u = rng.uniform();
            let stay = 1. - rates.iter().sum::<f64>() * TAU;
            if u >= stay {
                let mut cumulative = stay;
                for next in (0..3).filter(|s| *s != state) {
                    cumulative += rates[next] * TAU;
                    if u < cumulative {
                        state = next;
                        break;
                    }
                }
            }
        }
    }
    records
}
fn edges(destinations: [CreditState; 2]) -> OriginModel {
    OriginModel::LogLinear {
        edges: destinations
            .into_iter()
            .map(|destination| Edge {
                destination,
                intercept: p(-0.5, -6., 2.),
                covariate_coefficients: vec![p(0., -1., 1.)],
                term_coefficients: vec![],
            })
            .collect(),
    }
}
fn fit_request() -> FitRequest {
    let spec = || FractionSpec {
        intercept: p(0., -20., 20.),
        coefficients: vec![p(0., -20., 20.)],
    };
    FitRequest {
        schema: SCHEMA.into(),
        classification: ClassificationPolicy {
            full_payment_tolerance: 0.005,
            minimum_payment_tolerance: 0.005,
        },
        transition_template: CovariateFitRequest {
            schema: COVARIATE_SCHEMA.into(),
            measure: CreditMeasure::Physical,
            data_kind: DataKind::Synthetic,
            observation_mode: ObservationMode::ExactTransitionTimes,
            population: id("synthetic-card-panel"),
            model_policy: id("card-behaviour-policy"),
            training_source: id("training-panel"),
            holdout_source: id("holdout-panel"),
            training_split: id("train"),
            holdout_split: id("holdout"),
            train_start: at(0),
            train_end: at(CYCLE_DAYS * 16),
            holdout_end: at(CYCLE_DAYS * 24),
            columns: vec![column("score_band", -1., 1.)],
            term_levels: vec!["reference".into()],
            rows: [
                edges([CreditState::Watch, CreditState::Delinquent]),
                edges([CreditState::Performing, CreditState::Delinquent]),
                edges([CreditState::Performing, CreditState::Watch]),
            ],
            optimizer: Optimizer {
                max_iterations: 30,
                max_backtracks: 4,
                timeout_ms: 120_000,
                projected_gradient_tolerance: 1e-4,
                relative_information_tolerance: 1e-9,
            },
            segments: vec![],
        },
        payment: spec(),
        draw: [spec(), spec(), spec()],
        ccf_lookback_cycles: 3,
        supplied_ccf: [None; 3],
        fraction_gradient_tolerance: 1e-8,
        records: panel(),
    }
}

#[test]
fn synthetic_panel_recovers_generating_parameters_and_reports_holdout() {
    let result = fit(&fit_request()).unwrap();
    let model = &result.model;
    // Stated tolerances: 0.05 on logit coefficients, 0.03 on mean fractions, 25% on cycle rates.
    near(model.payment.intercept, TRUTH.payment.0, 0.05);
    near(model.payment.coefficients[0], TRUTH.payment.1, 0.05);
    for (fitted, truth) in model.draw.iter().zip(TRUTH.draw) {
        near(fitted.intercept, truth.0, 0.05);
        near(fitted.coefficients[0], truth.1, 0.05);
        assert!(
            fitted.converged && fitted.full_rank && fitted.active_bounds.is_empty(),
            "{fitted:?}"
        );
    }
    for state in STATES {
        let probabilities =
            transition_probabilities(model, state, &[0.], "reference", TAU).unwrap();
        near(probabilities.iter().sum::<f64>(), 1., 1e-12);
        for next in (0..3).filter(|s| *s != state.slot()) {
            let truth = TRUTH.rates[state.slot()][next] * TAU;
            assert!(
                (probabilities[next] / truth - 1.).abs() < 0.25,
                "{state:?}->{next}: {} vs {truth}",
                probabilities[next]
            );
        }
        near(model.ccf[state.slot()], TRUTH.ccf[state.slot()], 1e-9);
        assert_eq!(
            model.ccf_source[state.slot()],
            CcfSource::FittedTrainingDefaults
        );
    }
    // Chronological holdout is reported and predicted without having entered the fit.
    assert_eq!(
        result.training.records + result.holdout.records,
        fit_request().records.len()
    );
    assert!(result.holdout.records > 1500 && result.holdout.payment.observations > 300);
    near(
        result.holdout.payment.mean_predicted,
        result.holdout.payment.mean_observed,
        0.03,
    );
    assert!(result.holdout.payment.mean_absolute_error < 0.04);
    for d in &result.holdout.draw {
        assert!(d.observations > 200);
        near(d.mean_predicted, d.mean_observed, 0.03);
    }
    assert_eq!(result.training.defaults, 30);
    assert_eq!(result.training.defaults_without_reference, 0);
    // Synthetic fits are never adoptable and nothing here can authorize publication.
    assert!(model.numerical_eligible, "{:?}", model.eligibility_reasons);
    assert!(!model.adoption_eligible && !model.publication_eligible);
    assert!(model
        .eligibility_reasons
        .iter()
        .any(|r| r == "synthetic panel"));
    let mut adopt = projection(
        model.clone(),
        card(3, 500., 5000., false),
        BehaviourState::Revolver,
    );
    adopt.use_policy = ProjectionUse::Adoption;
    assert!(project(&adopt)
        .unwrap_err()
        .contains("eligible observed fit"));
    let analytical = project(&projection(
        model.clone(),
        card(3, 500., 5000., false),
        BehaviourState::Revolver,
    ))
    .unwrap();
    assert!(
        analytical.analytical_only
            && !analytical.adoption_output
            && !analytical.publication_eligible
    );
}

#[test]
fn holdout_changes_leave_the_fitted_identity_unchanged_and_training_changes_do_not() {
    let base = fit(&fit_request()).unwrap().model.identity_sha256;
    let mut later = fit_request();
    for r in later
        .records
        .iter_mut()
        .filter(|r| r.statement.ordinal >= BASE + CYCLE_DAYS * 16 && r.default_balance.is_none())
    {
        r.purchases *= 0.5;
    }
    assert_eq!(fit(&later).unwrap().model.identity_sha256, base);
    let mut earlier = fit_request();
    earlier
        .records
        .iter_mut()
        .find(|r| r.statement.ordinal == BASE && r.purchases > 1.)
        .unwrap()
        .purchases *= 0.5;
    assert_ne!(fit(&earlier).unwrap().model.identity_sha256, base);
    let mut reordered = fit_request();
    reordered.records.reverse();
    assert_eq!(fit(&reordered).unwrap().model.identity_sha256, base);
}

#[test]
fn default_converts_the_undrawn_line_once_and_reconciles() {
    let model = supplied(
        vec![],
        flat(0.1),
        fraction(logit(0.25), vec![]),
        STAY,
        [0.1, 0.4, 0.7],
    );
    let mut request = projection(
        model.clone(),
        card(4, 1000., 5000., false),
        BehaviourState::Revolver,
    );
    request.default_cycle = Some(2);
    let out = project(&request).unwrap();
    assert_eq!(out.cycles.len(), 3);
    let default = out.card.default.as_ref().expect("supplied default");
    let purchases: f64 = out.cycles.iter().map(|c| c.purchase).sum();
    // Outstanding the day before default plus 40% of that day's undrawn line, drawn exactly once.
    let before = out
        .card
        .points
        .iter()
        .find(|p| p.ordinal == request.card.cycles[2].due_ordinal - 1)
        .unwrap();
    let outstanding: f64 = before.principal.iter().sum();
    near(out.default_conversion_draw, 0.4 * before.undrawn, 1e-9);
    near(
        out.exposure_at_default.unwrap(),
        outstanding + 0.4 * before.undrawn,
        1e-9,
    );
    near(default.principal, out.exposure_at_default.unwrap(), 1e-9);
    near(
        out.card.drawn_principal,
        purchases + out.default_conversion_draw,
        1e-8,
    );
    near(
        1000. + out.card.drawn_principal,
        out.card.collected_principal + default.principal + out.card.closing_principal,
        1e-8,
    );
    assert_eq!(out.card.closing_principal, 0.);
    assert_eq!(
        out.cycles[2].payment, 0.,
        "the defaulting statement is not also paid"
    );
    let conversions = out
        .generated
        .steps
        .iter()
        .filter(|s| {
            s.ordinal == request.card.cycles[2].due_ordinal
                && matches!(s.action, CardAction::Draw { .. })
        })
        .count();
    assert_eq!(conversions, 1);
    // The pre-default exposure view uses the same single conversion.
    near(
        exposure_at_default(&model, BehaviourState::Revolver, 1200., 5000.).unwrap(),
        1200. + 0.4 * 3800.,
        1e-12,
    );
    near(
        exposure_at_default(&model, BehaviourState::Transactor, 6000., 5000.).unwrap(),
        6000.,
        0.,
    );
    // A delinquent line cannot draw: default then carries outstanding only.
    let mut delinquent = request.clone();
    delinquent.drivers[2].credit_state = ConsumerState::Delinquent;
    let out = project(&delinquent).unwrap();
    assert_eq!(out.default_conversion_draw, 0.);
    near(
        out.card.default.unwrap().principal,
        out.exposure_at_default.unwrap(),
        1e-9,
    );
}

#[test]
fn issuer_policies_are_supplied_actions_and_never_force_cash() {
    let columns = vec![column("score", 300., 850.)];
    let draw = || fraction(logit(0.05), vec![0.]);
    let model = supplied(
        columns,
        [draw(), draw(), draw()],
        fraction(logit(0.1), vec![0.]),
        STAY,
        [0.2; 3],
    );
    let mut request = projection(
        model,
        card(4, 4000., 5000., false),
        BehaviourState::Revolver,
    );
    for d in &mut request.drivers {
        d.covariates = vec![640.];
    }
    request.policy.limit_actions = vec![
        LimitAction {
            cycle_index: 1,
            column: "score".into(),
            lower: 300.,
            upper: 660.,
            multiplier: 0.5,
        },
        LimitAction {
            cycle_index: 2,
            column: "score".into(),
            lower: 720.,
            upper: 851.,
            multiplier: 1.5,
        },
    ];
    request.policy.delinquency_reprice = Some(CardRates {
        purchase: 0.5,
        cash_advance: 0.73,
        promotional: 0.5,
        promotional_expiry_ordinal: request.card.as_of_ordinal,
        post_promotion: 0.5,
    });
    request.drivers[3].credit_state = ConsumerState::Delinquent;
    let out = project(&request).unwrap();
    let limits: Vec<f64> = out
        .generated
        .steps
        .iter()
        .filter_map(|s| {
            if let CardAction::SetLimit { amount } = s.action {
                Some(amount)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(limits, vec![2500.], "only the matching score band acts");
    assert!(
        out.cycles[1].utilisation.unwrap() > 1.,
        "a cut below the balance leaves the debt in place"
    );
    assert_eq!(out.cycles[1].purchase, 0.);
    assert_eq!(out.cycles[1].undrawn, 0.);
    assert!(
        out.cycles[1].payment < out.cycles[1].statement_total,
        "no payment is invented by the cut"
    );
    assert_eq!(
        out.generated
            .steps
            .iter()
            .filter(|s| matches!(s.action, CardAction::SetRates { .. }))
            .count(),
        1
    );
    assert_eq!(
        out.cycles[3].purchase, 0.,
        "a delinquent line makes no purchases"
    );

    // Inactivity: an unused line with no rights closes after the declared idle cycles.
    let idle = supplied(vec![], flat(1e-12), fraction(0., vec![]), STAY, [0.; 3]);
    let mut request = projection(idle, card(5, 0., 2000., true), BehaviourState::Transactor);
    request.policy.inactivity_closure_cycles = Some(2);
    let out = project(&request).unwrap();
    assert_eq!(out.cycles.len(), 2);
    assert_eq!(
        out.closure_ordinal,
        Some(request.card.cycles[1].due_ordinal)
    );
    assert_eq!(out.card.points.last().unwrap().undrawn, 0.);
}

#[test]
fn simulated_utilisation_feeds_back_and_drifts_to_a_state_level() {
    // Purchases fall as utilisation rises; revolvers pay 10% above minimum.
    let columns = vec![column("utilisation", 0., 1.5)];
    let draw = || fraction(1.0, vec![-8.]);
    let model = supplied(
        columns,
        [draw(), draw(), draw()],
        fraction(logit(0.1), vec![0.]),
        STAY,
        [0.; 3],
    );
    let run = |opening: f64| {
        let mut request = projection(
            model.clone(),
            card(36, opening, 5000., false),
            BehaviourState::Revolver,
        );
        request.utilisation_column = Some("utilisation".into());
        project(&request).unwrap().cycles
    };
    let (low, high) = (run(250.), run(4500.));
    assert!(low[0].utilisation.unwrap() < 0.06 && high[0].utilisation.unwrap() > 0.89);
    assert!(low[0].draw_fraction > high[0].draw_fraction);
    let (a, b) = (low[35].utilisation.unwrap(), high[35].utilisation.unwrap());
    assert!(a > 0.2 && b < 0.8, "{a} {b}");
    near(a, b, 0.02);
    assert!(low[1].utilisation > low[0].utilisation && high[1].utilisation < high[0].utilisation);
}

#[test]
fn shared_uniform_selects_the_same_state_path_and_rates_must_be_probabilities() {
    let rows = [[0., 6., 0.], [0., 0., 6.], [0., 0., 0.]];
    let model = supplied(
        vec![],
        flat(0.05),
        fraction(logit(0.5), vec![]),
        rows,
        [0.; 3],
    );
    let probabilities =
        transition_probabilities(&model, BehaviourState::Transactor, &[], "reference", TAU)
            .unwrap();
    near(probabilities[1], 6. * TAU, 1e-15);
    near(probabilities[0], 1. - 6. * TAU, 1e-15);
    let states = |uniforms: [f64; 4]| {
        let mut request = projection(
            model.clone(),
            card(4, 0., 5000., true),
            BehaviourState::Transactor,
        );
        for (d, u) in request.drivers.iter_mut().zip(uniforms) {
            d.transition_uniform = u;
        }
        project(&request)
            .unwrap()
            .cycles
            .iter()
            .map(|c| c.state)
            .collect::<Vec<_>>()
    };
    use BehaviourState::*;
    assert_eq!(states([0.99, 0.1, 0.1, 0.1]), vec![Transactor; 4]);
    assert_eq!(
        states([0.0, 0.6, 0.1, 0.6]),
        vec![Transactor, Revolver, Revolver, MinimumPayer]
    );
    assert_eq!(states([0.0, 0.6, 0.1, 0.6]), states([0.0, 0.6, 0.1, 0.6]));
    assert!(
        transition_probabilities(&model, Transactor, &[], "reference", 0.5)
            .unwrap_err()
            .contains("cycle probability")
    );
    assert!(transition_probabilities(&model, Transactor, &[], "unknown", TAU).is_err());
}

#[test]
fn contract_violations_reject() {
    let rejects = |change: &dyn Fn(&mut FitRequest), text: &str| {
        let mut request = fit_request();
        change(&mut request);
        let error = fit(&request).unwrap_err();
        assert!(error.contains(text), "{error}");
    };
    rejects(&|r| r.schema = "consumer-card-1".into(), "schema");
    rejects(&|r| r.ccf_lookback_cycles = 2, "3 to 6");
    rejects(
        &|r| r.records[5].next_statement = at(CYCLE_DAYS * 6 + 1),
        "contiguous",
    );
    rejects(&|r| r.records[0].covariates = vec![3.], "declared domain");
    rejects(&|r| r.records[0].purchases = 1e9, "open-to-buy");
    rejects(
        &|r| r.transition_template.train_end = at(CYCLE_DAYS * 16 - 5),
        "crosses",
    );
    rejects(
        &|r| r.records.retain(|x| x.default_balance.is_none()),
        "supply a reviewed CCF",
    );
    rejects(
        &|r| {
            if let OriginModel::LogLinear { edges } = &mut r.transition_template.rows[0] {
                edges[0].destination = CreditState::Default;
            }
        },
        "credit exits",
    );
    let model = supplied(vec![], flat(0.1), fraction(0., vec![]), STAY, [0.; 3]);
    let good = projection(
        model.clone(),
        card(3, 100., 1000., false),
        BehaviourState::Revolver,
    );
    let mut tampered = good.clone();
    tampered.model.ccf[1] = 0.9;
    assert!(project(&tampered)
        .unwrap_err()
        .contains("identity mismatch"));
    let mut stepped = good.clone();
    stepped
        .card
        .steps
        .push(portfolio_model_core::consumer_card::CardStep {
            ordinal: stepped.card.as_of_ordinal,
            action: CardAction::Payment { amount: 1. },
        });
    assert!(project(&stepped).is_err());
    let mut short = good.clone();
    short.drivers.pop();
    assert!(project(&short).is_err());
    let mut uniform = good.clone();
    uniform.drivers[0].transition_uniform = 1.;
    assert!(project(&uniform).is_err());
    let json = serde_json::to_value(&good).unwrap();
    let mut extra = json.clone();
    extra["unexpected"] = serde_json::json!(1);
    assert!(serde_json::from_value::<ProjectionRequest>(extra).is_err());
    assert!(serde_json::from_value::<ProjectionRequest>(json).is_ok());
}
