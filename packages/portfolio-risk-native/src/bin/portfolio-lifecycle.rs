//! Raw-input numerical lifecycle entrypoint. No Python interpreter or callbacks.
use portfolio_risk_native::term_protocol::{respond_started, MAX_INPUT};
use std::io::{self, Read, Write};
fn main() {
    let started = std::time::Instant::now();
    let mut bytes = Vec::new();
    if io::stdin()
        .take((MAX_INPUT + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        std::process::exit(2);
    }
    let (output, ok) = respond_started(&bytes, started);
    #[cfg(feature = "compute-profile")]
    eprintln!("{}", portfolio_risk_native::profile::report());
    if io::stdout().lock().write_all(&output).is_err() {
        std::process::exit(2);
    }
    std::process::exit(if ok { 0 } else { 1 });
}
