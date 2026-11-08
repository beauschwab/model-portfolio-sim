//! Test-only capture of the exact matrix immediately consumed by native SVD.
use super::*;
use nalgebra::DVector;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::io::Write;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static RECEIPTS: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

pub(super) struct Inputs<'a> {
    pub rows: &'a [&'a Point],
    pub means: &'a [f64],
    pub scales: &'a [f64],
    pub active_features: &'a [usize],
    pub targets: &'a [f64],
    pub matrix: &'a DMatrix<f64>,
}

fn rows(matrix: &DMatrix<f64>) -> Vec<Vec<f64>> {
    matrix
        .row_iter()
        .map(|r| r.iter().copied().collect())
        .collect()
}

pub(super) fn capture(input: Inputs<'_>) {
    if !ENABLED.get() {
        return;
    }
    let matrix = input.matrix;
    let decomposition = matrix.clone().svd(true, true);
    let u = decomposition.u.as_ref().unwrap();
    let vt = decomposition.v_t.as_ref().unwrap();
    let diagonal = DMatrix::from_diagonal(&decomposition.singular_values);
    let reconstructed = u * diagonal * vt;
    let cutoff =
        f64::EPSILON * matrix.nrows().max(matrix.ncols()) as f64 * decomposition.singular_values[0];
    let y = DVector::from_column_slice(input.targets);
    let coefficients = decomposition.solve(&y, cutoff).unwrap();
    let residual = matrix * &coefficients - &y;
    let normal_gradient = matrix.transpose() * &residual;
    let energy = matrix.norm_squared();
    let spectrum_energy = decomposition.singular_values.norm_squared();
    let mut receipt = json!({
        "feature_rows":input.rows.iter().map(|r|r.features.clone()).collect::<Vec<_>>(),
        "means":input.means,"scales":input.scales,"active_features":input.active_features,
        "matrix":rows(matrix),"targets":input.targets,
        "column_squared_norms":matrix.column_iter().map(|v|v.norm_squared()).collect::<Vec<_>>(),
        "column_sums":matrix.column_iter().map(|v|v.sum()).collect::<Vec<_>>(),
        "frobenius_squared":energy,"spectrum_squared_sum":spectrum_energy,
        "spectrum_energy_relative_error":(spectrum_energy-energy).abs()/energy,
        "singular_values":decomposition.singular_values.as_slice(),"cutoff":cutoff,
        "u":rows(u),"v_t":rows(vt),"u_sigma_vt":rows(&reconstructed),
        "reconstruction_relative_error":(&reconstructed-matrix).norm()/energy.sqrt(),
        "u_orthogonality_error":(u.transpose()*u-DMatrix::identity(u.ncols(),u.ncols())).norm(),
        "v_orthogonality_error":(vt*vt.transpose()-DMatrix::identity(vt.nrows(),vt.nrows())).norm(),
        "coefficients":coefficients.as_slice(),"solve_residual_norm":residual.norm(),
        "normal_gradient_norm":normal_gradient.norm(),
        "relative_normal_gradient":normal_gradient.norm()/(matrix.norm()*y.norm()).max(1.),
    });
    receipt["bounded_explicit_epsilon"] = if let Some(explicit) =
        nalgebra::linalg::SVD::try_new(matrix.clone(), true, true, f64::EPSILON, 100_000)
    {
        let eu = explicit.u.as_ref().unwrap();
        let ev = explicit.v_t.as_ref().unwrap();
        let rebuilt = eu * DMatrix::from_diagonal(&explicit.singular_values) * ev;
        let cutoff =
            f64::EPSILON * matrix.nrows().max(matrix.ncols()) as f64 * explicit.singular_values[0];
        let coefficients = explicit.solve(&y, cutoff).unwrap();
        let residual = matrix * &coefficients - &y;
        json!({
            "converged":true,"epsilon":f64::EPSILON,"maximum_iterations":100_000,
            "singular_values":explicit.singular_values.as_slice(),"cutoff":cutoff,
            "spectrum_energy_relative_error":(explicit.singular_values.norm_squared()-energy).abs()/energy,
            "reconstruction_relative_error":(&rebuilt-matrix).norm()/energy.sqrt(),
            "u_orthogonality_error":(eu.transpose()*eu-DMatrix::identity(eu.ncols(),eu.ncols())).norm(),
            "v_orthogonality_error":(ev*ev.transpose()-DMatrix::identity(ev.nrows(),ev.nrows())).norm(),
            "coefficients":coefficients.as_slice(),"solve_residual_norm":residual.norm(),
            "relative_normal_gradient":(matrix.transpose()*residual).norm()/(matrix.norm()*y.norm()).max(1.),
        })
    } else {
        json!({"converged":false})
    };
    RECEIPTS.with_borrow_mut(|receipts| receipts.push(receipt));
}

