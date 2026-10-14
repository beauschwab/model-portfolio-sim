//! Checked atomic ABI dispatch for the built-in financial kernels.
use super::abi_operations::*;
use super::buffers::Buffer;
use super::market::market_paths;
use super::validation::validate;
use crate::income::kernels::accrual;
use crate::income::kernels::income;
use crate::math::arrays::View;
use crate::models::behavioral::behavioral;
use crate::models::rates::lmm;
use crate::models::volatility::volatility;
use crate::portfolio::forward_cashflows::program;
use crate::portfolio::units::evaluate::interactive;
use crate::pricing::discount::oas;
use crate::products::deposits::kernels::deposits;
use crate::products::derivatives::swaption::swaption;
use crate::products::money_market::money_market;
use crate::products::mortgages::kernels::mortgage;
use crate::products::mortgages::kernels::mortgage_batch;
use crate::products::term::kernel::corporate;
use std::slice;

#[no_mangle]
pub extern "C" fn portfolio_quant_abi_version() -> u32 {
    ABI_VERSION
}

/// Status 0 success, 1 invalid arrays/model/nonconvergence, 2 nonfinite output,
/// 3 pool error. No output buffer is modified on failure.
///
/// # Safety
/// Descriptors and their aligned data must be live for the declared sizes.
/// Inputs must not overlap outputs; outputs must be exclusively owned and not
/// overlap each other. No pointer is retained after the synchronous call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_quant_call(
    op: u32,
    inputs: *const Buffer,
    ni: usize,
    outputs: *const Buffer,
    no: usize,
    threads: usize,
) -> i32 {
    if inputs.is_null() || outputs.is_null() || ni > 64 || no > 16 || threads == 0 || threads > 256
    {
        return 1;
    }
    let result = std::panic::catch_unwind(|| {
        // SAFETY: caller supplies live descriptor arrays, per ABI contract.
        let input = unsafe { slice::from_raw_parts(inputs, ni) };
        let output = unsafe { slice::from_raw_parts(outputs, no) };
        let mut views = Vec::with_capacity(ni);
        for b in input {
            if b.data.is_null()
                || b.len > isize::MAX as usize / 8
                || b.shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b)) != Some(b.len)
            {
                return Err(1);
            }
            // SAFETY: validated descriptor length; ownership required by caller.
            let v = unsafe { slice::from_raw_parts(b.data, b.len) };
            if v.iter().any(|v| v.is_nan()) {
                return Err(1);
            }
            views.push(View { v, shape: b.shape });
        }
        validate(op, &views);
        let compute = || match op {
            LMM => lmm(&views),
            CORPORATE => corporate(&views, false),
            CD => corporate(&views, true),
            MORTGAGE => mortgage(&views, false),
            MORTGAGE_STRESS => mortgage(&views, true),
            DEPOSIT => deposits(&views, false),
            DEPOSIT_STRESS => deposits(&views, true),
            CSR_OAS => oas(&views, true),
            CSR_PV => oas(&views, false),
            MORTGAGE_BATCH => mortgage_batch(&views),
            BEHAVIORAL => behavioral(&views),
            SWAPTION => swaption(&views),
            PROGRAM => program(&views),
            INCOME => income(&views),
            ACCRUAL => accrual(&views),
            MONEY_MARKET => money_market(&views),
            VOLATILITY => volatility(&views),
            CURVE => vec![crate::market::bootstrap(
                views[0].v,
                views[1].v,
                views[2].ix(0),
                views[3].at(0),
            )
            .expect("invalid curve")],
            RATE_PATHS | MORTGAGE_PATHS => market_paths(&views, op == MORTGAGE_PATHS),
            FIT_CC => {
                let fit = crate::models::calibration::fit_current_coupon(views[0].v)
                    .expect("invalid current coupon fit");
                vec![fit.beta, vec![fit.lambda, fit.r_squared]]
            }
            FIT_PS => vec![
                crate::models::calibration::fit_ps_spread(views[0].v, views[1].at(0))
                    .expect("invalid spread fit")
                    .to_vec(),
            ],
            FACTOR_LOADINGS => vec![crate::models::calibration::factor_loadings(
                views[0].ix(0),
                views[1].ix(0),
                views[2].at(0),
                views[3].at(0),
            )
            .expect("invalid factor loadings")],
            FIT_VOL | FIT_DEPOSIT => {
                let fit = if op == FIT_VOL {
                    crate::models::calibration::fit_abcd(
                        views[0].v,
                        views[1].v,
                        views[2].v,
                        views[3].v,
                        views[4].v,
                        [views[5].at(0), views[5].at(1)],
                    )
                } else {
                    crate::models::calibration::fit_deposits(views[0].v)
                }
                .expect("invalid nonlinear fit");
                vec![
                    fit.x,
                    vec![fit.rmse, fit.evaluations as f64, fit.status as f64],
                ]
            }
            SHARED_DRAWS => {
                let seed: Vec<_> = views[0].v.iter().map(|&v| v as u32).collect();
                let out = crate::math::random::SharedDraws::new(
                    &seed,
                    [views[1].ix(0), views[1].ix(1), views[1].ix(2)],
                )
                .expect("invalid shared draws");
                vec![out.rates, out.spread, out.hpi]
            }
            MORTGAGE_RISK | MORTGAGE_FORWARD_STRESS => {
                use crate::products::mortgages::risk::{
                    MortgageRiskRequest, PrepayData, RiskConfig,
                };
                let a = &views;
                let c = &a[8];
                let seed: Vec<_> = a[7].v.iter().map(|&v| v as u32).collect();
                let request = MortgageRiskRequest {
                    tenors: a[0].v,
                    swap_rates: a[1].v,
                    vol_quotes: a[2].v,
                    cc_history: a[3].v,
                    ps_history: a[4].v,
                    book: a[5].v,
                    original_hpi: a[6].v,
                    prepay_multiplier: a[if op == 28 { 25 } else { 27 }].v,
                    seed: &seed,
                    fixed_oas: a[9].v,
                    config: RiskConfig {
                        base_paths: c.ix(0),
                        sensitivity_paths: c.ix(1),
                        months: c.ix(2),
                        forwards: c.ix(3),
                        factors: c.ix(4),
                        dt: c.at(5),
                        tenor: c.at(6),
                        shift: c.at(7),
                        float32_paths: c.at(8) != 0.,
                        curve_bump: c.at(9),
                        vol_bump: c.at(10),
                        hpi: [c.at(11), c.at(12), c.at(13)],
                        incentive_lag: c.ix(14),
                        ps_spot: c.at(15),
                        rational_sigmoid: c.at(16) != 0.,
                    },
                    prepay: PrepayData {
                        month_of_year: a[10].v,
                        seasonality: a[11].v,
                        parameters: a[12].v,
                        ltv_knots: a[13].v,
                        ltv_coefficients: a[14].v,
                        smm_table: a[15].v,
                        smm_scale: a[16].at(0),
                        burnout_table: a[17].v,
                        burnout_scale: a[16].at(1),
                        cc_vol_points: a[18].v,
                        fico_x: a[19].v,
                        fico_y: a[20].v,
                        size_x: a[21].v,
                        size_y: a[22].v,
                        state_multipliers: a[23].v,
                        channel_multipliers: a[24].v,
                    },
                };
                if op == MORTGAGE_RISK {
                    let out = request.run().expect("invalid mortgage risk lifecycle");
                    vec![out.oas, out.price, out.dv01, out.sensitivities]
                } else {
                    let horizons: Vec<_> = (0..a[25].v.len()).map(|i| a[25].ix(i)).collect();
                    let out = request
                        .run_stress(&horizons, a[26].v)
                        .expect("invalid mortgage stress lifecycle");
                    vec![
                        out.base_value,
                        out.base_price,
                        out.shock_value,
                        out.pnl,
                        out.aggregate_base,
                        out.aggregate_pnl,
                        out.forward_dv01,
                        out.oas,
                    ]
                }
            }
            PARALLEL_SHOCK => {
                let a = &views;
                let shock = crate::market::ParallelShock {
                    horizon: a[3].ix(0),
                    shock_bp: a[3].at(1),
                    dt: a[3].at(2),
                    incentive_lag: a[3].ix(3),
                    cc_lambda: a[3].at(4),
                    hpi_beta: a[3].at(5),
                    cc_rate_beta_sum: a[4].v[..4].iter().sum(),
                };
                let out = shock
                    .apply(a[0].v, a[1].v, a[2].v, [a[0].shape[0], a[0].shape[1]])
                    .expect("invalid parallel shock");
                vec![out.mtg, out.hpi, out.yoy, out.df]
            }
            MORTGAGE_CACHE => vec![crate::market::cache::statistics(views[0].at(0) != 0.).to_vec()],
            INTERACTIVE => interactive(&views),
            _ => panic!("unsupported operation"),
        };
        // These operations contain no parallel work. A worker handoff costs more
        // than the coefficient reduction and worsens interactive tail latency.
        let result = if matches!(op, MORTGAGE_CACHE | INTERACTIVE) {
            compute()
        } else {
            crate::runtime::compute::pool(threads)?.install(compute)
        };
        if result.len() != no {
            return Err(1);
        }
        for (values, out) in result.iter().zip(output) {
            if out.data.is_null()
                || values.len() != out.len
                || out.shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b)) != Some(out.len)
            {
                return Err(1);
            }
            if values.iter().any(|x| !x.is_finite()) {
                return Err(2);
            }
        }
        for (values, out) in result.iter().zip(output) {
            // SAFETY: all lengths validated before the first write; caller owns output.
            unsafe {
                std::ptr::copy_nonoverlapping(values.as_ptr(), out.data, values.len());
            }
        }
        Ok(())
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(code)) => code,
        Err(_) => 1,
    }
}
