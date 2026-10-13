//! Isolated OSS integration probe, not an application pricing backend.
use chrono::Datelike;
use convex_analytics::spreads::{OASCalculator, ZSpreadCalculator};
use convex_bonds::instruments::{CallableBond, FixedRateBond};
use convex_bonds::options::HullWhite;
use convex_bonds::traits::Bond;
use convex_bonds::types::{CallEntry, CallSchedule, CallType};
use convex_core::types::{Date, Frequency, Spread, SpreadType};
use convex_curves::RateCurveDyn;
use convex_curves::curves::DiscountCurveBuilder;
use finstack_quant_core::cashflow::npv_amounts_with_curve;
use finstack_quant_core::market_data::term_structures::DiscountCurve as FCurve;
use finstack_quant_core::math::interp::InterpStyle;
use finstack_quant_models::credit::pool::{RichardRollPrepay, StochasticPrepayment};
use rayon::prelude::*;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use std::hint::black_box;
use std::time::Instant;
use stochastic_rs_core::simd_rng::Deterministic;
use stochastic_rs_quant::calendar::DayCountConvention as SDayCount;
use stochastic_rs_quant::cashflows::{Cashflow, CashflowPricer, Leg, SimpleCashflow};
use stochastic_rs_quant::curves::{CurvePoint, DiscountCurve as SCurve, InterpolationMethod};
use stochastic_rs_quant::lattice::short_rate::{
    CallableBondSpec, HullWhiteTree, HullWhiteTreeModel, price_callable_bond,
};
use stochastic_rs_stochastic::diffusion::ou::Ou;
use stochastic_rs_stochastic::traits::ProcessExt;

