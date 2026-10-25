//! Architecture and contract checks, in Rust: generated HTTP/TypeScript contracts are current,
//! the service links no production financial crate and invokes no Python, and the shared schema
//! it creates matches the SQLAlchemy definitions it must coexist with.
use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(ROOT).join(rel))
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn generated_contracts_are_current() {
    let openapi =
        serde_json::to_string_pretty(&workbench_native::contract::openapi()).unwrap() + "\n";
    assert_eq!(
        read("contract/openapi.json"),
        openapi,
        "run `workbench-native contract openapi > contract/openapi.json`"
    );
    assert_eq!(
        read("../web/src/lib/native-api.generated.ts"),
        workbench_native::contract::typescript(),
        "run `workbench-native contract typescript > ../web/src/lib/native-api.generated.ts`"
    );
}

#[test]
fn the_service_links_no_financial_crate_and_no_python() {
    let manifest = read("Cargo.toml");
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap();
    for forbidden in [
        "portfolio-decision-native",
        "portfolio-ledger-native",
        "portfolio-risk-native",
        "portfolio-model-core",
        "pyo3",
        "numpy",
    ] {
        assert!(!deps.contains(forbidden), "service depends on {forbidden}");
    }
    assert!(
        deps.contains("portfolio-verification-native"),
        "independent verifier missing"
    );
    for entry in std::fs::read_dir(Path::new(ROOT).join("src")).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for pattern in [
            "Command::new(\"python",
            "Command::new(\"uv",
            "\"py\"",
            "pyo3",
            ".py\"",
        ] {
            assert!(
                !text.contains(pattern),
                "{} invokes Python: {pattern}",
                path.display()
            );
        }
    }
}

#[test]
fn shared_tables_keep_the_sqlalchemy_definitions() {
    // Every column the Python metadata declares, with its type spelling, appears in the Rust DDL.
    let python = read("../api/app/repository.py");
    let rust = read("src/migrate.rs");
    for (table, columns) in [
        (
            "workspaces",
            vec![
                "tenant VARCHAR(128)",
                "workspace VARCHAR(128)",
                "revision INTEGER",
                "owner VARCHAR(64)",
                "lease_until FLOAT",
            ],
        ),
        (
            "jobs",
            vec![
                "id VARCHAR(64)",
                "kind VARCHAR(64)",
                "status VARCHAR(20)",
                "request JSON",
                "result JSON",
                "progress JSON",
                "detail TEXT",
                "created_at FLOAT",
                "finished_at FLOAT",
                "owner VARCHAR(64)",
                "attempt INTEGER",
                "token VARCHAR(64)",
                "idempotency_key VARCHAR(128)",
            ],
        ),
        (
            "revisions",
            vec!["manifest JSON", "actor VARCHAR(200)", "reason VARCHAR(200)"],
        ),
    ] {
        assert!(
            python.contains(&format!("\"{table}\"")),
            "{table} is no longer a Python table"
        );
        for c in columns {
            assert!(
                rust.contains(c),
                "{table}.{c} differs from the SQLAlchemy definition"
            );
        }
    }
    assert!(
        python.contains("version=2"),
        "the shared schema version moved; update SHARED_SCHEMA_VERSION"
    );
    assert!(
        python.contains("jobs.c.status == \"queued\""),
        "legacy claim no longer selects only `queued`"
    );
}
