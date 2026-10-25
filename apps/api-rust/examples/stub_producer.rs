//! Test producer for the service's process tests. It replays the frozen, independently verified
//! stage C stream for exactly the frozen request, so fencing tests do not need a HiGHS build.
//! `STUB_SLEEP_MS` delays the output; `STUB_TAMPER=1` changes one posting (the verifier must
//! refuse it). The live `portfolio-model-ledger` path has its own end-to-end test.
use std::io::{Read, Write};

const REQUEST: &str = include_str!(
    "../../../packages/portfolio-verification-native/tests/fixtures/lifecycle-unwind.request.json"
);
const STREAM: &str = include_str!(
    "../../../packages/portfolio-verification-native/tests/fixtures/lifecycle-unwind.stream.jsonl"
);

fn main() {
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).expect("stdin");
    let envelope: serde_json::Value = serde_json::from_slice(&input).expect("envelope");
    let expected: serde_json::Value = serde_json::from_str(REQUEST).expect("fixture");
    // The same envelope contract as `portfolio-model-ledger`: exactly a bounded timeout and the request.
    let timeout = envelope
        .get("execution_timeout_ms")
        .and_then(serde_json::Value::as_u64);
    if envelope.as_object().map(|o| o.len()) != Some(2)
        || !timeout.is_some_and(|t| (1..=3_600_000).contains(&t))
    {
        eprintln!("explicit execution timeout required");
        std::process::exit(2);
    }
    if envelope.get("request") != Some(&expected) {
        eprintln!("stub producer replays only the frozen lifecycle fixture");
        std::process::exit(3);
    }
    if let Ok(ms) = std::env::var("STUB_SLEEP_MS") {
        std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap_or(0)));
    }
    let stream = if std::env::var("STUB_TAMPER").as_deref() == Ok("1") {
        STREAM.replacen("\"debit_amount\":95.0", "\"debit_amount\":94.0", 1)
    } else {
        STREAM.to_string()
    };
    std::io::stdout()
        .write_all(stream.as_bytes())
        .expect("stdout");
}
