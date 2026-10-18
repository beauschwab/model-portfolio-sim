//! Bounded raw/normalized specification input, streamed native daily output.
use std::io::{self, Read};
fn main() {
    let result = (|| -> Result<(), String> {
        let mut input = Vec::new();
        // Only an explicitly tiered credit ledger request may exceed the historical 100 MB.
        io::stdin()
            .take(512_000_001)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > 512_000_000 {
            return Err("input budget exceeded".into());
        }
        let value: serde_json::Value = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
        if input.len() > 100_000_000
            && !(value.get("schema").and_then(serde_json::Value::as_str)
                == Some(portfolio_ledger_native::credit_replay::SCHEMA)
                && value.get("scale_tier").and_then(serde_json::Value::as_str)
                    == Some(portfolio_ledger_native::credit_replay::ACCEPTANCE_TIER))
        {
            return Err("input budget exceeded".into());
        }
        if value.get("schema").and_then(serde_json::Value::as_str)
            == Some(portfolio_ledger_native::credit_path_replay::SCHEMA)
        {
            let request = serde_json::from_value(value).map_err(|e| e.to_string())?;
            let prepared =
                portfolio_ledger_native::credit_path_replay::CompiledPathLedger::compile(request)?;
            return portfolio_ledger_native::credit_path_replay::run(prepared, io::stdout());
        }
        if value.get("schema").and_then(serde_json::Value::as_str)
            == Some(portfolio_ledger_native::credit_replay::SCHEMA)
        {
            let request = serde_json::from_value(value).map_err(|e| e.to_string())?;
            let prepared =
                portfolio_ledger_native::credit_replay::CompiledCreditReplay::compile(request)?;
            return portfolio_ledger_native::credit_replay::run(prepared, io::stdout());
        }
        if value.get("schema").and_then(serde_json::Value::as_str)
            == Some(portfolio_ledger_native::dated_ledger::SCHEMA)
        {
            let request = serde_json::from_value(value).map_err(|e| e.to_string())?;
            let prepared = portfolio_ledger_native::dated_ledger::CompiledDatedLedger::compile(
                request, 60000, 90_000_000,
            )?;
            return portfolio_ledger_native::daily::run_dated(prepared, Box::new(io::stdout()));
        }
        let spec = if value.get("version").is_some() {
            portfolio_ledger_native::spec::normalize(value, 60000, 90_000_000)?
        } else {
            serde_json::from_value(value).map_err(|e| e.to_string())?
        };
        portfolio_ledger_native::daily::run(spec, Box::new(io::stdout()))
    })();
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
