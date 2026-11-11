use super::*;

#[test]
fn actual_columns_must_fit_the_admitted_bound_before_matrix_allocation() {
    let points: Vec<_> = (0..8)
        .map(|i| Point {
            features: vec![i as f64],
            state: CreditState::Performing,
            principal: 100.,
            legal: 0.,
            surviving_rights: true,
            fee_payoffs: None,
        })
        .collect();
    let rows: Vec<_> = points.iter().collect();
    assert!(fit_admitted(0, &rows, &[100.; 8], 1, 1)
        .err()
        .unwrap()
        .contains("before matrix allocation"));
}

fn point(features: Vec<f64>) -> Point {
    Point {
        features,
        state: CreditState::Performing,
        principal: 100.,
        legal: 0.,
        surviving_rights: true,
        fee_payoffs: None,
    }
}

#[test]
fn exact_constants_have_exact_centers_and_inactive_fallback_at_survivor_counts() {
    let constants = vec![1., 0.1, -0.1, 137.125, 1e12, 1e-200, -1e-200, 0.];
    for count in [115, 238] {
        let points: Vec<_> = (0..count).map(|_| point(constants.clone())).collect();
        let rows: Vec<_> = points.iter().collect();
        let result = fit(0, &rows, &vec![91.; count], 2).unwrap();
        assert_eq!(result.means, constants);
        assert_eq!(result.scales, vec![1.; constants.len()]);
        assert!(result.active_features.is_empty());
        assert_eq!(result.rank, 1);
        assert_eq!(result.coefficients.len(), 1);
        assert_eq!(result.observations, count);
        assert!((forecast(&result, &points[0]).unwrap() - 91.).abs() < 1e-9);
    }
}

#[test]
fn genuinely_varying_near_constants_retain_ordered_mean_variance_and_threshold() {
    for count in [115, 238] {
        let points: Vec<_> = (0..count)
            .map(|i| {
                let direction = if i % 2 == 0 { -1. } else { 1. };
                point(vec![1. + direction * 1e-12, direction * 1e-16])
            })
            .collect();
        let rows: Vec<_> = points.iter().collect();
        let result = fit(0, &rows, &vec![91.; count], 1).unwrap();
        for column in 0..2 {
            let mut mean = 0.;
            for row in &points {
                mean += row.features[column] / count as f64;
            }
            let mut variance = 0.;
            for row in &points {
                variance += (row.features[column] - mean).powi(2) / count as f64;
            }
            let scale = variance.sqrt();
            let expected = if scale <= 8. * f64::EPSILON * mean.abs().max(1.) {
                1.
            } else {
                scale
            };
            assert_eq!(result.means[column].to_bits(), mean.to_bits());
            assert_eq!(result.scales[column].to_bits(), expected.to_bits());
        }
        assert_eq!(result.active_features, vec![0]);
    }
}

#[test]
fn exact_center_does_not_allow_unobserved_evaluation_feature_or_credit_state() {
    let points: Vec<_> = (0..115).map(|_| point(vec![1.])).collect();
    let rows: Vec<_> = points.iter().collect();
    let result = fit(0, &rows, &vec![91.; 115], 2).unwrap();
    assert!(forecast(&result, &point(vec![1.01]))
        .unwrap_err()
        .contains("lacks varying training coverage"));
    let mut unobserved = point(vec![1.]);
    unobserved.state = CreditState::Watch;
    assert!(forecast(&result, &unobserved)
        .unwrap_err()
        .contains("lacks training coverage"));
}
