//! One bounded JSON request on stdin, one JSON response on stdout. This executable
//! exercises native strategy ownership without a Python interpreter or callback.
use std::io::{self, Read};

fn main() {
    let result = (|| -> Result<serde_json::Value, String> {
        let mut input = Vec::new();
        io::stdin()
            .take(128 * 1024 * 1024 + 1)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > 128 * 1024 * 1024 {
            return Err("request exceeds 128 MiB".into());
        }
        let request: serde_json::Value =
            serde_json::from_slice(&input).map_err(|e| e.to_string())?;
        drop(input);
        if request["op"] != "optimize_library" && request["op"] != "run_owned" {
            return Err("standalone strategy requires optimize_library or run_owned".into());
        }
        portfolio_decision_native::dispatch(request)
    })();
    match result {
        Ok(v) => println!("{}", serde_json::json!({"ok":v})),
        Err(e) => {
            println!("{}", serde_json::json!({"error":e}));
            std::process::exit(2);
        }
    }
}
