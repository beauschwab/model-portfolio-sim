//! Explicit modeled-source coordinators; financial preparation and replay stay native.
use serde_json::{json, Value};
use std::io::{self, Read, Write};
fn main() {
    let result = (|| -> Result<(), String> {
        let dispatcher_started = std::time::Instant::now();
        let mut input = Vec::new();
        // The explicitly selected saved derivative/hedge acceptance tier admits larger
        // envelopes; every other route keeps the 32 MiB input bound.
        let tier_bound = portfolio_decision_native::saved_derivative_mixed::stream::MAX_TIER_BYTES;
        io::stdin()
            .take(tier_bound as u64 + 1)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > tier_bound {
            return Err("model ledger input exceeds 32 MiB".into());
        }
        let value: Value = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
        if input.len() > 32 * 1024 * 1024
            && !(value
                .get("request")
                .and_then(|r| r.get("schema"))
                .and_then(Value::as_str)
                .is_some_and(|s| s == "saved-derivative-mixed-1" || s == "saved-hedge-mixed-1")
                && value
                    .get("request")
                    .and_then(|r| r.get("scale_tier"))
                    .is_some())
        {
            return Err("model ledger input exceeds 32 MiB".into());
        }
        if value
            .get("request")
            .and_then(|r| r.get("schema"))
            .and_then(Value::as_str)
            == Some("commercial-credit-ledger-1")
        {
            return portfolio_decision_native::commercial_ledger::run_started(
                &input,
                &mut io::stdout(),
                dispatcher_started,
            );
        }
        if value
            .get("request")
            .and_then(|r| r.get("schema"))
            .and_then(Value::as_str)
            == Some("saved-derivative-deposit-portfolio-mixed-1")
        {
            return portfolio_decision_native::joint_deposit_derivative::run_started(
                &input,
                &mut io::stdout(),
                dispatcher_started,
            );
        }
        if value
            .get("request")
            .and_then(|r| r.get("schema"))
            .and_then(Value::as_str)
            == Some("deposit-performing-portfolio-ledger-2")
        {
            use portfolio_ledger_native::deposit_source_portfolio;
            let (request, timeout, raw) = deposit_source_portfolio::parse_envelope(&input)?;
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(timeout)?),
                || {
                    let prepared = deposit_source_portfolio::prepare(request)?;
                    deposit_source_portfolio::run_public(&raw, &prepared, Box::new(io::stdout()))
                },
            );
        }
        if value
            .get("request")
            .and_then(|r| r.get("schema"))
            .and_then(Value::as_str)
            .is_some_and(|s| s == "saved-derivative-mixed-1" || s == "saved-hedge-mixed-1")
        {
            return portfolio_decision_native::saved_derivative_mixed::public::run_started(
                &input,
                &mut io::stdout(),
                dispatcher_started,
            );
        }
        if value
            .get("request")
            .and_then(|r| r.get("schema"))
            .and_then(Value::as_str)
            == Some("deposit-performing-income-ledger-1")
        {
            use portfolio_ledger_native::deposit_source_principal;
            let (request, timeout, raw) = deposit_source_principal::parse_envelope(&input)?;
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(timeout)?),
                || {
                    let prepared = deposit_source_principal::prepare(request)?;
                    deposit_source_principal::run_public(prepared, raw, Box::new(io::stdout()))
                },
            );
        }
        if value
            .get("request")
            .and_then(|r| r.get("schema"))
            .and_then(Value::as_str)
            == Some("deposit-revised-income-ledger-1")
        {
            use portfolio_ledger_native::deposit_revision;
            let (request, timeout, raw) = deposit_revision::parse_envelope(&input)?;
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(timeout)?),
                || {
                    let prepared = deposit_revision::prepare(request)?;
                    deposit_revision::run_public(prepared, raw, Box::new(io::stdout()))
                },
            );
        }
        if value
            .get("request")
            .and_then(|r| r.get("schema"))
            .and_then(Value::as_str)
            == Some("mortgage-mixed-accounting-1")
        {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Envelope {
                request: portfolio_ledger_native::mortgage_pending_owner::Request,
                execution_timeout_ms: u64,
            }
            let envelope: Envelope = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
            if input.len() > 1024 * 1024 {
                return Err("mortgage raw envelope1MiB bound".into());
            }
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(
                    envelope.execution_timeout_ms,
                )?),
                || {
                    let prepared =
                        portfolio_ledger_native::mortgage_pending_owner::prepare(envelope.request)?;
                    portfolio_ledger_native::mortgage_pending_owner::run(
                        &prepared,
                        Box::new(io::stdout()),
                    )
                },
            );
        }
        drop(input);
        let timeout = value
            .get("execution_timeout_ms")
            .and_then(Value::as_u64)
            .ok_or("explicit execution timeout required")?;
        let request = value.get("request").ok_or("raw source request required")?;
        if value
            .as_object()
            .ok_or("invalid model ledger envelope")?
            .len()
            != 2
        {
            return Err("unknown model ledger envelope fields".into());
        }
        portfolio_compute_control::scope(
            Some(portfolio_compute_control::Control::new(timeout)?),
            || match request.get("schema").and_then(Value::as_str) {
                Some("graph-dated-ledger-1") => {
                    use portfolio_decision_native::advanced_ledger;
                    use portfolio_ledger_native::{daily, dated_ledger::CompiledDatedLedger};
                    let raw = serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
                    let prepared = advanced_ledger::prepare(&raw)?;
                    let mut output = io::stdout();
                    serde_json::to_writer(&mut output,&json!({"workflow":{
                        "schema":"model-ledger-preparation-1","model_schema":"graph-dated-ledger-1",
                        "prepared_request":&prepared.dated_request,"source_receipt":&prepared.receipt}})).map_err(|e|e.to_string())?;
                    output
                        .write_all(b"\n")
                        .and_then(|_| output.flush())
                        .map_err(|e| e.to_string())?;
                    let compiled =
                        CompiledDatedLedger::compile(prepared.dated_request, 60000, 90000000)?;
                    daily::run_dated(compiled, Box::new(output))
                }
                Some("deposit-graph-dated-ledger-1") => {
                    use portfolio_decision_native::deposit_ledger;
                    use portfolio_ledger_native::{daily, dated_ledger::CompiledDatedLedger};
                    let raw = serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
                    let prepared = deposit_ledger::prepare(&raw)?;
                    let mut output = io::stdout();
                    serde_json::to_writer(&mut output,&json!({"workflow":{
                        "schema":"model-ledger-preparation-1","model_schema":"deposit-graph-dated-ledger-1",
                        "prepared_request":&prepared.dated_request,"source_receipt":&prepared.receipt}})).map_err(|e|e.to_string())?;
                    output
                        .write_all(b"\n")
                        .and_then(|_| output.flush())
                        .map_err(|e| e.to_string())?;
                    let compiled =
                        CompiledDatedLedger::compile(prepared.dated_request, 60000, 90000000)?;
                    daily::run_dated(compiled, Box::new(output))
                }
                Some("deposit-graph-income-ledger-1") => {
                    use portfolio_decision_native::deposit_ledger::income;
                    use portfolio_ledger_native::{daily, dated_ledger::CompiledDatedLedger};
                    let raw = serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
                    let prepared = income::prepare(&raw)?;
                    let mut output = io::stdout();
                    serde_json::to_writer(&mut output,&json!({"workflow":{
                        "schema":"model-ledger-preparation-1","model_schema":"deposit-graph-income-ledger-1",
                        "prepared_request":&prepared.dated_request,"source_receipt":&prepared.receipt}})).map_err(|e|e.to_string())?;
                    output
                        .write_all(b"\n")
                        .and_then(|_| output.flush())
                        .map_err(|e| e.to_string())?;
                    let compiled =
                        CompiledDatedLedger::compile(prepared.dated_request, 60000, 90000000)?;
                    daily::run_dated(compiled, Box::new(output))
                }
                Some("liquidity-allocation-1") => {
                    use portfolio_decision_native::workflows::lifecycle::allocation;
                    let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
                    allocation::run(&bytes, Box::new(io::stdout())).map(|_| ())
                }
                Some("liquidity-lifecycle-1") => {
                    use portfolio_decision_native::workflows::lifecycle::acquisition;
                    let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
                    acquisition::run(&bytes, Box::new(io::stdout())).map(|_| ())
                }
                Some("driven-credit-ledger-1") => {
                    use portfolio_ledger_native::{credit_path_replay, driven_credit};
                    let raw = serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
                    let prepared = driven_credit::prepare(&raw)?;
                    let mut output = io::stdout();
                    serde_json::to_writer(&mut output,&json!({"workflow":{
                        "schema":"model-ledger-preparation-1","model_schema":"driven-credit-ledger-1",
                        "prepared_request":&prepared.ledger,"source_receipt":&prepared.receipt}})).map_err(|e|e.to_string())?;
                    output
                        .write_all(b"\n")
                        .and_then(|_| output.flush())
                        .map_err(|e| e.to_string())?;
                    let compiled =
                        credit_path_replay::CompiledPathLedger::compile(prepared.ledger)?;
                    credit_path_replay::run(compiled, output)
                }
                _ => Err("unsupported modeled source ledger schema".into()),
            },
        )
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
