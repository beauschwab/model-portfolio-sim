//! Component fixture producer for independent-verifier tests, not an application entrypoint.
use std::io::{Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = Vec::new();
    std::io::stdin().take((4 << 20) + 1).read_to_end(&mut raw)?;
    if raw.len() > 4 << 20 {
        return Err("macro fixture input bound".into());
    }
    let request: portfolio_model_core::consumer_macro::Request = serde_json::from_slice(&raw)?;
    let result = portfolio_model_core::consumer_macro::evaluate(&request, &mut || Ok(()))?;
    serde_json::to_writer(std::io::stdout().lock(), &result)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}
