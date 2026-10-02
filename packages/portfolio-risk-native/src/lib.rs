//! Native cashflow/pricing kernels and market-path lifecycle. Rust market contexts
//! own curve bootstrap and path stages. Calibration and seeded shared Gaussian
//! tape generation, bounded dependency graphs and public controllers are native.
use rayon::prelude::*;
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
pub mod accounting_lifecycle;
pub mod balance_risk;
pub mod cache;
pub mod calibration;
pub mod cohorts;
pub mod controllers;
pub mod conventions;
pub mod dated_term;
pub mod decision_inputs;
pub mod deposit_lifecycle;
pub mod forecast_input;
pub mod forecast_lifecycle;
mod graph_cache;
pub mod hedge_lifecycle;
pub mod incremental;
pub mod kpi_lifecycle;
mod least_squares;
pub mod ledger_mapping;
pub mod lifecycle_market;
pub mod market;
mod market_cache;
pub mod model_lifecycle;
pub mod mortgage_input;
pub mod mortgage_risk;
#[cfg(feature = "compute-profile")]
pub mod profile;
pub mod program_lifecycle;
pub mod treasury;
pub mod treasury_bridge;

#[macro_export]
macro_rules! compute_span {
    ($name:literal) => {
        #[cfg(feature = "compute-profile")]
        let _compute_span = $crate::profile::Span::new($name);
    };
}
mod quant;
pub mod random;
pub mod term_deck;
pub mod term_protocol;
pub mod term_risk;
pub mod unit_lifecycle;
pub mod whatif;
mod ziggurat;

type PoolSlot = Option<(usize, Arc<rayon::ThreadPool>)>;
static POOL: OnceLock<Mutex<PoolSlot>> = OnceLock::new();

fn pool(threads: usize) -> Result<Arc<rayon::ThreadPool>, i32> {
    let mut slot = POOL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| 3)?;
    if let Some((n, value)) = slot.as_ref() {
        if *n == threads {
            return Ok(value.clone());
        }
    }
    let value = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|_| 3)?,
    );
    *slot = Some((threads, value.clone()));
    Ok(value)
}

fn price(
    offsets: &[i64],
    times: &[f64],
    values: &[f64],
    oas: &[f64],
    paths: usize,
    threads: usize,
    output: &mut [f64],
) -> Result<(), i32> {
    let n = oas.len();
    if paths == 0
        || threads == 0
        || threads > 256
        || offsets.len() != n + 1
        || output.len() != n
        || times.len() != values.len()
        || offsets[0] != 0
        || offsets[n] != values.len() as i64
        || offsets.windows(2).any(|w| w[0] < 0 || w[1] <= w[0])
        || times.iter().any(|v| !v.is_finite() || *v < 0.0)
        || values.iter().chain(oas).any(|v| !v.is_finite())
    {
        return Err(1);
    }
    let calculate = |i: usize| {
        let mut sum = 0.0;
        for j in offsets[i] as usize..offsets[i + 1] as usize {
            sum += values[j] * (-oas[i] * times[j]).exp();
        }
        sum / paths as f64
    };
    if n < 256 || threads == 1 {
        for (i, value) in output.iter_mut().enumerate() {
            *value = calculate(i);
        }
    } else {
        pool(threads)?.install(|| {
            output
                .par_iter_mut()
                .enumerate()
                .for_each(|(i, v)| *v = calculate(i))
        });
    }
    if output.iter().any(|v| !v.is_finite()) {
        return Err(2);
    }
    Ok(())
}

#[no_mangle]
pub extern "C" fn portfolio_risk_abi_version() -> u32 {
    1
}

/// Status: 0 success, 1 invalid dimensions/inputs, 2 nonfinite price, 3 panic/pool error.
///
/// # Safety
/// Caller owns aligned, live, nonoverlapping buffers for the declared lengths:
/// offsets n+1; times/values m; oas/output n. Input storage must remain immutable
/// until return. Output must be exclusively writable. No pointer is retained.
#[no_mangle]
pub unsafe extern "C" fn portfolio_risk_price(
    n: usize,
    m: usize,
    offsets: *const i64,
    times: *const f64,
    values: *const f64,
    oas: *const f64,
    paths: usize,
    threads: usize,
    output: *mut f64,
) -> i32 {
    if n == 0 {
        return if m == 0 && paths > 0 && threads > 0 && threads <= 256 {
            0
        } else {
            1
        };
    }
    if n >= (isize::MAX as usize / 8)
        || m > (isize::MAX as usize / 8)
        || offsets.is_null()
        || times.is_null()
        || values.is_null()
        || oas.is_null()
        || output.is_null()
    {
        return 1;
    }
    // SAFETY: buffer lifetime/size/non-aliasing are the caller's ABI contract.
    // The Python adapter supplies checked immutable arrays and fresh output.
    std::panic::catch_unwind(|| {
        price(
            std::slice::from_raw_parts(offsets, n + 1),
            std::slice::from_raw_parts(times, m),
            std::slice::from_raw_parts(values, m),
            std::slice::from_raw_parts(oas, n),
            paths,
            threads,
            std::slice::from_raw_parts_mut(output, n),
        )
    })
    .map_or(3, |result| result.err().unwrap_or(0))
}

/// Run an owned lifecycle on the bounded shared compute pool.
pub fn with_compute_threads<T: Send>(
    threads: usize,
    work: impl FnOnce() -> Result<T, String> + Send,
) -> Result<T, String> {
    if !(1..=256).contains(&threads) {
        return Err("invalid compute thread budget".into());
    }
    let control = portfolio_compute_control::active();
    pool(threads)
        .map_err(|_| "native thread pool unavailable")?
        .install(|| portfolio_compute_control::scope(control, work))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_price_and_negative_spread() {
        let mut out = [0.; 2];
        price(
            &[0, 2, 3],
            &[1., 2., 3.],
            &[200., 100., 80.],
            &[0.01, -0.02],
            2,
            1,
            &mut out,
        )
        .unwrap();
        assert!(
            (out[0] - (200. * (-0.01_f64).exp() + 100. * (-0.02_f64).exp()) / 2.).abs() < 1e-12
        );
        assert!((out[1] - 40. * 0.06_f64.exp()).abs() < 1e-12);
    }
    #[test]
    fn invalid_and_overflow() {
        assert_eq!(price(&[0, 0], &[], &[], &[0.], 1, 1, &mut [0.]), Err(1));
        assert_eq!(
            price(&[0, 1], &[1.], &[1.], &[f64::NAN], 1, 1, &mut [0.]),
            Err(1)
        );
        assert_eq!(
            price(&[0, 1], &[1000.], &[1.], &[-10.], 1, 1, &mut [0.]),
            Err(2)
        );
    }
    #[test]
    fn thread_count_does_not_change_reduction_order() {
        let offsets: Vec<i64> = (0..=1000).map(|x| x * 3).collect();
        let times = vec![1.; 3000];
        let values = vec![1.; 3000];
        let oas = vec![0.02; 1000];
        let mut a = vec![0.; 1000];
        let mut b = vec![0.; 1000];
        price(&offsets, &times, &values, &oas, 33, 1, &mut a).unwrap();
        price(&offsets, &times, &values, &oas, 33, 4, &mut b).unwrap();
        assert_eq!(a, b);
    }
}
