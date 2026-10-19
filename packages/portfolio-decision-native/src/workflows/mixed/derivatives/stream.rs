//! Explicitly selected acceptance tier for the saved derivative and hedge mixed journals.
//!
//! The default tier keeps every historical bound and the buffered single-envelope output. The
//! `acceptance_60000` tier admits up to 60,000 dated positions and 90 million joined work
//! units (the dated ledger's own large tier), and streams the shared daily journal straight to the caller after a header that binds
//! the exact input envelope by SHA-256 instead of echoing the raw request. No financial bound,
//! tolerance or verification gate changes; only position, work and transport ceilings do.
use portfolio_compute_control::checkpoint;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// Largest raw request any tier admits (the selected tier then applies its own bound).
pub const MAX_TIER_BYTES: usize = 512 * 1024 * 1024;
/// Complete streamed native journal bound for the acceptance tier.
pub const MAX_TIER_STREAM: u64 = 16 * 1024 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleTier {
    #[serde(rename = "acceptance_60000")]
    Acceptance60000,
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub positions: usize,
    pub work: usize,
    pub raw_bytes: usize,
}
pub fn limits(tier: Option<ScaleTier>) -> Limits {
    match tier {
        None => Limits {
            positions: 512,
            work: portfolio_ledger_native::owners::derivatives::MAX_WORK,
            raw_bytes: 32 * 1024 * 1024,
        },
        Some(ScaleTier::Acceptance60000) => Limits {
            positions: 60_000,
            work: 90_000_000,
            raw_bytes: MAX_TIER_BYTES,
        },
    }
}

/// Where a streamed tier writes: the caller's output and the SHA-256 of the exact envelope bytes.
pub struct Streaming<'a> {
    pub input_sha256: String,
    pub output: &'a mut dyn Write,
}

/// Write the preparation header: the source receipt and the input-envelope hash (no raw echo).
pub fn header(
    output: &mut dyn Write,
    model_schema: &str,
    receipt: &impl Serialize,
    input_sha256: &str,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct Workflow<'a, R: Serialize> {
        schema: &'static str,
        model_schema: &'a str,
        source_receipt: &'a R,
        input_sha256: &'a str,
    }
    #[derive(Serialize)]
    struct Header<'a, R: Serialize> {
        workflow: Workflow<'a, R>,
    }
    let bytes = portfolio_model_core::strict_json::encode(
        &Header {
            workflow: Workflow {
                schema: "model-ledger-preparation-1",
                model_schema,
                source_receipt: receipt,
                input_sha256,
            },
        },
        32 * 1024 * 1024,
    )?;
    checkpoint()?;
    output
        .write_all(&bytes)
        .and_then(|_| output.write_all(b"\n"))
        .map_err(|e| e.to_string())
}

/// Run the shared daily Sim on a scoped thread and forward its journal in bounded chunks.
/// Cancellation and deadlines reach both threads; a failed write stops the Sim.
pub fn run_streaming(
    dated: &portfolio_ledger_native::dated_ledger::CompiledDatedLedger,
    compiled: &portfolio_ledger_native::owners::derivatives::Compiled<'_>,
    output: &mut dyn Write,
) -> Result<(), String> {
    struct Sender {
        tx: std::sync::mpsc::SyncSender<Vec<u8>>,
        buffer: Vec<u8>,
    }
    impl Write for Sender {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            checkpoint().map_err(io::Error::other)?;
            self.buffer.extend_from_slice(data);
            if self.buffer.len() >= CHUNK {
                let chunk = std::mem::replace(&mut self.buffer, Vec::with_capacity(CHUNK));
                self.tx
                    .send(chunk)
                    .map_err(|_| io::Error::other("streamed journal receiver closed"))?;
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if !self.buffer.is_empty() {
                let chunk = std::mem::take(&mut self.buffer);
                self.tx
                    .send(chunk)
                    .map_err(|_| io::Error::other("streamed journal receiver closed"))?;
            }
            Ok(())
        }
    }
    impl Drop for Sender {
        fn drop(&mut self) {
            let _ = self.flush();
        }
    }
    let control = portfolio_compute_control::active();
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
    std::thread::scope(|scope| -> Result<(), String> {
        let sim = scope.spawn(move || {
            portfolio_compute_control::scope(control, || {
                portfolio_ledger_native::daily::run_dated_with_derivatives(
                    dated,
                    compiled,
                    Box::new(Sender {
                        tx,
                        buffer: Vec::with_capacity(CHUNK),
                    }),
                )
            })
        });
        let mut written = 0u64;
        let mut forward = || -> Result<(), String> {
            for chunk in rx.iter() {
                checkpoint()?;
                written = written
                    .checked_add(chunk.len() as u64)
                    .filter(|n| *n <= MAX_TIER_STREAM)
                    .ok_or("streamed mixed journal exceeds the acceptance tier bound")?;
                output.write_all(&chunk).map_err(|e| e.to_string())?;
            }
            Ok(())
        };
        let forwarded = forward();
        // Dropping the receiver stops a Sim still writing after a forwarding failure.
        drop(rx);
        let simulated = sim
            .join()
            .map_err(|_| "streamed mixed journal thread panicked".to_string())?;
        simulated?;
        forwarded?;
        output.flush().map_err(|e| e.to_string())?;
        checkpoint()
    })
}
