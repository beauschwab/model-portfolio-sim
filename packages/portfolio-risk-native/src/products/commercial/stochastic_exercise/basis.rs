//! Explicit continuation-basis refinements. The historical quadratic basis keeps
//! its exact term and reduction order; no evaluation sample selects a degree.
pub(super) fn degree(policy: &str) -> Result<usize, String> {
    match policy {
        "native_observable_linear_svd_train_evaluate-1" => Ok(1),
        "native_observable_quadratic_svd_train_evaluate-1" => Ok(2),
        "native_observable_cubic_svd_train_evaluate-1" => Ok(3),
        _ => Err("explicit supported observable continuation basis required".into()),
    }
}

pub(super) fn quadratic(degree: &usize) -> bool {
    *degree == 2
}

pub(super) fn columns(features: usize, degree: usize) -> usize {
    // Admission bounds features to 64 and degree to 1..=3 before this helper.
    let mut columns = 1 + features;
    if degree >= 2 {
        columns += features * (features + 1) / 2;
    }
    if degree == 3 {
        columns += features * (features + 1) * (features + 2) / 6;
    }
    columns
}

pub(super) fn values(
    features: &[f64],
    means: &[f64],
    scales: &[f64],
    active: &[usize],
    degree: usize,
) -> Vec<f64> {
    let x: Vec<_> = active
        .iter()
        .map(|&i| (features[i] - means[i]) / scales[i])
        .collect();
    let mut values = Vec::with_capacity(columns(x.len(), degree));
    values.push(1.);
    values.extend_from_slice(&x);
    if degree >= 2 {
        for i in 0..x.len() {
            for j in i..x.len() {
                values.push(x[i] * x[j]);
            }
        }
    }
    if degree == 3 {
        for i in 0..x.len() {
            for j in i..x.len() {
                for k in j..x.len() {
                    values.push(x[i] * x[j] * x[k]);
                }
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_refinements_retain_historical_quadratic_order() {
        let input = [2., 3.];
        let linear = values(&input, &[0.; 2], &[1.; 2], &[0, 1], 1);
        let quadratic = values(&input, &[0.; 2], &[1.; 2], &[0, 1], 2);
        let cubic = values(&input, &[0.; 2], &[1.; 2], &[0, 1], 3);
        assert_eq!(linear, [1., 2., 3.]);
        assert_eq!(quadratic, [1., 2., 3., 4., 6., 9.]);
        assert_eq!(cubic, [1., 2., 3., 4., 6., 9., 8., 12., 18., 27.]);
        for (degree, vector) in [(1, linear), (2, quadratic), (3, cubic)] {
            assert_eq!(vector.len(), columns(2, degree));
            assert_eq!(values(&[], &[], &[], &[], degree), [1.]);
        }
    }

    #[test]
    fn supported_degrees_and_capacity_are_explicit() {
        assert_eq!(
            degree("native_observable_cubic_svd_train_evaluate-1").unwrap(),
            3
        );
        assert!(degree("automatic_best_evaluation_fit").is_err());
        assert_eq!(columns(64, 1), 65);
        assert_eq!(columns(64, 2), 2145);
        assert_eq!(columns(64, 3), 47905);
    }
}
