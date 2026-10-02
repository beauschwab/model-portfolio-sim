//! Raw corporate/CD schedules and risk without a Python runtime.
use portfolio_risk_native::term_protocol::{respond, MAX_INPUT};
use std::io::{self, Read, Write};
fn main() {
    let mut body = Vec::new();
    if io::stdin()
        .take((MAX_INPUT + 1) as u64)
        .read_to_end(&mut body)
        .is_err()
    {
        std::process::exit(2);
    }
    let (output, ok) = respond(&body);
    if io::stdout().lock().write_all(&output).is_err() {
        std::process::exit(2);
    }
    std::process::exit(if ok { 0 } else { 1 });
}
