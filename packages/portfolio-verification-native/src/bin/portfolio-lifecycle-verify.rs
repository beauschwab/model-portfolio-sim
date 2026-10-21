//! `portfolio-lifecycle-verify <request.json> <stream.jsonl | ->`: prints the verification receipt
//! as JSON and exits 0, or prints the refusal to stderr and exits 1. No Python, no network.
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: portfolio-lifecycle-verify <request.json> <stream.jsonl | ->");
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
    // The stream file is read incrementally; its bound follows the request's tier.
    let result = read(
        &args[1],
        portfolio_verification_native::MAX_TIER_REQUEST_BYTES,
    )
    .and_then(|request| {
        // `-` reads the stream from standard input, so a producer can be piped in directly.
        if args[2] == "-" {
            return portfolio_verification_native::verify_reader(&request, std::io::stdin().lock());
        }
        let file = fs::File::open(&args[2]).map_err(|e| format!("{}: {e}", args[2]))?;
        portfolio_verification_native::verify_reader(&request, std::io::BufReader::new(file))
    });
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
