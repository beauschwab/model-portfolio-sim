use portfolio_model_core::consumer_behavior::{self as behavior, Product};
use portfolio_model_core::consumer_contract::ConsumerState;
use portfolio_model_core::consumer_product_paths::*;
use portfolio_model_core::{consumer_card as card, consumer_installment as loan};

fn request(product: Product) -> Request {
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-behaviour-spec-1.json"
    ))
    .unwrap();
    let mut fit: behavior::Request = serde_json::from_value(raw["fit"].clone()).unwrap();
    fit.specification.product = product;
    let mut projection: behavior::Projection =
        serde_json::from_value(raw["projection"].clone()).unwrap();
    let source = &mut projection.path_source;
    source.monetary_unit = "USD".into();
    source.boundaries.clear();
    for p in &mut source.paths {
        p.principal = 0.;
        p.undrawn_commitment = 0.;
        p.accrued_interest = 0.;
        p.book_adjustment = 0.;
    }
    for i in &mut source.intervals {
        i.accrual_rates = [0.; 3];
        i.credit_conversion_factor = [0.; 3];
    }
    let path = source.paths[0].clone();
    let asof = source.origin_time.ordinal;
    let contract = if product == Product::CreditCard {
        let mut q: card::CardRequest = serde_json::from_str(include_str!(
            "../../../examples/models/consumer-card-1.json"
        ))
        .unwrap();
        q.as_of_ordinal = asof;
        q.horizon_days = 365;
        q.steps.clear();
        q.cycles.clear();
        q.rates.purchase = 0.;
        q.rates.cash_advance = 0.;
        q.rates.promotional = 0.;
        q.rates.post_promotion = 0.;
        q.rates.promotional_expiry_ordinal = asof + 200;
        ProductSource::Card {
            path_id: path.path_id,
            path_key: path.path_key,
            contract: Box::new(q),
        }
    } else {
        let mut q: loan::InstallmentRequest = serde_json::from_str(include_str!(
            "../../../examples/models/consumer-installment-1.json"
        ))
        .unwrap();
        q.as_of_ordinal = asof;
        q.horizon_days = 365;
        q.note_rate = 0.;
        q.payment_dates = vec![asof + 365];
        q.product = if product == Product::Auto {
            loan::InstallmentProduct::Auto {
                new_vehicle: false,
                vintage_ordinal: asof - 365,
                original_ltv: 1.,
                channel: "supplied".into(),
                collateral_value_as_of: 600.,
                annual_depreciation: 0.,
                sale_haircut: 0.,
                repossession_cost: 0.,
                recovery_delay_days: 20,
            }
        } else {
            loan::InstallmentProduct::Personal {
                vintage_ordinal: asof - 365,
                dti: 0.4,
                dti_definition: "supplied".into(),
                channel: "supplied".into(),
                collection_fraction: 0.5,
                collection_cost: 0.,
                recovery_delay_days: 20,
            }
        };
        ProductSource::Installment {
            path_id: path.path_id,
            path_key: path.path_key,
            contract: Box::new(q),
        }
    };
    Request {
        schema: SCHEMA.into(),
        timeout_ms: 30000,
        fit,
        projection,
        policy: Policy {
            review: portfolio_model_core::credit_accounting::Identity {
                id: "supplied-daily-policy".into(),
                revision: "1".into(),
            },
            clock: ClockPolicy::NextCalendarDayBoundary,
            same_day_transitions: CollisionPolicy::Reject,
            live_states: [
                ConsumerState::Performing,
                ConsumerState::Delinquent,
                ConsumerState::Delinquent,
            ],
        },
        products: vec![contract],
    }
}

