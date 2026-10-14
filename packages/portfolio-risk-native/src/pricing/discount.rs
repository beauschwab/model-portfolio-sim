use crate::math::arrays::View;
use rayon::prelude::*;

pub(crate) fn oas(a: &[View<'_>], solve: bool) -> Vec<Vec<f64>> {
    // CSR offsets, exact times, discounted sums, target/spread, paths,
    // tolerance, max_iter, lower, upper.
    let n = a[0].v.len() - 1;
    assert_eq!(a[1].v.len(), a[2].v.len());
    assert_eq!(a[0].ix(0), 0);
    assert_eq!(a[0].ix(n), a[1].v.len());
    assert!(a[4].at(0) > 0.0);
    // Flat result buffers avoid two/three heap allocations per contract on
    // every fixed-OAS bump. Each row retains the exact sequential reduction.
    let calculate = |s| {
        let (start, end) = (a[0].ix(s), a[0].ix(s + 1));
        assert!(end > start);
        let price = |o: f64| {
            let (mut px, mut deriv) = (0.0, 0.0);
            for j in start..end {
                let e = a[2].at(j) * (-o * a[1].at(j)).exp();
                px += e;
                deriv -= e * a[1].at(j);
            }
            (px / a[4].at(0), deriv / a[4].at(0))
        };
        if !solve {
            return [price(a[3].at(s)).0, 0.];
        }
        let (mut lo, mut hi, mut o) = (a[7].at(0), a[8].at(0), 0.0);
        for _ in 0..a[6].ix(0) {
            let (px, dpx) = price(o);
            let err = px - a[3].at(s);
            if err.abs() < a[5].at(0) {
                break;
            }
            if err > 0.0 {
                lo = lo.max(o);
            }
            if err < 0.0 {
                hi = hi.min(o);
            }
            let cand = o + if dpx.abs() > 1e-12 { -err / dpx } else { 0.0 };
            o = if cand <= lo || cand >= hi || !cand.is_finite() {
                (lo + hi) * 0.5
            } else {
                cand
            };
        }
        let px = price(o).0;
        assert!(
            px.is_finite() && (px - a[3].at(s)).abs() <= a[5].at(0),
            "OAS did not converge"
        );
        [o, px]
    };
    let mut output = vec![vec![0.; n]; if solve { 2 } else { 1 }];
    if solve {
        let (spread, prices) = output.split_at_mut(1);
        spread[0]
            .par_iter_mut()
            .zip(prices[0].par_iter_mut())
            .enumerate()
            .for_each(|(s, (o, p))| {
                let values = calculate(s);
                *o = values[0];
                *p = values[1];
            });
    } else {
        output[0]
            .par_iter_mut()
            .enumerate()
            .for_each(|(s, p)| *p = calculate(s)[0]);
    }
    output
}

pub(crate) fn price(
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
        crate::runtime::compute::pool(threads)?.install(|| {
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
