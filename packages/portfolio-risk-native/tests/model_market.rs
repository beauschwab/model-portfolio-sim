use portfolio_risk_native::model_market::{curves::*, joint::*, smile::*, *};
#[test]
fn public_synthetic_example_has_independent_boundary_math() {
    let r: ModelMarketRequest =
        serde_json::from_str(include_str!("../../../examples/models/model-market-1.json")).unwrap();
    let o = analyze(&r).unwrap();
    near(o.curve_results[0].collateral_discount_factor, 0.95, 0.);
    near(o.curve_results[0].index_basis, 1. / 0.94 - 1. / 0.95, 1e-15);
    near(
        o.option_results[0].value,
        0.01 / (2. * std::f64::consts::PI).sqrt(),
        1e-15,
    );
    near(
        o.joint.unwrap().increments[0],
        0.1257302210933933 * 0.005,
        1e-15,
    );
}
fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b}");
}
fn curve(id: &str, role: CurveRole) -> SuppliedCurve {
    SuppliedCurve {
        curve_id: id.into(),
        source_id: "synthetic-source".into(),
        source_revision: "v1".into(),
        currency: "USD".into(),
        role,
        times_years: vec![0., 1., 2.],
        discount_factors: vec![1., 0.95, 0.85],
        right_tail: CurveTail::Reject,
    }
}
fn surface() -> SmileSurface {
    SmileSurface {
        surface_id: "synthetic-smile".into(),
        source_id: "controlled-quotes".into(),
        source_revision: "v1".into(),
        convention: VolConvention::Normal {},
        expiries_years: vec![1., 2.],
        tenors_years: vec![1., 5.],
        strikes: vec![-0.01, 0., 0.01],
        volatilities: vec![
            0.01, 0.012, 0.016, 0.02, 0.024, 0.032, 0.02, 0.024, 0.032, 0.04, 0.048, 0.064,
        ],
        expiry_tail: VolTail::Reject,
        tenor_tail: VolTail::Reject,
        strike_tail: VolTail::Reject,
    }
}
fn joint() -> JointGaussianRequest {
    JointGaussianRequest {
        source_revision: "synthetic-covariance-1".into(),
        shared_draw_id: "native-seed-zero-grid-v1".into(),
        drivers: vec![
            JointDriver {
                driver_id: "rate-1y".into(),
                kind: DriverKind::RateFactor { tenor_years: 1. },
            },
            JointDriver {
                driver_id: "mortgage-spread".into(),
                kind: DriverKind::MortgageSpread {},
            },
            JointDriver {
                driver_id: "hpi-region-a".into(),
                kind: DriverKind::RegionalLogHpi {
                    region_id: "region-a".into(),
                },
            },
        ],
        annual_covariance: vec![0.0001, 0., 0., 0., 0.0004, 0., 0., 0., 0.01],
        durations_years: vec![0.25],
        paths: 4,
        retained_factors: 3,
        draws: DrawSource::SuppliedStandardNormals {
            values: vec![1., 1., 1., 1., -1., -1., -1., 1., -1., -1., -1., 1.],
        },
    }
}