#[test]
fn three_products_fit_project_states_and_keep_one_monetary_owner() {
    for product in [Product::Auto, Product::Personal, Product::CreditCard] {
        let r = request(product);
        let out = evaluate(&r).unwrap();
        assert!(!out.adoption_eligible && !out.empirical_accuracy_validated);
        let state_default = out.credit_states.paths.paths[0].default.as_ref().unwrap();
        assert_eq!(state_default.principal, 0.);
        assert_eq!(state_default.economic_principal_loss, 0.);
        let expected = r.projection.path_source.origin_time.ordinal
            + (state_default.year * 365.).ceil() as i32;
        match &out.products[0] {
            ProductResult::Installment {
                transitions,
                result,
                ..
            } => {
                assert_eq!(transitions[0].ordinal, expected);
                assert_eq!(
                    transitions[0].original_year.to_bits(),
                    state_default.year.to_bits()
                );
                let default = result.default.as_ref().unwrap();
                assert_eq!(default.principal, 1000.);
                assert_eq!(default.ordinal, expected);
                assert!(result.cashflows.iter().any(|c| c.ordinal == expected + 20));
                assert_eq!(result.principal_collected, 0.);
            }
            ProductResult::Card {
                result,
                transitions,
                ..
            } => {
                assert_eq!(transitions[0].ordinal, expected);
                assert_eq!(result.default.as_ref().unwrap().principal, 1000.);
                assert_eq!(result.collected_principal, 0.);
                assert!(result.cashflows.iter().any(|c| c.ordinal > expected));
            }
        }
    }
}

#[test]
fn shared_state_and_daily_owners_stop_inside_execution() {
    for product in [Product::Auto, Product::Personal, Product::CreditCard] {
        let source = request(product);
        let output = evaluate(&source).unwrap();
        let mut calls = 0;
        let mut stop = || {
            calls += 1;
            if calls == 3 {
                Err("original parent control".into())
            } else {
                Ok(())
            }
        };
        let error = portfolio_model_core::credit_path::evaluate_with_check(
            &output.credit_states.projected_path_source,
            &mut stop,
        )
        .unwrap_err();
        assert_eq!(error, "original parent control");
        assert_eq!(calls, 3);

        let mut calls = 0;
        let mut stop = || {
            calls += 1;
            if calls == 3 {
                Err("daily owner cancellation".into())
            } else {
                Ok(())
            }
        };
        let error = match &output.products[0] {
            ProductResult::Installment {
                compiled_source, ..
            } => loan::project_with_check(compiled_source, &mut stop).unwrap_err(),
            ProductResult::Card {
                compiled_source,
                closure_ordinal,
                ..
            } => {
                card::simulate_with_check(compiled_source, *closure_ordinal, &mut stop).unwrap_err()
            }
        };
        assert_eq!(error, "daily owner cancellation");
        assert_eq!(calls, 3);
    }
}

#[test]
fn fitted_card_exit_requires_zero_rights_and_closes_the_line() {
    let mut r = request(Product::CreditCard);
    for row in &mut r.fit.records {
        if row.end_status.exit.is_some() {
            row.end_status.exit = Some(behavior::Exit::ClosedZeroBalance);
            row.end_status.balance = 0.;
            row.end_status.line_open = false;
            row.end_status.days_past_due = 0;
        }
    }
    let behavior::FitTemplate::Exact { request: q } = &mut r.fit.fit_template else {
        panic!()
    };
    let portfolio_model_core::credit_covariate::OriginModel::LogLinear { edges } = &mut q.rows[0]
    else {
        panic!()
    };
    edges[0].destination = portfolio_model_core::credit::CreditState::Prepaid;
    assert!(evaluate(&r).unwrap_err().contains("zero gross rights"));
    let ProductSource::Card { contract, .. } = &mut r.products[0] else {
        panic!()
    };
    contract.opening_principal = [0.; 3];
    let out = evaluate(&r).unwrap();
    let ProductResult::Card {
        closure_ordinal: Some(day),
        result,
        ..
    } = &out.products[0]
    else {
        panic!()
    };
    assert!(result
        .points
        .iter()
        .filter(|p| p.ordinal >= *day)
        .all(|p| p.state == ConsumerState::PaidOff && p.undrawn == 0.));
    assert!(result.cashflows.is_empty());
}

#[test]
fn identities_money_grid_actions_and_full_tail_work_reject() {
    let base = request(Product::Personal);
    let mut r = base.clone();
    r.projection.path_source.paths[0].principal = 1.;
    assert!(admit(&r).is_err());
    let mut r = base.clone();
    let ProductSource::Installment { path_key, .. } = &mut r.products[0] else {
        panic!()
    };
    *path_key += 1;
    assert!(admit(&r).is_err());
    let mut r = base.clone();
    let ProductSource::Installment { contract, .. } = &mut r.products[0] else {
        panic!()
    };
    contract.horizon_days -= 1;
    assert!(admit(&r).is_err());
    let mut r = base.clone();
    let ProductSource::Installment { contract, .. } = &mut r.products[0] else {
        panic!()
    };
    contract.steps.push(loan::InstallmentStep {
        ordinal: contract.as_of_ordinal + 10,
        action: loan::InstallmentAction::Default {},
    });
    assert!(admit(&r).is_err());
    let mut r = request(Product::CreditCard);
    let ProductSource::Card { contract, .. } = &mut r.products[0] else {
        panic!()
    };
    for day in 0..10 {
        contract.steps.push(card::CardStep {
            ordinal: contract.as_of_ordinal + day,
            action: card::CardAction::Fee {
                balance_class: card::BalanceClass::Purchase,
                amount: 0.,
            },
        });
    }
    assert!(admit(&r).unwrap_err().contains("aggregate full-tail row"));
}

