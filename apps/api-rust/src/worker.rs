//! The durable leased worker. One process per workspace holds the workspace lease, which is the
//! only compute ownership; it takes the lease only while the workspace is routed to Rust, and
//! a routing change (a new epoch) fences it. Each attempt reads its raw input, executable
//! identity and original deadline from the immutable authority row, runs the native producer
//! under a watchdog (deadline, cancellation, lease loss, output bound), verifies the stream
//! independently, writes and rereads checksummed objects, and publishes only through the final
//! fenced transaction.
use crate::config::Config;
use crate::objects::{Descriptor, Objects};
use crate::repo::{Claim, Repo};
use crate::{fault, now, publish, Refusal};
use portfolio_verification_native::{verify, MAX_REQUEST_BYTES, MAX_STREAM_BYTES};
use serde_json::json;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_STDERR_BYTES: usize = 64 * 1024;

pub fn executable_sha256(path: &Path) -> Result<String, Refusal> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)
        .map_err(|e| Refusal::new("EXECUTABLE_UNAVAILABLE", format!("{}: {e}", path.display())))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| Refusal::new("EXECUTABLE_UNAVAILABLE", e.to_string()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(crate::hex(&h.finalize()))
}

/// A native admission refusal (`CODE: detail` on stderr) keeps the native code, so a refused plan
/// reports why (for example `INSUFFICIENT_SETTLEMENT_LIQUIDITY` or `FORMAL_OWNER_REQUIRED`);
/// anything else is a process failure.
pub fn native_refusal(status: &str, stderr: &str) -> Refusal {
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if let Some((code, detail)) = line.split_once(": ") {
        if !code.is_empty()
            && code.len() <= 64
            && code.bytes().all(|c| c.is_ascii_uppercase() || c == b'_')
        {
            let mut r = Refusal::new("NATIVE_REFUSED", detail);
            r.detail = format!("{code}: {detail}");
            return r;
        }
    }
    Refusal::new(
        "NATIVE_FAILED",
        format!("native process exited with {status}: {stderr}"),
    )
}

pub fn executable_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub struct Worker {
    pub config: Config,
    pub repo: Repo,
    pub objects: Objects,
}

/// A fresh lease identity. Every lease tenure uses its own, so an attempt this process abandoned
/// after losing its lease is requeued (the claim requeues running work of any other owner) when
/// the process takes the lease again.
fn tenure_owner() -> String {
    format!("rust:{}", uuid::Uuid::new_v4().simple())
}

/// Outcomes that discard this attempt's output without recording anything: the job belongs to
/// another attempt, was cancelled, or the workspace moved.
fn discards(code: &str) -> bool {
    matches!(
        code,
        "LEASE_LOST" | "ROUTE_CHANGED" | "SUPERSEDED" | "CANCELLED"
    )
}

impl Worker {
    pub fn new(config: Config) -> Result<Worker, String> {
        let db = crate::sql::Db::open(&config.database_url)?;
        crate::migrate::run(&db)?;
        let repo = Repo::new(db, &config.tenant, &config.workspace);
        let objects = Objects::open(&config.artifact_url, &config.tenant, &config.workspace)?;
        Ok(Worker {
            config,
            repo,
            objects,
        })
    }

