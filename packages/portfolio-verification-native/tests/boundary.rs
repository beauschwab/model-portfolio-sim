//! The verifier's independence is a Cargo and source boundary (issue #46): it may use transport
//! and identity schemas it defines itself, never production financial crates.
use std::fs;
use std::path::Path;

#[test]
fn cargo_dependencies_exclude_every_production_financial_crate() {
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("[profile")
        .next()
        .unwrap();
    for line in deps
        .lines()
        .filter(|l| l.contains('=') && !l.trim_start().starts_with('#'))
    {
        let name = line.split('=').next().unwrap().trim();
        assert!(
            ["serde", "serde_json", "sha2"].contains(&name),
            "unexpected dependency `{name}`"
        );
    }
    assert!(
        !deps.contains("portfolio-"),
        "a portfolio crate dependency would make the verifier its own oracle"
    );
}

#[test]
fn sources_reference_no_production_crate() {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut files,
    );
    assert!(!files.is_empty());
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for forbidden in [
            "portfolio_risk_native",
            "portfolio_decision_native",
            "portfolio_ledger_native",
            "portfolio_model_core",
            "portfolio_compute_control",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} references {forbidden}",
                file.display()
            );
        }
    }
}