#[test]
fn cancellation_and_same_day_supplied_action_refuse_without_dropping_cash() {
    let mut r = request(Product::Personal);
    assert!(evaluate_checked(&r, &mut || Err("cancel-test".into()))
        .unwrap_err()
        .contains("cancel-test"));
    let out = evaluate(&r).unwrap();
    let ProductResult::Installment { transitions, .. } = &out.products[0] else {
        panic!()
    };
    let ProductSource::Installment { contract, .. } = &mut r.products[0] else {
        panic!()
    };
    contract.steps.push(loan::InstallmentStep {
        ordinal: transitions[0].ordinal,
        action: loan::InstallmentAction::Payment { amount: 1. },
    });
    assert!(evaluate(&r).unwrap_err().contains("collides"));
}

#[test]
fn two_distinct_fractional_transitions_on_one_day_are_not_collapsed() {
    let mut r = request(Product::Personal);
    let behavior::FitTemplate::Exact { request: q } = &mut r.fit.fit_template else {
        panic!()
    };
    for (index, hazards) in [
        [0., 1., 0., 0.25, 0.],
        [1., 0., 0., 0.25, 0.],
        [0., 0., 0., 0.25, 0.],
    ]
    .into_iter()
    .enumerate()
    {
        q.rows[index]=serde_json::from_value(serde_json::json!({"kind":"fixed_row","hazards_per_year":hazards,"review":{"id":"collision-fixture","revision":"1"},"rationale":"Supplied rapid-transition assumption for collision rejection"})).unwrap();
    }
    let portfolio_model_core::credit_path::DrawStream::Supplied { paths, .. } =
        &mut r.projection.path_source.draw_stream
    else {
        panic!()
    };
    paths[0].pairs = vec![
        portfolio_model_core::credit_path::UniformPair {
            waiting_uniform: (-1.25_f64 * 0.0001).exp(),
            destination_uniform: 0.1,
        },
        portfolio_model_core::credit_path::UniformPair {
            waiting_uniform: (-1.25_f64 * 0.0001).exp(),
            destination_uniform: 0.1,
        },
        portfolio_model_core::credit_path::UniformPair {
            waiting_uniform: (-1.25_f64 * 0.3).exp(),
            destination_uniform: 0.99,
        },
    ];
    assert!(evaluate(&r)
        .unwrap_err()
        .contains("multiple transitions on one mapped day"));
}

#[test]
fn mapped_clock_drives_actual_day_interest_and_recovery_tail_cannot_truncate() {
    let mut r = request(Product::Personal);
    let ProductSource::Installment { contract, .. } = &mut r.products[0] else {
        panic!()
    };
    contract.note_rate = 0.1;
    let out = evaluate(&r).unwrap();
    let ProductResult::Installment {
        result,
        transitions,
        ..
    } = &out.products[0]
    else {
        panic!()
    };
    let days = transitions[0].ordinal - r.projection.path_source.origin_time.ordinal;
    let default = result.default.as_ref().unwrap();
    assert!(
        (default.recognized_accrued_interest - 1000. * 0.1 * f64::from(days) / 365.).abs() < 1e-12
    );
    assert_eq!(default.recovery_ordinal, default.ordinal + 20);
    assert!(out.actual_product_rows <= out.prospective_product_rows);
    let ProductSource::Installment { contract, .. } = &mut r.products[0] else {
        panic!()
    };
    let loan::InstallmentProduct::Personal {
        recovery_delay_days,
        ..
    } = &mut contract.product
    else {
        panic!()
    };
    *recovery_delay_days = 200;
    assert!(evaluate(&r).is_err());
}
