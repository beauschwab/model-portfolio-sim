//! One bounded JSON request on stdin, one JSON response on stdout. This executable
//! exercises native strategy ownership without a Python interpreter or callback.
use std::io::{self, Read};

fn main() {
    let started = std::time::Instant::now();
    let mut ftp_handled = false;
    let result = (|| -> Result<serde_json::Value, String> {
        let mut input = Vec::new();
        io::stdin()
            .take(128 * 1024 * 1024 + 1)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > 128 * 1024 * 1024 {
            return Err("request exceeds 128 MiB".into());
        }
        let request = portfolio_decision_native::commercial_intake::parse(&input)?;
        if request["op"] == "analyze_commercial_fee_journal" {
            return portfolio_decision_native::commercial_fee_journal::run_started(&input, started);
        }
        if request["op"] == "analyze_commercial_holding_journal" {
            portfolio_decision_native::commercial_holding_journal::run_cli_started(
                &input,
                io::stdout(),
                started,
            )?;
            ftp_handled = true;
            return Ok(serde_json::Value::Null);
        }
        if request["op"] == "analyze_commercial_candidate_journal" {
            return portfolio_decision_native::commercial_candidate_journal::run_envelope_started(
                &input, started,
            );
        }
        if request["op"] == "analyze_commercial_treasury" {
            return portfolio_decision_native::commercial_treasury::run_envelope(&input);
        }
        if request["op"] == "prepare_modeled_product_ftp" {
            // Historical generic peek admits128MiB; selected raw parser independently
            // enforces1MiB. This is not a preparse1MiB RSS guarantee.
            portfolio_decision_native::modeled_product_ftp::public_cli::execute(
                &input,
                io::stdout(),
            )?;
            ftp_handled = true;
            return Ok(serde_json::Value::Null);
        }
        if request["op"] == "analyze_mortgage_closing_capital" {
            return portfolio_decision_native::mortgage_closing_capital::run_envelope(&input);
        }
        drop(input);
        if request["op"] == "analyze_product_mixed_product_credit" {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ProductCreditEnvelope {
                op: String,
                schema: String,
                execution_timeout_ms: u64,
                request: portfolio_decision_native::product_mixed_product_credit::Request,
            }
            let raw: ProductCreditEnvelope =
                serde_json::from_value(request).map_err(|e| e.to_string())?;
            if raw.op != "analyze_product_mixed_product_credit"
                || raw.schema != "product-mixed-product-credit-1"
            {
                return Err("unsupported product-credit mixed operation".into());
            }
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(
                    raw.execution_timeout_ms,
                )?),
                || {
                    let result = portfolio_decision_native::product_mixed_product_credit::evaluate(
                        &raw.request,
                    )?;
                    portfolio_compute_control::checkpoint()?;
                    serde_json::to_value(result).map_err(|e| e.to_string())
                },
            );
        }
        if request["op"] == "analyze_product_mixed_consumer_credit" {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Envelope {
                op: String,
                schema: String,
                execution_timeout_ms: u64,
                request: portfolio_decision_native::product_mixed_consumer_credit::Request,
            }
            let raw: Envelope = serde_json::from_value(request).map_err(|e| e.to_string())?;
            if raw.op != "analyze_product_mixed_consumer_credit"
                || raw.schema != "product-mixed-consumer-credit-1"
            {
                return Err("unsupported consumer mixed operation".into());
            }
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(
                    raw.execution_timeout_ms,
                )?),
                || {
                    let result =
                        portfolio_decision_native::product_mixed_consumer_credit::evaluate(
                            &raw.request,
                        )?;
                    portfolio_compute_control::checkpoint()?;
                    serde_json::to_value(result).map_err(|e| e.to_string())
                },
            );
        }
        if request["op"] == "analyze_product_mixed_dated" {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct MixedEnvelope {
                op: String,
                schema: String,
                execution_timeout_ms: u64,
                request: portfolio_decision_native::product_mixed_dated::Request,
            }
            let raw: MixedEnvelope = serde_json::from_value(request).map_err(|e| e.to_string())?;
            if raw.op != "analyze_product_mixed_dated" || raw.schema != "product-mixed-dated-1" {
                return Err("unsupported mixed operation".into());
            }
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(
                    raw.execution_timeout_ms,
                )?),
                || {
                    let result =
                        portfolio_decision_native::product_mixed_dated::evaluate(&raw.request)?;
                    portfolio_compute_control::checkpoint()?;
                    serde_json::to_value(result).map_err(|e| e.to_string())
                },
            );
        }
        if request["op"] == "analyze_product_candidates" {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct CandidateEnvelope {
                op: String,
                schema: String,
                execution_timeout_ms: u64,
                request: portfolio_decision_native::product_candidate_journal::Request,
            }
            let raw: CandidateEnvelope =
                serde_json::from_value(request).map_err(|e| e.to_string())?;
            if raw.op != "analyze_product_candidates" || raw.schema != "product-candidate-journal-1"
            {
                return Err("unsupported candidate operation".into());
            }
            return portfolio_compute_control::scope(
                Some(portfolio_compute_control::Control::new(
                    raw.execution_timeout_ms,
                )?),
                || {
                    let result = portfolio_decision_native::product_candidate_journal::evaluate(
                        &raw.request,
                    )?;
                    serde_json::to_value(result).map_err(|e| e.to_string())
                },
            );
        }
        if ![
            "optimize_library",
            "optimize_aged",
            "optimize_product",
            "optimize_commercial",
            "run_owned",
        ]
        .contains(&request["op"].as_str().unwrap_or(""))
        {
            return Err(
                "standalone strategy requires optimize_library, optimize_aged, optimize_product, optimize_commercial or run_owned".into(),
            );
        }
        portfolio_decision_native::dispatch(request)
    })();
    if ftp_handled {
        return;
    }
    match result {
        Ok(v) => println!("{}", serde_json::json!({"ok":v})),
        Err(e) => {
            println!("{}", serde_json::json!({"error":e}));
            std::process::exit(2);
        }
    }
}