pub(super) fn start() {
    RECEIPTS.with_borrow_mut(Vec::clear);
    ENABLED.set(true);
}
pub(super) fn finish() -> Vec<Value> {
    ENABLED.set(false);
    RECEIPTS.with_borrow_mut(std::mem::take)
}
fn save(name: &str, receipt: &Value) {
    if let Some(directory) = std::env::var_os("PORTFOLIO_REGRESSION_AUDIT_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(format!("{name}.json")))
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(receipt).unwrap())
            .unwrap();
    }
}
fn value(receipt: &Value, name: &str) -> f64 {
    receipt[name].as_f64().unwrap()
}

#[test]
fn actual_matrix_matches_independent_rows_and_bad_legacy_factors_are_repaired() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/commercial-regression-svd-actual-rows.json"
    ))
    .unwrap();
    let mut violations = Vec::new();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let request: Request = serde_json::from_value(case["raw"].clone()).unwrap();
        start();
        let fitted = fit_training(&request).unwrap();
        let mut receipts = finish();
        assert_eq!(receipts.len(), 1);
        let mut receipt = receipts.pop().unwrap();
        let independent = case["independent_training_rows"].as_array().unwrap();
        let actual = receipt["feature_rows"].as_array().unwrap();
        assert_eq!(actual.len(), independent.len());
        for (i, (row, expected)) in actual.iter().zip(independent).enumerate() {
            for (a, b) in row
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["features"].as_array().unwrap())
            {
                assert!((a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-12);
            }
            assert!(
                (receipt["targets"][i].as_f64().unwrap() - expected["target"].as_f64().unwrap())
                    .abs()
                    < 1e-9
            );
        }
        for norm in receipt["column_squared_norms"].as_array().unwrap() {
            assert!((norm.as_f64().unwrap() - actual.len() as f64).abs() < 1e-9);
        }
        for sum in &receipt["column_sums"].as_array().unwrap()[1..] {
            assert!(sum.as_f64().unwrap().abs() < 1e-9);
        }
        let checked = checked_least_squares::solve(
            &DMatrix::from_row_slice(
                actual.len(),
                3,
                &receipt["matrix"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|r| r.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()))
                    .collect::<Vec<_>>(),
            ),
            &receipt["targets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>(),
            || Ok(()),
        )
        .unwrap();
        for (a, b) in fitted.regressions()[0]
            .coefficients
            .iter()
            .zip(&checked.coefficients)
        {
            assert_eq!(a.to_bits(), b.to_bits());
        }
        receipt["checked_regression"] = serde_json::to_value(&fitted.regressions()[0]).unwrap();
        receipt["case_id"] = case["id"].clone();
        receipt["independent_source_rows_and_targets_match"] = json!(true);
        receipt["independent_training_rows"] = case["independent_training_rows"].clone();
        save(&format!("exact-case-{index}"), &receipt);
        for metric in [
            "spectrum_energy_relative_error",
            "reconstruction_relative_error",
            "u_orthogonality_error",
            "v_orthogonality_error",
            "relative_normal_gradient",
        ] {
            if value(&receipt, metric) > 1e-9 {
                violations.push(format!("case{index} {metric}={}", value(&receipt, metric)));
            }
        }
    }
    assert_eq!(
        violations.len(),
        18,
        "all6 retained baseline cases must expose reconstruction, energy and solve defects: {}",
        violations.join("; ")
    );
}

