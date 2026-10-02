//! Native raw-input workflow. Stdout is a bounded-batch ledger protocol stream.
use std::io::{self, Read};
fn main() {
    let result = (|| -> Result<(), String> {
        let mut input = Vec::new();
        io::stdin()
            .take(128 * 1024 * 1024 + 1)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > 128 * 1024 * 1024 {
            return Err("request exceeds 128 MiB".into());
        }
        let request = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
        drop(input);
        portfolio_decision_native::run_workflow(request, Box::new(io::stdout()))
    })();
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(2);
    }
}
