//! Bounded raw/normalized specification input, streamed native daily output.
use std::io::{self, Read};
fn main() {
    let result = (|| -> Result<(), String> {
        let mut input = Vec::new();
        io::stdin()
            .take(100_000_001)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > 100_000_000 {
            return Err("input budget exceeded".into());
        }
        let value: serde_json::Value = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
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
