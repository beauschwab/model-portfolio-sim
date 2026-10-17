use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::floating_rate::{
    Calculation, IndexDescriptor, IndexKind, MarginTreatment, RateBounds,
};
use portfolio_risk_native::derivative_accounting::{self as accounting, *};
use portfolio_risk_native::derivative_contract::{csa::*, options::*, swaps::*, Identity};
use portfolio_risk_native::derivative_contract::{DerivativeRequest, Operation};
use portfolio_risk_native::term_enhancements::indexed::*;
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn day(d: u32) -> i32 {
    ordinal_from_ymd(2024, 1, d).unwrap()
}
fn utc(d: i32) -> i64 {
    i64::from(d - 719163) * 86400 + 9 * 3600
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn swap(asof: i32, start: i32, end: i32, rate: f64, side: Side, path: &str) -> SwapRequest {
    let coupon = Coupon::Indexed {
        index: IndexDescriptor {
            index_id: "term-1m".into(),
            kind: IndexKind::Term,
            tenor_months: Some(1),
            average_calendar_days: None,
        },
        calendar: Box::new(id("calendar")),
        complete_calendar_mapping: true,
        calculation: Calculation::TermSimple,
        margin_treatment: MarginTreatment::Simple,
        margin: 0.,
        index_bounds: RateBounds {
            floor: None,
            cap: None,
        },
        all_in_bounds: RateBounds {
            floor: None,
            cap: None,
        },
        reset_cutoff_unix_seconds: Some(utc(start)),
        lookback_business_days: 0,
        observation_shift: false,
        observations: vec![IndexObservation {
            interest_start_ordinal: start,
            interest_end_ordinal: end,
            observation_start_ordinal: start,
            observation_end_ordinal: end,
            fixing_ordinal: start,
            selection_cutoff_unix_seconds: utc(start),
            rate,
            source: RateSource::ConditionalProjection {
                model: id("index-model"),
                path: id("index-path"),
                information_unix_seconds: utc(asof.min(start)),
                measure: CashflowMeasure::Pricing,
            },
        }],
    };
    let mut points = vec![DiscountPoint {
        ordinal: asof,
        discount_factor: 1.,
    }];
    if end > asof {
        points.push(DiscountPoint {
            ordinal: end,
            discount_factor: 1.,
        });
    }
    SwapRequest {
        contract: id("swap-contract"),
        index_basis: id("index-basis"),
        rate_path: id("index-path"),
        position_id: "swap".into(),
        scenario_id: path.into(),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        asof_ordinal: asof,
        valuation_cutoff_unix_seconds: utc(asof),
        tail_end_ordinal: end.max(asof),
        payment_timing: PaymentTiming::AfterScheduledPayments,
        side,
        notional: 360.,
        fixed_rate: 0.05,
        fixed_periods: vec![FixedPeriod {
            period_id: "f1".into(),
            start_ordinal: start,
            end_ordinal: end,
            payment_ordinal: end,
            day_count: ActualDayCount::Act360,
        }],
        floating_periods: vec![ContractPeriod {
            period_id: "l1".into(),
            start_ordinal: start,
            end_ordinal: end,
            payment_ordinal: end,
            day_count: ActualDayCount::Act360,
            coupon,
            fee: 0.,
        }],
        discount_curve: DiscountCurve {
            source: id("discount"),
            collateral_discount_policy: id("discount-policy"),
            currency: "USD".into(),
            asof_ordinal: asof,
            information_unix_seconds: utc(asof),
            basis: "collateralized_clean".into(),
            points,
        },
    }
}
fn option() -> SwaptionRequest {
    let mut u = swap(day(3), day(3), day(13), 0.1, Side::Payer, "option");
    u.tail_end_ordinal = day(13);
    SwaptionRequest {
        contract: id("option-contract"),
        exercise_policy: id("european-positive-value"),
        conditional_model: id("tree-model"),
        crn: id("shared-crn"),
        position_id: "option".into(),
        scenario_id: "option".into(),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Pricing,
        asof_ordinal: day(1),
        expiry_ordinal: day(3),
        settlement_ordinal: day(3),
        tail_end_ordinal: day(13),
        side: OptionSide::Long,
        settlement: Settlement::CollateralizedCashPrice,
        states: vec![
            TerminalState {
                state_id: "up".into(),
                probability: 0.5,
                discount_to_expiry: 0.99,
                discount_source: id("tree-discount"),
                underlying: u.clone(),
                delivered_position_id: "delivered-swap".into(),
                netting_set_id: "ns".into(),
            },
            TerminalState {
                state_id: "down".into(),
                probability: 0.5,
                discount_to_expiry: 0.99,
                discount_source: id("tree-discount"),
                underlying: {
                    let mut d = u;
                    if let Coupon::Indexed { observations, .. } = &mut d.floating_periods[0].coupon
                    {
                        observations[0].rate = 0.02;
                    }
                    d
                },
                delivered_position_id: "delivered-swap".into(),
                netting_set_id: "ns".into(),
            },
        ],
    }
}
fn node(d: u32, available: bool) -> Node {
    Node {
        ordinal: day(d),
        valuation_cutoff_unix_seconds: utc(day(d)),
        payment_timing: PaymentTiming::AfterScheduledPayments,
        trades: vec![Trade::Swap {
            request: Box::new(swap(day(d), day(1), day(11), 0.1, Side::Payer, "p")),
        }],
        discount_from_asof: 1.,
        discount_source: id("discount"),
        collateral_rate: 0.,
        collateral_rate_source: id("collateral-rate"),
        funding_spread: 0.,
        funding_source: id("funding"),
        settlement_available: available,
    }
}
fn csa() -> CsaRequest {
    CsaRequest {
        asof_ordinal: day(1),
        measure: CashflowMeasure::Pricing,
        conditional_model: id("scenario-model"),
        crn: id("shared-crn"),
        policy: Policy {
            identity: id("csa"),
            entity: "bank".into(),
            counterparty: "cp".into(),
            currency: "USD".into(),
            monetary_unit: "currency_units".into(),
            receive_threshold: 0.,
            post_threshold: 0.,
            minimum_transfer: 0.,
            settlement_lag_calendar_days: 0,
            opening_vm: 0.,
            opening_received_im: 0.,
            opening_posted_im: 0.,
            opening_source: id("opening"),
            received_vm_reusable_for_funding: false,
            received_im_segregated: true,
            collateral_interest_day_count: ActualDayCount::Act360,
            tail_return_ordinal: day(12),
        },
        mapping: vec![Mapping {
            position_id: "swap".into(),
            contract: id("swap-contract"),
            entity: "bank".into(),
            counterparty: "cp".into(),
            currency: "USD".into(),
            netting_set_id: "ns".into(),
            legal_review: id("legal"),
            enforceable_netting: true,
        }],
        paths: vec![Path {
            path_id: "p".into(),
            source: id("path"),
            probability: 1.,
            nodes: vec![node(1, true), node(3, true), node(11, true), node(12, true)],
            default: None,
        }],
        own_credit: None,
    }
}
fn physical_path(mapped: &str, start: i32, end: i32, rate: f64) -> PhysicalIndexPath {
    let u = swap(start, start, end, rate, Side::Payer, "p");
    let Coupon::Indexed {
        mut observations, ..
    } = u.floating_periods[0].coupon.clone()
    else {
        unreachable!()
    };
    observations[0].source = RateSource::Observed {
        fixing: id("observed-reset"),
        publication_unix_seconds: utc(start) - 3600,
    };
    PhysicalIndexPath {
        position_id: mapped.into(),
        period_id: "l1".into(),
        path: id("physical-path"),
        observations,
    }
}
fn request() -> DerivativeAccountingRequest {
    let mut c = csa();
    c.measure = CashflowMeasure::Physical;
    c.paths[0].nodes = (1..=12).map(|d| node(d, true)).collect();
    DerivativeAccountingRequest {
        schema: accounting::SCHEMA.into(),
        model_version: accounting::MODEL_VERSION.into(),
        run: id("accounting-run"),
        scenario_id: "p".into(),
        entity: "bank".into(),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Physical,
        basis: ExecutionBasis::ModeledPhysicalScenario,
        policy: AccountingPolicy {
            identity: id("accounting-policy"),
            trades: vec![TradeReview {
                position_id: "swap".into(),
                designation: Designation::UndesignatedTrading,
                designation_review: id("designation"),
                mark_recognition: MarkRecognition::CleanValueChangesInEarnings,
                coupon_recognition: CouponRecognition::SettleDerivativeCarrying,
                delivery_review: None,
            }],
            custody: CustodyRecognition::SegregatedCustodianMemoOnly,
            default: DefaultRecognition::ReviewedModeledCloseoutShortfallInEarnings,
            xva: DiagnosticRecognition::ReportOnly,
            modeled_funding_cost: DiagnosticRecognition::ReportOnly,
        },
        opening: OpeningState {
            source: id("opening-ledger"),
            ordinal: day(1),
            cash: 100.,
            trades: vec![TradeAnchor {
                position_id: "swap".into(),
                clean_value: 0.5,
            }],
            posted_vm_asset: 0.,
            received_vm_liability: 0.,
            posted_im_asset: 0.,
            received_im_custody: 0.,
            equity_credit_balance: 100.5,
        },
        derivative: DerivativeRequest {
            schema: "derivative-contract-1".into(),
            model_version: "derivative-contracts-1".into(),
            operation: Operation::Netting {
                request: Box::new(c),
            },
        },
        physical_index_paths: vec![physical_path("swap", day(1), day(11), 0.1)],
        pricing_xva_reference: None,
    }
}
fn raw(r: &mut DerivativeAccountingRequest) -> &mut CsaRequest {
    let Operation::Netting { request } = &mut r.derivative.operation else {
        unreachable!()
    };
    request
}
fn balance(o: &DerivativeAccountingResult, a: AccountCode) -> f64 {
    o.daily
        .last()
        .unwrap()
        .financial_trial_balance
        .iter()
        .find(|b| b.account == a)
        .map_or(0., |b| b.debit_balance)
}
fn verify(o: &DerivativeAccountingResult) {
    // Independently replay signed postings; no native journal helper used.
    let mut financial = std::collections::BTreeMap::new();
    for (a, v) in [
        (AccountCode::Cash, o.opening.cash),
        (AccountCode::PostedVmAsset, o.opening.posted_vm_asset),
        (
            AccountCode::ReceivedVmLiability,
            -o.opening.received_vm_liability,
        ),
        (AccountCode::PostedImAsset, o.opening.posted_im_asset),
        (AccountCode::OpeningEquity, -o.opening.equity_credit_balance),
    ] {
        financial.insert(a, v);
    }
    for t in &o.opening.trades {
        *financial.entry(AccountCode::DerivativeAsset).or_insert(0.) += t.clean_value.max(0.);
        *financial
            .entry(AccountCode::DerivativeLiability)
            .or_insert(0.) -= (-t.clean_value).max(0.);
    }
    for event in &o.intents {
        close(event.postings.iter().map(|p| p.debit_amount).sum(), 0.);
        assert_eq!(event.measure, CashflowMeasure::Physical);
        assert!(!event.sources.is_empty());
        if event.ledger == JournalLedger::Financial {
            for p in &event.postings {
                *financial.entry(p.account).or_insert(0.) += p.debit_amount;
            }
        }
    }
    for b in &o.daily.last().unwrap().financial_trial_balance {
        close(*financial.get(&b.account).unwrap_or(&0.), b.debit_balance);
    }
    for d in &o.daily {
        close(
            d.financial_trial_balance
                .iter()
                .map(|b| b.debit_balance)
                .sum(),
            0.,
        );
        close(
            d.custody_trial_balance
                .iter()
                .map(|b| b.debit_balance)
                .sum(),
            0.,
        );
    }
}
#[test]
fn hand_swap_cash_and_balanced_gross_collateral() {
    let o = accounting::evaluate(&request()).unwrap();
    verify(&o);
    // N360*(10/360)*(.10-.05)=.50, zero discount, collateral returned once.
    close(o.daily.last().unwrap().cash, 100.5);
    close(balance(&o, AccountCode::MarkGainLoss), 0.);
    assert_eq!(o.cash_rights.len(), 2);
    assert_eq!(o.daily.len(), 12);
    assert!(o.native.cva.is_none());
    close(o.daily.last().unwrap().received_vm_liability, 0.);
    assert!(o
        .intents
        .iter()
        .any(|i| i.event_id.starts_with("variation_margin")));
}
#[test]
fn coupon_income_presentation_offsets_mark_without_double_earnings() {
    let mut r = request();
    r.policy.trades[0].coupon_recognition = CouponRecognition::ContractualIncomeAtDueWithMarkOffset;
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 100.5);
    close(balance(&o, AccountCode::ContractualIncome), -0.5);
    close(balance(&o, AccountCode::MarkGainLoss), 0.5);
}
#[test]
fn signed_actual_reset_does_not_relabel_pricing_projection_as_cash() {
    let mut r = request();
    r.physical_index_paths[0].observations[0].rate = -0.02;
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    // Fixed -.5, actual float -.2: net cash -.7 vs opening clean asset +.5.
    close(o.daily.last().unwrap().cash, 99.3);
    close(balance(&o, AccountCode::MarkGainLoss), 1.2);
    assert!(o.cash_rights.iter().any(|c| c.physical_amount < 0.
        && matches!(c.sources.first(), Some(RateSource::Observed { .. }))));
}
fn mpor_request(rate: f64) -> DerivativeAccountingRequest {
    let mut r = request();
    r.derivative.model_version = "derivative-contracts-2".into();
    let p = r.physical_index_paths.remove(0);
    let c = raw(&mut r);
    c.policy.receive_threshold = 10.;
    c.policy.post_threshold = 10.;
    let mut physical = p;
    physical.observations[0].rate = rate;
    c.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(3),
        closeout_ordinal: day(12),
        legal_review: id("default-legal"),
        replacement_value_policy: id("replacement-policy"),
        counterparty_recovery: 0.4,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 1.,
        posted_collateral_setoff: true,
        received_im_available_on_default: false,
        mpor: Some(MporPolicy {
            identity: id("mpor"),
            claim_measure: CashflowMeasure::Physical,
            carry_policy: UnpaidCarry::NominalNoInterest,
            exercise_rights_policy: ExerciseRightsPolicy::PreserveContractualEuropeanToCloseout,
            physical_index_paths: vec![physical],
            settlement_instructions: ["fixed:f1", "floating:l1"]
                .into_iter()
                .map(|s| ObligationInstruction {
                    position_id: "swap".into(),
                    event_id: s.into(),
                    payment_ordinal: day(11),
                    disposition: ObligationDisposition::RetainToCloseout,
                })
                .collect(),
        }),
    });
    r
}
#[test]
fn actual_unpaid_mpor_rights_recovery_and_mark_loss_once() {
    let o = accounting::evaluate(&mpor_request(0.08)).unwrap();
    verify(&o);
    // Actual net due .3, R=.4: cash .12, credit loss .18, clean mark reduction .2.
    close(o.daily.last().unwrap().cash, 100.12);
    close(balance(&o, AccountCode::CounterpartyLossExpense), 0.18);
    close(balance(&o, AccountCode::MarkGainLoss), 0.2);
    close(o.daily[10].unpaid_receivable, 0.8);
    close(o.daily[10].unpaid_payable, 0.5);
    assert!(o
        .cash_rights
        .iter()
        .all(|r| r.closeout_ordinal == Some(day(12))));
}
#[test]
fn negative_unpaid_claim_is_paid_once_without_credit_haircut() {
    let o = accounting::evaluate(&mpor_request(-0.02)).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 99.3);
    close(balance(&o, AccountCode::CounterpartyLossExpense), 0.);
    close(balance(&o, AccountCode::MarkGainLoss), 1.2);
}
#[test]
fn explicit_mpor_settlement_removes_retained_right_before_closeout() {
    let mut r = mpor_request(0.08);
    for i in &mut raw(&mut r).paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .settlement_instructions
    {
        i.disposition = ObligationDisposition::SettleOnNode {
            ordinal: day(11),
            evidence: id("modeled-settlement"),
        };
    }
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 100.3);
    close(balance(&o, AccountCode::CounterpartyLossExpense), 0.);
    assert!(o
        .cash_rights
        .iter()
        .all(|r| r.settled_ordinal == Some(day(11)) && r.closeout_ordinal.is_none()));
}
#[test]
fn default_separates_received_vm_custody_and_posted_im_return_loss() {
    let mut r = request();
    r.physical_index_paths.clear();
    r.opening.received_vm_liability = 0.2;
    r.opening.posted_im_asset = 0.2;
    r.opening.received_im_custody = 0.1;
    let c = raw(&mut r);
    c.policy.opening_vm = 0.2;
    c.policy.opening_posted_im = 0.2;
    c.policy.opening_received_im = 0.1;
    c.policy.receive_threshold = 10.;
    c.policy.post_threshold = 10.;
    c.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(1),
        closeout_ordinal: day(3),
        legal_review: id("closeout-legal"),
        replacement_value_policy: id("replacement"),
        counterparty_recovery: 0.5,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 0.25,
        posted_collateral_setoff: true,
        received_im_available_on_default: true,
        mpor: None,
    });
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 100.25);
    close(balance(&o, AccountCode::CounterpartyLossExpense), 0.1);
    close(
        balance(&o, AccountCode::PostedCollateralReturnLossExpense),
        0.15,
    );
    close(o.daily.last().unwrap().received_im_custody, 0.);
    assert!(o
        .intents
        .iter()
        .any(|i| i.ledger == JournalLedger::CustodyMemo));
}
#[test]
fn late_margin_call_and_static_im_full_tail_conserve_cash() {
    let mut r = request();
    r.opening.posted_im_asset = 0.2;
    r.opening.received_im_custody = 0.3;
    r.opening.equity_credit_balance += 0.2;
    let c = raw(&mut r);
    c.policy.opening_posted_im = 0.2;
    c.policy.opening_received_im = 0.3;
    c.policy.settlement_lag_calendar_days = 2;
    c.policy.tail_return_ordinal = day(14);
    c.paths[0].nodes = (1..=14).map(|d| node(d, d != 3)).collect();
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 100.7);
    assert!(o.native.paths[0]
        .calls
        .iter()
        .any(|c| c.settled_ordinal == Some(day(4))));
    close(o.daily.last().unwrap().posted_im_asset, 0.);
    close(o.daily.last().unwrap().received_im_custody, 0.);
}
#[test]
fn input_policy_anchor_measure_calendar_and_source_fail_closed() {
    let mut r = request();
    r.policy.trades[0].designation = Designation::CashFlowHedge;
    assert!(accounting::evaluate(&r)
        .unwrap_err()
        .contains("formal hedge accounting"));
    let mut r = request();
    r.opening.equity_credit_balance = 100.;
    assert!(accounting::evaluate(&r)
        .unwrap_err()
        .contains("trial balance"));
    let mut r = request();
    r.opening.trades[0].clean_value = 0.6;
    assert!(accounting::evaluate(&r).unwrap_err().contains("opening"));
    let mut r = request();
    raw(&mut r).paths[0].nodes.remove(4);
    assert!(accounting::evaluate(&r)
        .unwrap_err()
        .contains("every calendar-day"));
    let mut r = request();
    r.physical_index_paths.clear();
    assert!(accounting::evaluate(&r)
        .unwrap_err()
        .contains("not Pricing projections"));
    let mut r = request();
    r.measure = CashflowMeasure::Pricing;
    assert!(accounting::evaluate(&r).unwrap_err().contains("Physical"));
    let mut r = request();
    r.monetary_unit = "millions".into();
    assert!(accounting::evaluate(&r).is_err());
}
#[test]
fn combined_work_rejected_before_cash_schedule_allocation() {
    let mut r = request();
    let obs = r.physical_index_paths[0].observations[0].clone();
    r.physical_index_paths[0].observations = vec![obs; 250001];
    assert!(accounting::evaluate(&r)
        .unwrap_err()
        .contains("before allocation"));
}
fn option_request(physical: bool) -> DerivativeAccountingRequest {
    let mut r = request();
    r.physical_index_paths.clear();
    r.policy.trades[0].position_id = "option".into();
    r.policy.trades[0].delivery_review = Some(id("delivery-accounting"));
    r.opening.trades[0].position_id = "option".into();
    r.opening.trades[0].clean_value = 0.2475;
    r.opening.equity_credit_balance = 100.2475;
    let c = raw(&mut r);
    c.policy.tail_return_ordinal = day(14);
    c.mapping[0].position_id = "option".into();
    c.mapping[0].contract = id("option-contract");
    c.policy.receive_threshold = 10.;
    c.policy.post_threshold = 10.;
    let mut live = option();
    live.scenario_id = "p".into();
    live.settlement = if physical {
        Settlement::PhysicalDelivery
    } else {
        Settlement::CollateralizedCashPrice
    };
    for s in &mut live.states {
        s.underlying.scenario_id = "p".into();
    }
    let mut terminal = live.clone();
    terminal.asof_ordinal = day(3);
    terminal.states.truncate(1);
    terminal.states[0].probability = 1.;
    terminal.states[0].discount_to_expiry = 1.;
    c.paths[0].nodes = (1..=14)
        .map(|d| {
            let mut n = node(d, true);
            n.trades = if d < 3 {
                let mut o = live.clone();
                o.asof_ordinal = day(d);
                vec![Trade::EuropeanSwaption {
                    request: Box::new(o),
                }]
            } else {
                let mut u = swap(day(d), day(3), day(13), 0.1, Side::Payer, "p");
                u.position_id = "delivered-swap".into();
                vec![Trade::ExpiredEuropean {
                    terminal: Box::new(terminal.clone()),
                    election: id("physical-expiry-election"),
                    delivered_underlying: if physical { Some(Box::new(u)) } else { None },
                    settlement_discount: None,
                }]
            };
            n
        })
        .collect();
    if physical {
        r.physical_index_paths = vec![physical_path("option", day(3), day(13), 0.1)];
    }
    r
}
#[test]
fn physical_delivery_transfers_carrying_and_future_cash_without_intrinsic_cash() {
    let o = accounting::evaluate(&option_request(true)).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 100.5);
    close(balance(&o, AccountCode::MarkGainLoss), -0.2525);
    let transfers = o
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("physical_delivery"))
        .collect::<Vec<_>>();
    assert_eq!(transfers.len(), 1);
    assert!(transfers[0]
        .postings
        .iter()
        .all(|p| p.account != AccountCode::Cash));
    assert!(o
        .cash_rights
        .iter()
        .all(|c| c.contract_position_id == "delivered-swap" && c.payment_ordinal == day(13)));
}
#[test]
fn european_cash_election_settles_intrinsic_exactly_once() {
    let o = accounting::evaluate(&option_request(false)).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 100.5);
    close(balance(&o, AccountCode::MarkGainLoss), -0.2525);
    assert_eq!(o.cash_rights.len(), 1);
    assert_eq!(o.cash_rights[0].event_id, "option_cash:up");
    assert_eq!(o.cash_rights[0].settled_ordinal, Some(day(3)));
}
#[test]
fn raw_pricing_xva_diagnostic_creates_no_physical_posting() {
    let mut r = request();
    let mut diagnostic = mpor_request(0.08).derivative;
    let Operation::Netting { request: c } = &mut diagnostic.operation else {
        unreachable!()
    };
    c.measure = CashflowMeasure::Pricing;
    let p = c.paths[0].default.as_mut().unwrap().mpor.as_mut().unwrap();
    p.claim_measure = CashflowMeasure::Pricing;
    p.physical_index_paths.clear();
    r.pricing_xva_reference = Some(Box::new(diagnostic));
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    close(o.pricing_xva_diagnostic.as_ref().unwrap().cva.unwrap(), 0.3);
    close(o.daily.last().unwrap().cash, 100.5);
    close(balance(&o, AccountCode::CounterpartyLossExpense), 0.);
    close(balance(&o, AccountCode::MarkGainLoss), 0.);
}
#[test]
fn signed_collateral_remuneration_separates_custodian_interest() {
    let mut r = request();
    r.opening.received_im_custody = 0.3;
    r.opening.posted_im_asset = 0.2;
    r.opening.equity_credit_balance += 0.2;
    let c = raw(&mut r);
    c.policy.opening_received_im = 0.3;
    c.policy.opening_posted_im = 0.2;
    c.policy.receive_threshold = 10.;
    c.policy.post_threshold = 10.;
    for n in &mut c.paths[0].nodes {
        n.collateral_rate = -0.036;
    }
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    // Posted IM .2 pays negative daily remuneration -.00002 for eleven days.
    close(o.daily.last().unwrap().cash, 100.7 - 0.00022);
    close(balance(&o, AccountCode::CollateralInterestIncome), 0.00022);
    close(
        o.native.paths[0]
            .movements
            .iter()
            .filter(|m| m.kind == "collateral_interest")
            .map(|m| m.custodian_cash)
            .sum(),
        0.00033,
    );
}
#[test]
fn receiver_cash_and_posted_vm_asset_have_full_final_day_tail() {
    let mut r = request();
    r.opening.trades[0].clean_value = -0.5;
    r.opening.equity_credit_balance = 99.5;
    for n in &mut raw(&mut r).paths[0].nodes {
        let Trade::Swap { request: u } = &mut n.trades[0] else {
            unreachable!()
        };
        u.side = Side::Receiver;
    }
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 99.5);
    close(balance(&o, AccountCode::DerivativeLiability), 0.);
    assert!(o.daily.iter().any(|d| d.posted_vm_asset > 0.));
    close(o.daily.last().unwrap().posted_vm_asset, 0.);
}
#[test]
fn invalid_full_tail_physical_publication_and_delivery_policy_rejected() {
    let mut r = request();
    raw(&mut r).policy.tail_return_ordinal = day(10);
    raw(&mut r).paths[0].nodes.truncate(10);
    assert!(accounting::evaluate(&r).is_err());
    let mut r = request();
    let RateSource::Observed {
        publication_unix_seconds,
        ..
    } = &mut r.physical_index_paths[0].observations[0].source
    else {
        unreachable!()
    };
    *publication_unix_seconds = utc(day(12));
    assert!(accounting::evaluate(&r).is_err());
    let mut r = option_request(true);
    r.policy.trades[0].delivery_review = None;
    assert!(accounting::evaluate(&r).unwrap_err().contains("delivery"));
    let mut r = request();
    raw(&mut r).paths[0].nodes[1].ordinal = i32::MIN;
    assert!(accounting::evaluate(&r).is_err());
    let mut r = request();
    r.physical_index_paths
        .push(r.physical_index_paths[0].clone());
    assert!(accounting::evaluate(&r).is_err());
}
#[test]
fn short_physical_delivery_is_a_liability_and_future_payment() {
    let mut r = option_request(true);
    r.opening.trades[0].clean_value = -0.2475;
    r.opening.equity_credit_balance = 99.7525;
    for n in &mut raw(&mut r).paths[0].nodes {
        match &mut n.trades[0] {
            Trade::EuropeanSwaption { request } => request.side = OptionSide::Short,
            Trade::ExpiredEuropean {
                terminal,
                delivered_underlying,
                ..
            } => {
                terminal.side = OptionSide::Short;
                delivered_underlying.as_mut().unwrap().side = Side::Receiver;
            }
            _ => unreachable!(),
        }
    }
    let o = accounting::evaluate(&r).unwrap();
    verify(&o);
    close(o.daily.last().unwrap().cash, 99.5);
    close(balance(&o, AccountCode::MarkGainLoss), 0.2525);
    assert!(o
        .daily
        .iter()
        .any(|d| d.trades[0].contract_position_id == "delivered-swap"
            && d.trades[0].clean_carrying < 0.));
}
#[test]
fn same_day_before_after_coupon_marks_give_identical_closing() {
    let after = accounting::evaluate(&request()).unwrap();
    let mut r = request();
    let n = &mut raw(&mut r).paths[0].nodes[10];
    n.payment_timing = PaymentTiming::BeforeScheduledPayments;
    let Trade::Swap { request: u } = &mut n.trades[0] else {
        unreachable!()
    };
    u.payment_timing = n.payment_timing;
    let before = accounting::evaluate(&r).unwrap();
    verify(&before);
    close(
        before.daily.last().unwrap().cash,
        after.daily.last().unwrap().cash,
    );
    close(
        balance(&before, AccountCode::MarkGainLoss),
        balance(&after, AccountCode::MarkGainLoss),
    );
}
#[test]
fn synthetic_json_request_hand_result() {
    let envelope: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/derivative-accounting-1.json"
    ))
    .unwrap();
    let r: DerivativeAccountingRequest =
        serde_json::from_value(envelope["request"].clone()).unwrap();
    let result = accounting::evaluate(&r).unwrap();
    verify(&result);
    close(result.daily.last().unwrap().cash, 100.5);
    let mut invalid = envelope["request"].clone();
    invalid["cash_override"] = serde_json::json!(1000.);
    assert!(serde_json::from_value::<DerivativeAccountingRequest>(invalid).is_err());
}