#[test]
fn minimal_deterministic_legacy_decomposition_defect_is_repaired() {
    let mut found = None;
    'outer: for count in [6, 8, 12, 16, 32, 64] {
        for loading in [3., 7., 20., 100.] {
            let points: Vec<_> = (0..count)
                .map(|i| {
                    let x = ((i * 17 + 3) % 31) as f64 / 100.;
                    Point {
                        features: vec![x, loading * x],
                        state: CreditState::Performing,
                        principal: 100.,
                        legal: 0.,
                        surviving_rights: true,
                        fee_payoffs: None,
                    }
                })
                .collect();
            let refs: Vec<_> = points.iter().collect();
            let targets: Vec<_> = points.iter().map(|p| 100. + 10. * p.features[0]).collect();
            start();
            let fitted = fit(0, &refs, &targets, 1).unwrap();
            let mut receipt = finish().pop().unwrap();
            if value(&receipt, "reconstruction_relative_error") > 1e-9
                || value(&receipt, "relative_normal_gradient") > 1e-9
            {
                for (point, target) in points.iter().zip(&targets) {
                    let predicted = basis::values(
                        &point.features,
                        &fitted.means,
                        &fitted.scales,
                        &fitted.active_features,
                        1,
                    )
                    .iter()
                    .zip(&fitted.coefficients)
                    .map(|(x, b)| x * b)
                    .sum::<f64>();
                    assert!((predicted - target).abs() < 1e-9);
                }
                receipt["checked_regression"] = serde_json::to_value(&fitted).unwrap();
                found = Some(receipt);
                break 'outer;
            }
        }
    }
    if let Some(receipt) = found {
        save("minimal-repro", &receipt);
    } else {
        panic!("retained pinned nalgebra defect was not reproduced");
    }
}

