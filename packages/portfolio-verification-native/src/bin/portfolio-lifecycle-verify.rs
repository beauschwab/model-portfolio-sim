//! `portfolio-lifecycle-verify <request.json> <stream.jsonl>`: prints the verification receipt
//! as JSON and exits 0, or prints the refusal to stderr and exits 1. No Python, no network.
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: portfolio-lifecycle-verify <request.json> <stream.jsonl>");
        process::exit(2);
    }
    let read = |path: &str, limit: usize| -> Result<Vec<u8>, String> {
        let size = fs::metadata(path)
            .map_err(|e| format!("{path}: {e}"))?
            .len();
        if size > limit as u64 {
            return Err(format!("{path} exceeds the verifier's byte admission"));
        }
        fs::read(path).map_err(|e| format!("{path}: {e}"))
    };
    let result = read(&args[1], portfolio_verification_native::MAX_REQUEST_BYTES)
        .and_then(|request| {
            read(&args[2], portfolio_verification_native::MAX_STREAM_BYTES)
                .map(|stream| (request, stream))
        })
        .and_then(|(request, stream)| portfolio_verification_native::verify(&request, &stream));
    match result {
        Ok(receipt) => println!(
            "{}",
            serde_json::to_string(&receipt).expect("receipt serializes")
        ),
        Err(error) => {
            eprintln!("{error}");
            process::exit(1);
        }
    }
}