    /// Runs until stopped. With `exit_when_idle`, returns once no lease can be taken or the
    /// queue is empty, after releasing a lease it still owns.
    pub fn run(&self, exit_when_idle: bool, stop: &AtomicBool) -> Result<(), String> {
        let mut idle_rounds = 0;
        let mut owner = tenure_owner();
        while !stop.load(Ordering::SeqCst) {
            let epoch = match self.repo.acquire(&owner, self.config.lease_seconds) {
                Ok(Some(epoch)) => epoch,
                Ok(None) | Err(_) => {
                    idle_rounds += 1;
                    if exit_when_idle && idle_rounds > 20 {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
            };
            idle_rounds = 0;
            eprintln!("worker {owner} holds the workspace lease (epoch {epoch})");
            let lost = Arc::new(AtomicBool::new(false));
            let done = Arc::new(AtomicBool::new(false));
            let heartbeat = {
                let (lost, done) = (lost.clone(), done.clone());
                let db_url = self.config.database_url.clone();
                let (tenant, workspace) =
                    (self.config.tenant.clone(), self.config.workspace.clone());
                let owner = owner.clone();
                let seconds = self.config.lease_seconds;
                std::thread::spawn(move || {
                    let Ok(db) = crate::sql::Db::open(&db_url) else {
                        lost.store(true, Ordering::SeqCst);
                        return;
                    };
                    let repo = Repo::new(db, &tenant, &workspace);
                    let period = Duration::from_secs_f64((seconds / 3.).clamp(0.2, 2.));
                    while !done.load(Ordering::SeqCst) {
                        std::thread::sleep(period);
                        if fault::heartbeat_stalled() {
                            continue;
                        }
                        if !matches!(repo.renew(&owner, epoch, seconds), Ok(true)) {
                            lost.store(true, Ordering::SeqCst);
                            return;
                        }
                    }
                })
            };
            let mut drained = false;
            while !lost.load(Ordering::SeqCst) && !stop.load(Ordering::SeqCst) {
                match self.repo.claim(&owner, epoch) {
                    Ok(Some(claim)) => self.execute(&claim, &lost),
                    Ok(None) => {
                        if exit_when_idle {
                            drained = true;
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(200));
                    }
                    Err(r) => {
                        eprintln!("claim refused: {r}");
                        lost.store(true, Ordering::SeqCst);
                    }
                }
            }
            done.store(true, Ordering::SeqCst);
            let _ = heartbeat.join();
            if lost.load(Ordering::SeqCst) {
                owner = tenure_owner();
            } else {
                let _ = self.repo.release(&owner);
            }
            if drained {
                return Ok(());
            }
        }
        let _ = self.repo.release(&owner);
        Ok(())
    }

    fn execute(&self, claim: &Claim, lost: &AtomicBool) {
        match self.attempt(claim, lost) {
            Ok(()) => eprintln!("published {} (attempt {})", claim.job_id, claim.attempt),
            Err(r) if discards(r.code) => {
                eprintln!("discarded {} attempt {}: {r}", claim.job_id, claim.attempt)
            }
            Err(r) => {
                eprintln!("refused {} attempt {}: {r}", claim.job_id, claim.attempt);
                if let Err(e) = self.repo.fail(claim, &r.to_string()) {
                    eprintln!("could not record the refusal: {e}");
                }
            }
        }
    }

    fn attempt(&self, claim: &Claim, lost: &AtomicBool) -> Result<(), Refusal> {
        let a = &claim.authority;
        // The returned job view is never authority. It must still agree with the immutable row.
        let view_raw = Descriptor::from_json(&claim.view["raw"]).ok();
        if claim.view["authority_sha256"] != claim.authority_sha256.as_str()
            || view_raw.as_ref() != Some(&a.raw)
            || claim.view["schema"] != a.schema.as_str()
            || a.digest() != claim.authority_sha256
        {
            return Err(Refusal::new(
                "AUTHORITY_MISMATCH",
                "the job view differs from its immutable admission record",
            ));
        }
        if a.schema != crate::LIFECYCLE_SCHEMA || a.verifier != crate::VERIFIER {
            return Err(Refusal::new(
                "UNSUPPORTED_SCHEMA",
                "this worker cannot verify the admitted schema",
            ));
        }
        let raw = self.objects.get_bounded(&a.raw, MAX_REQUEST_BYTES as u64)?;
        if crate::sha256_hex(&raw) != a.raw.sha256 {
            return Err(Refusal::new(
                "AUTHORITY_MISMATCH",
                "raw input identity differs",
            ));
        }
        if now() >= a.deadline_at {
            return Err(Refusal::new(
                "DEADLINE_EXPIRED",
                "the original deadline passed before execution",
            ));
        }
        let exe = &self.config.ledger_exe;
        if executable_name(exe) != a.executable_name
            || executable_sha256(exe)? != a.executable_sha256
        {
            return Err(Refusal::new(
                "EXECUTABLE_CHANGED",
                "the native executable differs from the one admitted; submit a new run",
            ));
        }
        self.repo.progress(
            claim,
            json!({"stage": "native", "pct": 10, "attempt": claim.attempt}),
        )?;
        let stream = self.native(claim, exe, &raw, lost)?;
        self.repo.progress(
            claim,
            json!({"stage": "verifying", "pct": 60, "attempt": claim.attempt}),
        )?;
        let receipt = verify(&raw, &stream).map_err(|e| Refusal::new("VERIFICATION_REFUSED", e))?;
        self.repo.progress(
            claim,
            json!({"stage": "publishing", "pct": 75, "attempt": claim.attempt}),
        )?;
        let (manifest, value) = publish::write(&self.objects, claim, &raw, &stream, &receipt)?;
        publish::reread(&self.objects, &manifest, &value)?;
        if executable_sha256(exe)? != a.executable_sha256 {
            return Err(Refusal::new(
                "EXECUTABLE_CHANGED",
                "the native executable changed during the run",
            ));
        }
        fault::pause("final_io");
        let result = json!({
            "manifest": manifest.to_json(),
            "stream_sha256": value["stream"]["sha256"],
            "receipt": value["verification"]["receipt"],
            "persisted_verification": true,
        });
        self.repo.publish(claim, &manifest, result)
    }

    /// Runs the producer as a child process. Its input is the envelope `{"execution_timeout_ms": <ms>, "request": <raw>}`
    /// and the remaining time to the original deadline, with the raw bytes unchanged. The watchdog kills it on the original deadline, lease loss,
    /// cancellation or an output beyond the stream bound.
    fn native(
        &self,
        claim: &Claim,
        exe: &Path,
        raw: &[u8],
        lost: &AtomicBool,
    ) -> Result<Vec<u8>, Refusal> {
        let mut child = Command::new(exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Refusal::new("EXECUTABLE_UNAVAILABLE", e.to_string()))?;
        let mut stdin = child.stdin.take().expect("stdin");
        // The native cooperative deadline is the time left until the original admission deadline.
        let remaining_ms = ((claim.authority.deadline_at - now()) * 1000.)
            .floor()
            .clamp(1., 3_600_000.) as u64;
        let envelope = [
            format!("{{\"execution_timeout_ms\":{remaining_ms},\"request\":").as_bytes(),
            raw,
            b"}",
        ]
        .concat();
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&envelope);
        });
        let overflow = Arc::new(AtomicBool::new(false));
        let out = Arc::new(Mutex::new(Vec::new()));
        let reader = {
            let (out, overflow) = (out.clone(), overflow.clone());
            let mut stdout = child.stdout.take().expect("stdout");
            std::thread::spawn(move || {
                let mut buf = vec![0u8; 1 << 16];
                loop {
                    match stdout.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut o = out.lock().expect("stdout buffer");
                            if o.len() + n > MAX_STREAM_BYTES {
                                overflow.store(true, Ordering::SeqCst);
                                break;
                            }
                            o.extend_from_slice(&buf[..n]);
                        }
                    }
                }
            })
        };
        let err = Arc::new(Mutex::new(Vec::new()));
        let err_reader = {
            let err = err.clone();
            let mut stderr = child.stderr.take().expect("stderr");
            std::thread::spawn(move || {
                let mut buf = vec![0u8; 1 << 14];
                while let Ok(n) = stderr.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut e = err.lock().expect("stderr buffer");
                    e.extend_from_slice(&buf[..n]);
                    if e.len() > MAX_STDERR_BYTES {
                        let cut = e.len() - MAX_STDERR_BYTES;
                        e.drain(..cut);
                    }
                }
            })
        };
        let mut last_check = Instant::now();
        let stopped: Option<Refusal> = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| Refusal::new("NATIVE_FAILED", e.to_string()))?
            {
                if !status.success() {
                    let _ = err_reader.join();
                    let tail = String::from_utf8_lossy(&err.lock().expect("stderr")).into_owned();
                    break Some(native_refusal(&status.to_string(), &tail));
                }
                break None;
            }
            let reason = if now() >= claim.authority.deadline_at {
                Some(Refusal::new(
                    "DEADLINE_EXPIRED",
                    "the original deadline passed during the native run",
                ))
            } else if lost.load(Ordering::SeqCst) {
                Some(Refusal::new(
                    "LEASE_LOST",
                    "worker lease lost during the native run",
                ))
            } else if overflow.load(Ordering::SeqCst) {
                Some(Refusal::new(
                    "STREAM_BOUND",
                    "native output exceeds the stream bound",
                ))
            } else if last_check.elapsed() > Duration::from_millis(300)
                && !fault::heartbeat_stalled()
            {
                // A stalled (paused) process polls nothing; the final fence must still refuse it.
                last_check = Instant::now();
                match self.repo.current(claim) {
                    Ok(true) => None,
                    Ok(false) => Some(Refusal::new(
                        "CANCELLED",
                        "job cancelled or superseded during the native run",
                    )),
                    Err(e) => Some(e),
                }
            } else {
                None
            };
            if let Some(r) = reason {
                let _ = child.kill();
                let _ = child.wait();
                break Some(r);
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let _ = writer.join();
        let _ = reader.join();
        if let Some(r) = stopped {
            return Err(r);
        }
        if overflow.load(Ordering::SeqCst) {
            return Err(Refusal::new(
                "STREAM_BOUND",
                "native output exceeds the stream bound",
            ));
        }
        let out = std::mem::take(&mut *out.lock().expect("stdout"));
        Ok(out)
    }
}