#[test]
fn exact_curve_nodes_logdf_interpolation_and_actual_forward_basis() {
    let c = curve(
        "collateral",
        CurveRole::Collateral {
            collateral_index: "SOFR".into(),
        },
    );
    for (t, df) in c.times_years.iter().zip(&c.discount_factors) {
        assert_eq!(c.value(*t).unwrap().discount_factor, *df);
    }
    near(
        c.value(1.5).unwrap().discount_factor,
        (0.95_f64 * 0.85).sqrt(),
        1e-15,
    );
    let mut i = curve(
        "index",
        CurveRole::Index {
            index_id: "TERM-SOFR-3M".into(),
        },
    );
    i.discount_factors = vec![1., 0.94, 0.83];
    let q = ForwardQuery {
        collateral_curve_id: "collateral".into(),
        index_curve_id: "index".into(),
        start_year: 0.,
        end_year: 1.,
        accrual_fraction: 1.,
    };
    let out = forward_query(&[c, i], &q).unwrap();
    near(out.collateral_implied_forward, 1. / 0.95 - 1., 1e-15);
    near(out.index_forward, 1. / 0.94 - 1., 1e-15);
    near(out.index_basis, 1. / 0.94 - 1. / 0.95, 1e-15);
}
#[test]
fn curve_tails_are_explicit_and_flat_zero_differs_from_flat_forward() {
    let mut c = curve(
        "c",
        CurveRole::Index {
            index_id: "index".into(),
        },
    );
    assert!(c.value(3.).is_err());
    c.right_tail = CurveTail::FlatZero;
    let z = c.value(3.).unwrap();
    near(z.discount_factor, 0.85_f64.powf(1.5), 1e-15);
    assert!(z.extrapolated);
    c.right_tail = CurveTail::FlatForward;
    near(
        c.value(3.).unwrap().discount_factor,
        0.85 * 0.85 / 0.95,
        1e-15,
    );
}
#[test]
fn negative_rates_allow_increasing_df_and_negative_index_forwards() {
    let mut c = curve(
        "c",
        CurveRole::Index {
            index_id: "negative-index".into(),
        },
    );
    c.discount_factors = vec![1., 0.01_f64.exp(), 0.02_f64.exp()];
    let (f, tail) = c.simple_forward(0., 1., 1.).unwrap();
    near(f, (-0.01_f64).exp() - 1., 1e-15);
    assert!(!tail);
    assert!(c.value(1.5).unwrap().discount_factor > 1.);
}
#[test]
fn curve_admission_rejects_missing_anchor_duplicate_nodes_roles_and_unstable_tails() {
    let mut c = curve(
        "c",
        CurveRole::Index {
            index_id: "i".into(),
        },
    );
    c.discount_factors[0] = 0.99;
    assert!(c.validate().is_err());
    c.discount_factors[0] = 1.;
    c.times_years[2] = 1.;
    assert!(c.validate().is_err());
    c.times_years[2] = 2.;
    c.discount_factors[1] = f64::NAN;
    assert!(c.validate().is_err());
    c.discount_factors[1] = 1e-100;
    c.discount_factors[2] = 1e100;
    c.right_tail = CurveTail::FlatForward;
    assert!(c.value(100.).is_err());
    assert!(c.simple_forward(1., 0., 0.5).is_err());
}
#[test]
fn supplied_absolute_strike_quotes_reproduce_nodes_and_interpolate_total_variance() {
    let s = surface();
    let q = OptionQuery {
        expiry_years: 1.5,
        tenor_years: 3.,
        strike: 0.005,
        forward: 0.,
        payoff_unit: 1.,
        side: OptionSide::Call,
    };
    let out = s.price(&q).unwrap();
    near(out.volatility, 0.0015_f64.sqrt(), 1e-15);
    assert!(out.interpolated);
    let q = OptionQuery {
        expiry_years: 1.,
        tenor_years: 1.,
        strike: 0.,
        ..q
    };
    let node = s.price(&q).unwrap();
    near(node.volatility, 0.012, 1e-15);
    assert!(!node.interpolated);
    let wing = OptionQuery { strike: 0.01, ..q };
    assert!(s.price(&wing).unwrap().volatility > node.volatility);
}
#[test]
fn vol_tails_and_normal_black_units_are_not_silently_mixed() {
    let mut s = surface();
    let q = OptionQuery {
        expiry_years: 3.,
        tenor_years: 1.,
        strike: 0.,
        forward: 0.,
        payoff_unit: 1.,
        side: OptionSide::Call,
    };
    assert!(s.price(&q).is_err());
    s.expiry_tail = VolTail::FlatVolatility;
    let out = s.price(&q).unwrap();
    near(out.volatility, 0.024, 1e-15);
    assert!(out.expiry_tail_used);
    s.convention = VolConvention::ShiftedLognormal { shift: 0.03 };
    let black = s.price(&q).unwrap();
    assert!(black.value < out.value);
    assert_eq!(
        black.convention,
        VolConvention::ShiftedLognormal { shift: 0.03 }
    );
    let mut bad = s;
    bad.convention = VolConvention::ShiftedLognormal { shift: 0.005 };
    assert!(bad.validate().is_err());
}
#[test]
fn normal_option_analytical_atm_otm_negative_rates_and_put_call_parity() {
    let c = VolConvention::Normal {};
    near(
        option_value(-0.01, -0.01, 0.01, 2., c, OptionSide::Call).unwrap(),
        2. * 0.01 / (2. * std::f64::consts::PI).sqrt(),
        1e-16,
    );
    near(
        option_value(0., 0.01, 0.01, 1., c, OptionSide::Call).unwrap(),
        0.000833154705876863,
        1e-16,
    );
    near(
        option_value(0., 0.035, 0.01, 1., c, OptionSide::Call).unwrap(),
        0.00000058480918421416,
        1e-17,
    );
    let call = option_value(-0.01, 0.01, 0.02, 100., c, OptionSide::Call).unwrap();
    let put = option_value(-0.01, 0.01, 0.02, 100., c, OptionSide::Put).unwrap();
    near(call - put, -2., 1e-14);
}
#[test]
fn shifted_black_atm_negative_rates_small_vol_and_deterministic_limits() {
    let c = VolConvention::ShiftedLognormal { shift: 0.03 };
    near(
        option_value(-0.01, -0.01, 0.2, 1., c, OptionSide::Call).unwrap(),
        0.00159311349108116,
        1e-16,
    );
    near(
        option_value(0.02, 0.02, 1e-16, 1., c, OptionSide::Call).unwrap(),
        0.05 * 1e-16 / (2. * std::f64::consts::PI).sqrt(),
        1e-32,
    );
    let call = option_value(0.02, 0.01, 0.3, 1., c, OptionSide::Call).unwrap();
    let put = option_value(0.02, 0.01, 0.3, 1., c, OptionSide::Put).unwrap();
    near(call - put, 0.01, 1e-16);
    near(
        option_value(0.02, 0.01, 0., 1., c, OptionSide::Call).unwrap(),
        0.01,
        0.,
    );
    assert!(option_value(-0.04, -0.01, 0.2, 1., c, OptionSide::Call).is_err());
    near(
        option_value(
            -0.02,
            0.01,
            1e-320,
            1.,
            VolConvention::Normal {},
            OptionSide::Call,
        )
        .unwrap(),
        0.,
        0.,
    );
}
#[test]
fn invalid_smile_grids_shift_payoff_and_unknown_fields_fail() {
    let mut s = surface();
    s.volatilities.pop();
    assert!(s.validate().is_err());
    s = surface();
    s.strikes[1] = s.strikes[0];
    assert!(s.validate().is_err());
    s = surface();
    s.volatilities[0] = -1.;
    assert!(s.validate().is_err());
    assert!(option_value(
        0.,
        0.,
        f64::NAN,
        1.,
        VolConvention::Normal {},
        OptionSide::Call
    )
    .is_err());
    assert!(option_value(0., 0., 1., 1e15, VolConvention::Normal {}, OptionSide::Call).is_ok());
    assert!(serde_json::from_value::<VolConvention>(
        serde_json::json!({"kind":"normal","atm_only":true})
    )
    .is_err());
}
#[test]
fn controlled_joint_draws_reproduce_independent_covariance_and_unit_scaling() {
    let r = joint();
    let out = simulate(&r).unwrap();
    near(out.increments[0], 0.005, 1e-16);
    near(out.increments[1], 0.01, 1e-16);
    near(out.increments[2], 0.05, 1e-16);
    for i in 0..3 {
        near(out.sample_mean_annualized_innovation[i], 0., 1e-16);
    }
    for (actual, expected) in out
        .sample_annual_covariance
        .iter()
        .zip(&r.annual_covariance)
    {
        near(*actual, *expected, 1e-15);
    }
    near(out.diagnostics.discarded_covariance_frobenius, 0., 1e-15);
}
#[test]
fn rank_deficient_perfect_joint_correlation_is_supported_without_independent_hpi() {
    let mut r = joint();
    r.annual_covariance = vec![
        0.0001, 0.0002, 0.001, 0.0002, 0.0004, 0.002, 0.001, 0.002, 0.01,
    ];
    let out = simulate(&r).unwrap();
    for (a, b) in out
        .diagnostics
        .retained_annual_covariance
        .iter()
        .zip(&r.annual_covariance)
    {
        near(*a, *b, 1e-15);
    }
    for v in out.increments.chunks(3) {
        near(v[1], 2. * v[0], 1e-14);
        near(v[2], 10. * v[0], 1e-14);
    }
}
#[test]
fn covariance_psd_checks_tiny_axes_zero_rows_asymmetry_and_indefinite_blocks() {
    let mut r = joint();
    r.annual_covariance[1] = 0.1;
    assert!(simulate(&r).is_err());
    r = joint();
    r.annual_covariance = vec![1., 0.9, 0.9, 0.9, 1., -0.9, 0.9, -0.9, 1.];
    assert!(simulate(&r).is_err());
    r.annual_covariance = vec![1., 2e-15, 0., 2e-15, 1e-30, 0., 0., 0., 1.];
    assert!(simulate(&r).is_err());
    r.annual_covariance = vec![0., 1e-100, 0., 1e-100, 1., 0., 0., 0., 1.];
    assert!(simulate(&r).is_err());
    r = joint();
    r.annual_covariance[0] = -1e-30;
    assert!(simulate(&r).is_err());
}
#[test]
fn native_shared_draws_are_reproducible_antithetic_and_covariance_changes_keep_crn() {
    let mut r = joint();
    r.draws = DrawSource::NativeSeed { seed: vec![0] };
    let a = simulate(&r).unwrap();
    let b = simulate(&r).unwrap();
    assert_eq!(a.increments, b.increments);
    near(a.increments[0], 0.1257302210933933 * 0.005, 1e-17);
    for j in 0..3 {
        assert_eq!(a.increments[6 + j], -a.increments[j]);
        assert_eq!(a.increments[9 + j], -a.increments[3 + j]);
    }
    for c in &mut r.annual_covariance {
        *c *= 4.;
    }
    let stress = simulate(&r).unwrap();
    for (x, y) in a.increments.iter().zip(stress.increments) {
        near(y, 2. * x, 1e-15);
    }
}
#[test]
fn controlled_timestep_aggregation_preserves_exact_gaussian_terminal_innovation() {
    let mut fine = joint();
    fine.paths = 1;
    fine.durations_years = vec![0.25, 0.75];
    fine.draws = DrawSource::SuppliedStandardNormals {
        values: vec![1., 2., 3., -1., -2., -3.],
    };
    let f = simulate(&fine).unwrap();
    let mut coarse = fine;
    coarse.durations_years = vec![1.];
    let weighted = 0.5 - 0.75_f64.sqrt();
    coarse.draws = DrawSource::SuppliedStandardNormals {
        values: vec![weighted, 2. * weighted, 3. * weighted],
    };
    let c = simulate(&coarse).unwrap();
    for (a, b) in f.terminal_changes.iter().zip(c.terminal_changes) {
        near(*a, b, 1e-15);
    }
}
#[test]
fn factor_diagnostics_report_discarded_covariance_with_same_draw_grid() {
    let r = joint();
    let full = simulate(&r).unwrap();
    let mut reduced = r;
    reduced.retained_factors = 2;
    let low = simulate(&reduced).unwrap();
    near(low.diagnostics.discarded_covariance_frobenius, 0.01, 1e-15);
    assert_eq!(low.shape, full.shape);
    near(low.increments[0], full.increments[0], 1e-16);
    near(low.increments[1], full.increments[1], 1e-16);
    near(low.increments[2], 0., 1e-16);
}
#[test]
fn joint_domain_identity_work_and_regional_dimensions_are_strict() {
    let mut r = joint();
    r.paths = 100000;
    r.durations_years = vec![0.25; 100];
    assert!(simulate(&r).is_err());
    r = joint();
    r.drivers[2].driver_id = r.drivers[0].driver_id.clone();
    assert!(simulate(&r).is_err());
    r = joint();
    r.retained_factors = 4;
    assert!(simulate(&r).is_err());
    r = joint();
    r.durations_years[0] = 0.;
    assert!(simulate(&r).is_err());
    r = joint();
    r.draws = DrawSource::SuppliedStandardNormals {
        values: vec![0.; 3],
    };
    assert!(simulate(&r).is_err());
    assert!(serde_json::from_value::<DriverKind>(
        serde_json::json!({"kind":"mortgage_spread","independent_fallback":true})
    )
    .is_err());
}
#[test]
fn full_analysis_contract_reproduces_supplied_queries_without_legacy_switch() {
    let request = ModelMarketRequest {
        schema: SCHEMA.into(),
        identity: MarketModelIdentity {
            model_id: "synthetic-multicurve-smile-joint".into(),
            model_version: SCHEMA.into(),
            input_revision: "synthetic-v1".into(),
            assumption_id: "controlled-fixture".into(),
            rationale: "Synthetic fixture; no empirical calibration or model switch".into(),
            as_of_ordinal: 738916,
            currency: "USD".into(),
        },
        curves: vec![
            curve(
                "collateral",
                CurveRole::Collateral {
                    collateral_index: "SOFR".into(),
                },
            ),
            curve(
                "index",
                CurveRole::Index {
                    index_id: "TERM-SOFR-3M".into(),
                },
            ),
        ],
        curve_queries: vec![ForwardQuery {
            collateral_curve_id: "collateral".into(),
            index_curve_id: "index".into(),
            start_year: 0.,
            end_year: 1.,
            accrual_fraction: 1.,
        }],
        smile: Some(surface()),
        option_queries: vec![OptionQuery {
            expiry_years: 1.,
            tenor_years: 1.,
            strike: 0.,
            forward: 0.,
            payoff_unit: 1.,
            side: OptionSide::Call,
        }],
        joint: Some(joint()),
    };
    let out = analyze(&request).unwrap();
    near(out.curve_results[0].index_basis, 0., 0.);
    near(
        out.option_results[0].value,
        0.012 / (2. * std::f64::consts::PI).sqrt(),
        1e-16,
    );
    assert_eq!(out.identity.input_revision, "synthetic-v1");
    let mut bad = serde_json::to_value(request).unwrap();
    bad["use_legacy_atm_fit"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ModelMarketRequest>(bad).is_err());
}

#[test]
fn valid_nonbinary_variances_do_not_fail_from_diagonal_normalization_roundoff() {
    let cov = [
        0.03_f64.powi(2),
        0.,
        0.,
        0.,
        0.07_f64.powi(2),
        0.,
        0.,
        0.,
        0.21_f64.powi(2),
    ];
    let (_, d) = covariance_root(&cov, 3, 3).unwrap();
    for (a, b) in cov.iter().zip(d.retained_annual_covariance) {
        near(*a, b, 1e-15);
    }
}