fn timed(mut f: impl FnMut() -> f64, inner: usize) -> Value {
    for _ in 0..3 {
        black_box(f());
    }
    let mut samples = Vec::new();
    let mut checksum = 0.0;
    for _ in 0..11 {
        let start = Instant::now();
        for _ in 0..inner {
            checksum = black_box(f());
        }
        samples.push(start.elapsed().as_secs_f64() * 1e6 / inner as f64);
    }
    samples.sort_by(f64::total_cmp);
    json!({"median_us":samples[5],"p95_us":samples[10],"samples_us":samples,"checksum":checksum})
}
fn batch(n: usize, parallel: bool, price: impl Fn(usize) -> f64 + Sync) -> f64 {
    let values: Vec<f64> = if parallel {
        (0..n).into_par_iter().map(&price).collect()
    } else {
        (0..n).map(&price).collect()
    };
    black_box(&values).iter().sum()
}
fn fdate(d: Date) -> time::Date {
    let n = d.as_naive_date();
    time::Date::from_calendar_date(
        n.year(),
        time::Month::try_from(n.month() as u8).unwrap(),
        n.day() as u8,
    )
    .unwrap()
}
fn scurve(rate: f64) -> SCurve<f64> {
    SCurve::new(
        vec![
            CurvePoint {
                time: 0.0,
                discount_factor: 1.0,
            },
            CurvePoint {
                time: 40.0,
                discount_factor: (-rate * 40.0).exp(),
            },
        ],
        InterpolationMethod::LogLinearOnDiscountFactors,
    )
}
fn prepay_survival(path: &[f64], model: &impl StochasticPrepayment) -> f64 {
    let mut survivor = 1.0;
    let mut burnout = 1.0;
    for (month, &rate) in path.iter().enumerate().skip(1) {
        let smm = model.conditional_smm(60 + month as u32, &[0.0], rate, burnout);
        survivor *= 1.0 - smm;
        burnout = model.update_burnout(burnout, smm, model.expected_smm(60 + month as u32));
    }
    survivor
}
fn main() {
    let threads: usize = std::env::args()
        .nth(1)
        .unwrap_or("4".into())
        .parse()
        .unwrap();
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .unwrap();
    let settle = Date::from_ymd(2026, 1, 15).unwrap();
    let maturity = Date::from_ymd(2056, 1, 15).unwrap();
    let curve = DiscountCurveBuilder::new(settle)
        .add_pillar(0.0, 1.0)
        .add_pillar(40.0, (-0.04_f64 * 40.0).exp())
        .build()
        .unwrap();
    let combined = DiscountCurveBuilder::new(settle)
        .add_pillar(0.0, 1.0)
        .add_pillar(40.0, (-0.05_f64 * 40.0).exp())
        .build()
        .unwrap();
    let fcurve = FCurve::builder("USD")
        .base_date(fdate(settle))
        .knots([(0.0, 1.0), (40.0, (-0.05_f64 * 40.0).exp())])
        .interp(InterpStyle::LogLinear)
        .build()
        .unwrap();
    let scurve = scurve(0.05);
    let spricer = CashflowPricer::new(settle.as_naive_date(), SDayCount::Actual365Fixed);
    let calc = ZSpreadCalculator::new(&curve);
    let mut bonds = Vec::new();
    let mut flows = Vec::new();
    let mut fflows = Vec::new();
    let mut legs = Vec::new();
    let mut reference = Vec::new();
    for i in 0..10_000 {
        let coupon = 0.02 + (i % 41) as f64 * 0.001;
        let bond = FixedRateBond::builder()
            .cusip_unchecked("PROBE0000")
            .coupon_rate(Decimal::try_from(coupon).unwrap())
            .issue_date(settle)
            .maturity(maturity)
            .frequency(Frequency::SemiAnnual)
            .build()
            .unwrap();
        let cf = bond.cash_flows(settle);
        let raw: Vec<(f64, f64)> = cf
            .iter()
            .map(|c| {
                (
                    settle.days_between(&c.date) as f64 / 365.0,
                    c.amount.to_f64().unwrap(),
                )
            })
            .collect();
        let ff: Vec<_> = cf
            .iter()
            .map(|c| (fdate(c.date), c.amount.to_f64().unwrap()))
            .collect();
        let leg = Leg::from_cashflows(
            cf.iter()
                .map(|c| {
                    Cashflow::Simple(SimpleCashflow {
                        payment_date: c.date.as_naive_date(),
                        amount: c.amount.to_f64().unwrap(),
                    })
                })
                .collect(),
        );
        let expected: f64 = raw.iter().map(|(t, a)| a * (-0.05 * t).exp()).sum();
        for actual in [
            calc.price_with_spread(&bond, 0.01, settle),
            npv_amounts_with_curve(&fcurve, fdate(settle), &ff).unwrap(),
            spricer.leg_npv(&leg, &scurve),
        ] {
            assert!(
                (actual - expected).abs() < 1e-9,
                "PV mismatch {actual} != {expected}"
            );
        }
        bonds.push(bond);
        flows.push(raw);
        fflows.push(ff);
        legs.push(leg);
        reference.push(expected);
    }
    let mut batches = Vec::new();
    for n in [1usize, 1_000, 10_000] {
        let inner = if n == 1 { 100 } else { 1 };
        let convex_product = timed(
            || {
                batch(n, false, |i| {
                    calc.price_with_spread(black_box(&bonds[i]), black_box(0.01), settle)
                })
            },
            inner,
        );
        let convex_product_parallel = timed(
            || {
                batch(n, true, |i| {
                    calc.price_with_spread(black_box(&bonds[i]), black_box(0.01), settle)
                })
            },
            inner,
        );
        // Prepared cashflows: curve evaluation remains inside each library call.
        let convex_prepared = timed(
            || {
                batch(n, false, |i| {
                    flows[i]
                        .iter()
                        .map(|(t, a)| {
                            a * RateCurveDyn::discount_factor(&combined, black_box(*t)).unwrap()
                        })
                        .sum()
                })
            },
            inner,
        );
        let finstack = timed(
            || {
                batch(n, false, |i| {
                    npv_amounts_with_curve(&fcurve, fdate(settle), black_box(&fflows[i])).unwrap()
                })
            },
            inner,
        );
        let finstack_parallel = timed(
            || {
                batch(n, true, |i| {
                    npv_amounts_with_curve(&fcurve, fdate(settle), black_box(&fflows[i])).unwrap()
                })
            },
            inner,
        );
        let stochastic = timed(
            || batch(n, false, |i| spricer.leg_npv(black_box(&legs[i]), &scurve)),
            inner,
        );
        let stochastic_parallel = timed(
            || batch(n, true, |i| spricer.leg_npv(black_box(&legs[i]), &scurve)),
            inner,
        );
        // Our own fused adapter. This is NOT an OSS library performance result.
        let packed = timed(
            || {
                batch(n, false, |i| {
                    flows[i]
                        .iter()
                        .map(|(t, a)| a * (-black_box(0.05) * t).exp())
                        .sum()
                })
            },
            inner,
        );
        let packed_parallel = timed(
            || {
                batch(n, true, |i| {
                    flows[i]
                        .iter()
                        .map(|(t, a)| a * (-black_box(0.05) * t).exp())
                        .sum()
                })
            },
            inner,
        );
        batches.push(json!({"instruments":n,"convex_product":convex_product,"convex_product_parallel":convex_product_parallel,"convex_prepared":convex_prepared,"finstack_prepared":finstack,"finstack_prepared_parallel":finstack_parallel,"stochastic_prepared":stochastic,"stochastic_prepared_parallel":stochastic_parallel,"custom_fused_rust":packed,"custom_fused_rust_parallel":packed_parallel}));
    }
    // Reproduce a suspected upstream spread-unit error without changing its code.
    let correct_dv01 = calc.price_with_spread(&bonds[0], 0.01, settle)
        - calc.price_with_spread(&bonds[0], 0.0101, settle);
    let reported_dv01 = calc
        .spread_dv01(
            &bonds[0],
            Spread::new(Decimal::from(100), SpreadType::ZSpread),
            settle,
        )
        .to_f64()
        .unwrap();
    // Explicitly expose settlement-day inclusion differences.
    let due_today = Leg::from_cashflows(vec![Cashflow::Simple(SimpleCashflow {
        payment_date: settle.as_naive_date(),
        amount: 100.0,
    })]);
    let cutoff = json!({"finstack":npv_amounts_with_curve(&fcurve,fdate(settle),&[(fdate(settle),100.0)]).unwrap(),"stochastic":spricer.leg_npv(&due_today,&scurve)});
    // Convex calibrates its HW tree to the supplied curve. Keep this separate
    // from stochastic-rs's constant-theta diagnostic and our rule-based LMM.
    let cbase = FixedRateBond::builder()
        .cusip_unchecked("CALL00000")
        .coupon_percent(6.0)
        .issue_date(settle)
        .maturity(Date::from_ymd(2036, 1, 15).unwrap())
        .frequency(Frequency::SemiAnnual)
        .build()
        .unwrap();
    let mut schedule = CallSchedule::new(CallType::Bermudan);
    for i in 4..20 {
        let d = settle.add_months(i * 6).unwrap();
        schedule = schedule.with_entry(CallEntry::new(d, 100.0).with_end_date(d));
    }
    let cbond = CallableBond::new(cbase, schedule);
    let mut convex_callable = Vec::new();
    for steps in [100, 200, 400] {
        let oas_calc = OASCalculator::new(HullWhite::new(0.1, 0.01), steps);
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            oas_calc.price_with_oas(&cbond, &curve, 0.0, settle)
        }));
        match attempt {
            Ok(Ok(value)) => {
                let straight = calc.price_with_spread(cbond.base_bond(), 0.0, settle);
                assert!(value > 0.0 && value <= straight);
                convex_callable.push(json!({"steps":steps,"price":value,"straight_price":straight,
                    "pricing_including_tree_build":timed(||oas_calc.price_with_oas(black_box(&cbond),&curve,0.0,settle).unwrap(),1)}));
            }
            Ok(Err(error)) => {
                convex_callable.push(json!({"steps":steps,"error":error.to_string()}))
            }
            Err(error) => {
                let message = error
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or("non-string panic".into());
                convex_callable.push(json!({"steps":steps,"panic":message}));
            }
        }
    }
    // Distinguish an unadjusted call-date issue from a general event-grid issue.
    let mut aligned_schedule = CallSchedule::new(CallType::Bermudan);
    for cf in cbond.base_bond().cash_flows(settle) {
        if cf.date >= settle.add_months(24).unwrap()
            && cf.date < Date::from_ymd(2036, 1, 15).unwrap()
        {
            aligned_schedule =
                aligned_schedule.with_entry(CallEntry::new(cf.date, 100.0).with_end_date(cf.date));
        }
    }
    let aligned = CallableBond::new(cbond.base_bond().clone(), aligned_schedule);
    let aligned_calc = OASCalculator::new(HullWhite::new(0.1, 0.01), 200);
    let aligned_attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        aligned_calc.price_with_oas(&aligned, &curve, 0.0, settle)
    }));
    let aligned_result = match aligned_attempt {
        Ok(Ok(value)) => json!({"price":value}),
        Ok(Err(error)) => json!({"error":error.to_string()}),
        Err(_) => json!({"panic":true}),
    };
    // Standalone callable diagnostic: constant-theta Vasicek model, NOT our LMM.
    let model = HullWhiteTreeModel::new(0.04, 0.1, 0.04, 0.01);
    let mut callable = Vec::new();
    for steps in [100, 200, 400] {
        let tree = HullWhiteTree::new(model.clone(), 10.0, steps);
        let spec = CallableBondSpec::new(100.0, 0.06, (1..=20).map(|i| i as f64 * 0.5).collect())
            .with_calls((4..20).map(|i| (i as f64 * 0.5, 100.0)).collect());
        let value = price_callable_bond(&tree.tree, &tree.model, &spec);
        assert!(
            value.price.is_finite() && value.price > 0.0 && value.price <= value.straight_price
        );
        let pricing = timed(
            || price_callable_bond(black_box(&tree.tree), &tree.model, &spec).price,
            1,
        );
        callable.push(json!({"steps":steps,"price":value.price,"straight_price":value.straight_price,"pricing_with_reused_tree":pricing}));
    }
    // Seeded OU is a smoke/performance diagnostic, not an LMM speed comparison.
    let sample = || {
        Ou::<f64, _>::new(
            0.2,
            0.04,
            0.01,
            361,
            Some(0.04),
            Some(30.0),
            Deterministic::new(42),
        )
        .sample_map(4096, |p| p[p.len() - 1])
    };
    let first = sample();
    let second = sample();
    assert_eq!(first, second, "seeded batch not reproducible");
    let retained = Ou::<f64, _>::new(
        0.2,
        0.04,
        0.01,
        361,
        Some(0.04),
        Some(30.0),
        Deterministic::new(42),
    )
    .sample_par(4096);
    let retained_terminals: Vec<_> = retained.iter().map(|p| p[p.len() - 1]).collect();
    assert_eq!(
        first, retained_terminals,
        "retained and mapped paths differ"
    );
    let bumped = Ou::<f64, _>::new(
        0.2,
        0.045,
        0.01,
        361,
        Some(0.04),
        Some(30.0),
        Deterministic::new(42),
    )
    .sample_map(4096, |p| p[p.len() - 1]);
    let expected_shift = 0.005 * (1.0 - (1.0_f64 - 0.2 * 30.0 / 360.0).powi(360));
    let crn_max_error = first
        .iter()
        .zip(&bumped)
        .map(|(a, b)| (b - a - expected_shift).abs())
        .fold(0.0, f64::max);
    assert!(crn_max_error < 1e-12, "OU bump lost CRN coupling");
    let pool1 = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let single = pool1.install(sample);
    let ou_cross_threads = first == single;
    let mean = first.iter().sum::<f64>() / first.len() as f64;
    let var = first.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (first.len() - 1) as f64;
    let dt = 30.0 / 360.0;
    let decay = 1.0 - 0.2 * dt;
    let expected_var = 0.01_f64.powi(2) * dt * (1.0 - decay.powi(720)) / (1.0 - decay.powi(2));
    assert!((mean - 0.04).abs() < 5.0 * (expected_var / 4096.0).sqrt());
    assert!((var / expected_var - 1.0).abs() < 0.12);
    let ou_timing = timed(|| sample().iter().sum(), 1);
    // Behavioral primitive on shared paths. Not our application MBS cashflow model.
    let prepay = RichardRollPrepay::new(0.06, 2.0, 0.05, 0.1);
    let smm_low = prepay.conditional_smm(60, &[0.0], 0.03, 1.0);
    let smm_at = prepay.conditional_smm(60, &[0.0], 0.05, 1.0);
    let smm_high = prepay.conditional_smm(60, &[0.0], 0.07, 1.0);
    assert!(smm_low > smm_at && smm_at > smm_high && smm_high >= 0.0 && smm_low <= 1.0);
    assert!((smm_at - (1.0 - 0.94_f64.powf(1.0 / 12.0))).abs() < 1e-12);
    let prepay_base = retained[..512]
        .iter()
        .map(|p| prepay_survival(p.as_slice().unwrap(), &prepay))
        .sum::<f64>()
        / 512.0;
    let edited_prepay = RichardRollPrepay::new(0.08, 2.0, 0.05, 0.1);
    let prepay_edit = retained[..512]
        .iter()
        .map(|p| prepay_survival(p.as_slice().unwrap(), &edited_prepay))
        .sum::<f64>()
        / 512.0;
    assert!(prepay_edit < prepay_base && prepay_edit >= 0.0 && prepay_base <= 1.0);
    let prepay_timing = timed(
        || {
            retained[..512]
                .par_iter()
                .map(|p| prepay_survival(p.as_slice().unwrap(), black_box(&prepay)))
                .sum::<f64>()
                / 512.0
        },
        1,
    );
    let fixture: Vec<_> = flows.iter().take(41).map(|cf| json!(cf)).collect();
    let result = json!({"threads":threads,"instrument_count":10000,"cashflows_per_instrument":flows[0].len(),"pv_tolerance":1e-9,"all_pvs_match":true,"batches":batches,
        "convex_spread_dv01":{"spread_bp":100,"reported":reported_dv01,"finite_difference":correct_dv01,"relative_error":reported_dv01/correct_dv01-1.0},
        "settlement_day_cashflow":cutoff,"stochastic_callable_constant_theta":callable,"convex_callable_curve_fitted":convex_callable,"convex_callable_coupon_aligned_200_steps":aligned_result,
        "ou":{"paths":4096,"points":361,"same_seed_reproducible":true,"thread_count_invariant":ou_cross_threads,"retained_equals_mapped":true,"crn_bump_max_abs_error":crn_max_error,"terminal_mean":mean,"terminal_variance":var,"euler_expected_variance":expected_var,"timing":ou_timing},
        "finstack_prepay_primitive":{"paths":512,"months":360,"smm_rate_3pct":smm_low,"smm_rate_5pct":smm_at,"smm_rate_7pct":smm_high,"survival_base_cpr_6pct":prepay_base,"survival_edited_cpr_8pct":prepay_edit,"parallel_timing":prepay_timing},
        "fixture_cashflows":fixture,"reference_pvs_first_41":&reference[..41]});
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}