#[test]
fn full_cubic_actual_matrix_preserves_factors_across_admission_refinement() {
    fn assert_numeric_bits_equal(actual: &Value, expected: &Value) {
        if let (Some(actual), Some(expected)) = (actual.as_array(), expected.as_array()) {
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.iter().zip(expected) {
                assert_numeric_bits_equal(actual, expected);
            }
        } else {
            assert_eq!(
                actual.as_f64().unwrap().to_bits(),
                expected.as_f64().unwrap().to_bits()
            );
        }
    }
    let original =
        include_bytes!("../../../../tests/fixtures/commercial-regression-cubic-compatibility.json");
    let mut fixture: Value = serde_json::from_slice(original).unwrap();
    if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        // Platform libm rounding can change low bits. Keep the original Windows
        // snapshot and a separately captured Linux snapshot, not a looser
        // comparison. Integer IEEE bits also avoid decimal-parser round trips.
        let platform: Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/commercial-regression-cubic-linux.json"
        ))
        .unwrap();
        assert_eq!(platform["schema"], "platform-f64-bits-1");
        assert_eq!(platform["target"], "x86_64-unknown-linux-gnu");
        let cases = fixture["cases"].as_array_mut().unwrap();
        let recorded = platform["cases"].as_array().unwrap();
        assert_eq!(cases.len(), recorded.len());
        for (case, recorded) in cases.iter_mut().zip(recorded) {
            assert_eq!(case["id"], recorded["id"]);
            assert_eq!(case["raw"], recorded["raw"]);
            for name in ["corrected_regressions", "legacy_regressions"] {
                let mut regressions = recorded[name].clone();
                for regression in regressions.as_array_mut().unwrap() {
                    for field in ["means", "scales", "coefficients", "singular_values"] {
                        for value in regression[field].as_array_mut().unwrap() {
                            let decoded = f64::from_bits(value.as_u64().unwrap());
                            assert!(decoded.is_finite());
                            *value = Value::from(decoded);
                        }
                    }
                }
                case[name] = regressions;
            }
        }
    }
    let mut failures = Vec::new();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let request: Request = serde_json::from_value(case["raw"].clone()).unwrap();
        start();
        let fitted = fit_training(&request).unwrap();
        let mut receipts = finish();
        assert_eq!(receipts.len(), request.policy.exercise_dates.len());
        receipts.reverse();
        for (date_index, mut receipt) in receipts.into_iter().enumerate() {
            let rows = receipt["feature_rows"].as_array().unwrap();
            let points: Vec<_> = rows
                .iter()
                .map(|row| Point {
                    features: row
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_f64().unwrap())
                        .collect(),
                    state: CreditState::Performing,
                    principal: 100.,
                    legal: 0.,
                    surviving_rights: true,
                    fee_payoffs: None,
                })
                .collect();
            let references: Vec<_> = points.iter().collect();
            let targets: Vec<_> = receipt["targets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect();
            // This test-only direct fit exercises the pre-refinement unrestricted
            // column admission; it cannot bypass production source/graph admission.
            start();
            let direct = fit(
                request.policy.exercise_dates[date_index].ordinal,
                &references,
                &targets,
                3,
            )
            .unwrap();
            let mut direct_receipts = finish();
            assert_eq!(direct_receipts.len(), 1);
            let direct_receipt = direct_receipts.pop().unwrap();
            assert_eq!(receipt["matrix"], direct_receipt["matrix"]);
            assert_eq!(receipt["targets"], direct_receipt["targets"]);
            assert_numeric_bits_equal(&receipt["matrix"], &direct_receipt["matrix"]);
            assert_numeric_bits_equal(&receipt["targets"], &direct_receipt["targets"]);
            let actual = serde_json::to_value(&fitted.regressions()[date_index]).unwrap();
            let unrestricted = serde_json::to_value(&direct).unwrap();
            for name in ["means", "scales", "coefficients", "singular_values"] {
                assert_numeric_bits_equal(&actual[name], &unrestricted[name]);
                assert_numeric_bits_equal(
                    &actual[name],
                    &case["corrected_regressions"][date_index][name],
                );
            }
            let columns = fitted.regressions()[date_index].coefficients.len();
            let matrix = DMatrix::from_row_slice(
                rows.len(),
                columns,
                &receipt["matrix"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|row| row.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()))
                    .collect::<Vec<_>>(),
            );
            let diagnostic = checked_least_squares::audit_computed_candidate(&matrix, &targets);
            let expected_repair =
                case["id"].as_str().unwrap().ends_with("correlated_ou") && date_index == 0;
            assert_eq!(diagnostic["bounded_candidate_converged"], json!(true));
            assert_eq!(diagnostic["checked_repaired"], json!(expected_repair));
            assert_eq!(
                diagnostic["bounded_candidate_check"]["passed"],
                json!(!expected_repair)
            );
            receipt["case_id"] = case["id"].clone();
            receipt["raw"] = case["raw"].clone();
            receipt["checked_candidate"] = diagnostic;
            receipt["actual_checked_regression"] = actual.clone();
            receipt["unrestricted_columns_regression"] = unrestricted.clone();
            receipt["retained_legacy_regression"] = case["legacy_regressions"][date_index].clone();
            receipt["retained_corrected_regression"] =
                case["corrected_regressions"][date_index].clone();
            save(&format!("cubic-case-{index}-date-{date_index}"), &receipt);
            if actual != case["corrected_regressions"][date_index] {
                failures.push(format!("case{index}: checked full regression changed"));
            }
            if actual != unrestricted {
                failures.push(format!(
                    "case{index}: column guard changed actual regression"
                ));
            }
            for name in ["coefficients", "singular_values"] {
                assert_numeric_bits_equal(
                    &receipt[name],
                    &case["legacy_regressions"][date_index][name],
                );
                if receipt[name] != case["legacy_regressions"][date_index][name] {
                    failures.push(format!(
                        "case{index}: original native {name} differs from retained legacy artifact"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
