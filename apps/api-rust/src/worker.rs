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
use portfolio_verification_native::{
    admit_request, stream_bound, verify, verify_reader, MAX_STREAM_BYTES, MAX_TIER_REQUEST_BYTES,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_STDERR_BYTES: usize = 64 * 1024;

/// How the producer's peak resident memory is measured on this platform. On Windows the peak
/// working set is read from the child handle at exit (exact). On Linux the high-water mark
/// (`VmHWM`) is sampled every watchdog tick while the child lives, so it is a lower bound that
/// can miss growth in the last tick. Elsewhere it is not measured.
#[cfg(windows)]
pub const RSS_METHOD: &str = "windows_peak_working_set_at_exit";
#[cfg(target_os = "linux")]
pub const RSS_METHOD: &str = "linux_vmhwm_sampled_lower_bound";
#[cfg(not(any(windows, target_os = "linux")))]
pub const RSS_METHOD: &str = "unavailable";

#[cfg(windows)]
fn peak_rss(child: &std::process::Child) -> Option<u64> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: a zeroed counters struct is valid; the handle stays owned by `child` for the call.
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = size;
    let ok = unsafe { GetProcessMemoryInfo(child.as_raw_handle() as _, &mut c, size) };
    (ok != 0).then_some(c.PeakWorkingSetSize as u64)
}
#[cfg(target_os = "linux")]
fn peak_rss(child: &std::process::Child) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}
#[cfg(not(any(windows, target_os = "linux")))]
fn peak_rss(_: &std::process::Child) -> Option<u64> {
    None
}

/// The worker process's own lifetime peak resident memory (it buffers the bounded stream and runs
/// the independent verification and publication), exact on Windows and Linux.
#[cfg(windows)]
fn own_peak_rss() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: a zeroed counters struct is valid; the pseudo handle needs no closing.
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = size;
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, size) };
    (ok != 0).then_some(c.PeakWorkingSetSize as u64)
}
#[cfg(target_os = "linux")]
fn own_peak_rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}
#[cfg(not(any(windows, target_os = "linux")))]
fn own_peak_rss() -> Option<u64> {
    None
}

/// The producer's stream, spooled to a private temporary file as it arrives, with its exact byte
/// count and SHA-256. Verification, publication and the persisted reread read it incrementally,
/// so the worker never holds a large stream in memory. The file is removed when dropped.
pub struct Spool {
    path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}
impl Spool {
    pub fn open(&self) -> Result<std::fs::File, Refusal> {
        std::fs::File::open(&self.path).map_err(|e| Refusal::new("STORAGE_ERROR", e.to_string()))
    }
    /// The whole stream, for routes whose stream stays within the default bound.
    pub fn read_all(&self) -> Result<Vec<u8>, Refusal> {
        if self.bytes > MAX_STREAM_BYTES as u64 {
            return Err(Refusal::new(
                "STREAM_BOUND",
                "native output exceeds the stream bound",
            ));
        }
        std::fs::read(&self.path).map_err(|e| Refusal::new("STORAGE_ERROR", e.to_string()))
    }
}
impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Measured resources of one producer run, recorded in the job result (stage I).
pub struct Resources {
    pub peak_rss_bytes: Option<u64>,
    pub input_bytes: usize,
    pub stream_bytes: u64,
    pub stream_bound: u64,
    pub wall_ms: u64,
}
impl Resources {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "peak_rss_bytes": self.peak_rss_bytes, "rss_method": RSS_METHOD,
            "input_bytes": self.input_bytes, "stream_bytes": self.stream_bytes,
            "stream_bound_bytes": self.stream_bound, "wall_ms": self.wall_ms,
            "worker_lifetime_peak_rss_bytes": own_peak_rss(),
        })
    }
}

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
        let known = a.schema == crate::LIFECYCLE_SCHEMA || a.schema == crate::allocation::SCHEMA;
        if !known || a.verifier != crate::VERIFIER {
            return Err(Refusal::new(
                "UNSUPPORTED_SCHEMA",
                "this worker cannot verify the admitted schema",
            ));
        }
        let raw = self
            .objects
            .get_bounded(&a.raw, MAX_TIER_REQUEST_BYTES as u64)?;
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
        // Only an explicit lifecycle `acceptance_60000` request has the large stream bound.
        let tier = a.schema == crate::LIFECYCLE_SCHEMA
            && admit_request(&raw)
                .map_err(|e| Refusal::new("MALFORMED_INPUT", e))?
                .1;
        let (spool, resources) = self.native(claim, exe, &raw, lost, stream_bound(tier))?;
        self.repo.progress(
            claim,
            json!({"stage": "verifying", "pct": 60, "attempt": claim.attempt}),
        )?;
        if a.schema == crate::allocation::SCHEMA {
            let stream = spool.read_all()?;
            return self.allocation(claim, exe, &raw, &stream, &resources);
        }
        let receipt = verify_reader(&raw, std::io::BufReader::new(spool.open()?))
            .map_err(|e| Refusal::new("VERIFICATION_REFUSED", e))?;
        self.repo.progress(
            claim,
            json!({"stage": "publishing", "pct": 75, "attempt": claim.attempt}),
        )?;
        let (manifest, value) = publish::write(
            &self.objects,
            claim,
            &raw,
            &mut spool.open()?,
            &receipt,
            None,
        )?;
        if value["stream"]["sha256"] != spool.sha256.as_str()
            || value["stream"]["bytes"].as_u64() != Some(spool.bytes)
        {
            return Err(Refusal::new(
                "REREAD_MISMATCH",
                "the published stream differs from the producer's spooled output",
            ));
        }
        self.repo.progress(
            claim,
            json!({"stage": "rereading", "pct": 85, "attempt": claim.attempt}),
        )?;
        fault::pause("reread");
        publish::reread(&self.objects, &manifest, &value)?;
        // The reread passed: what remains is the fenced final publication.
        self.repo.progress(
            claim,
            json!({"stage": "publishing", "pct": 95, "attempt": claim.attempt}),
        )?;
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
            "resources": resources.to_json(),
        });
        self.repo.publish(claim, &manifest, result)
    }

    /// An allocation: the producer's receipt line, then (only for a passed exact replay) the replay
    /// stream. The merge is recomputed independently and the stream verified before anything is
    /// marked validated; an unvalidated allocation still publishes its receipt and violation.
    fn allocation(
        &self,
        claim: &Claim,
        exe: &Path,
        raw: &[u8],
        stream: &[u8],
        resources: &Resources,
    ) -> Result<(), Refusal> {
        let layout = |d: String| Refusal::new("STREAM_LAYOUT", d);
        let cut = stream
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| layout("allocation receipt line missing".into()))?;
        let head: serde_json::Value =
            serde_json::from_slice(&stream[..cut]).map_err(|e| layout(e.to_string()))?;
        let receipt = head
            .get("allocation")
            .cloned()
            .ok_or_else(|| layout("allocation receipt missing".into()))?;
        let rest = &stream[cut + 1..];
        let request: serde_json::Value = serde_json::from_slice(raw)
            .map_err(|e| Refusal::new("MALFORMED_INPUT", e.to_string()))?;
        let combined = crate::allocation::check(&request, &receipt, rest)?;
        self.repo.progress(
            claim,
            json!({"stage": "publishing", "pct": 75, "attempt": claim.attempt}),
        )?;
        let (inner, inner_value) = match &combined {
            Some(bytes) => {
                let verified =
                    verify(bytes, rest).map_err(|e| Refusal::new("VERIFICATION_REFUSED", e))?;
                let d = self.objects.put(bytes, "json")?;
                let mut reader = rest;
                let (m, v) = publish::write(
                    &self.objects,
                    claim,
                    bytes,
                    &mut reader,
                    &verified,
                    Some(&d),
                )?;
                (Some(m), Some(v))
            }
            None => (None, None),
        };
        let (manifest, value) = publish::write_allocation(
            &self.objects,
            claim,
            &receipt,
            combined.is_some(),
            inner.as_ref(),
        )?;
        publish::reread_allocation(&self.objects, &manifest, &value, inner_value.as_ref())?;
        if executable_sha256(exe)? != claim.authority.executable_sha256 {
            return Err(Refusal::new(
                "EXECUTABLE_CHANGED",
                "the native executable changed during the run",
            ));
        }
        fault::pause("final_io");
        let result = json!({
            "manifest": manifest.to_json(),
            "allocation": true,
            "validated": combined.is_some(),
            "outcome": receipt["outcome"],
            "selected": receipt["selected"],
            "persisted_verification": true,
            "resources": resources.to_json(),
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
        bound: u64,
    ) -> Result<(Spool, Resources), Refusal> {
        let started = Instant::now();
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
        let dir = std::env::temp_dir().join("workbench-native-spool");
        std::fs::create_dir_all(&dir).map_err(|e| Refusal::new("STORAGE_ERROR", e.to_string()))?;
        let path = dir.join(format!(
            "{}-{}-{}.jsonl",
            claim.job_id,
            claim.attempt,
            uuid::Uuid::new_v4().simple()
        ));
        let file = std::fs::File::create(&path)
            .map_err(|e| Refusal::new("STORAGE_ERROR", e.to_string()))?;
        // Owns the file from here: removed on every exit path.
        let mut spool = Spool {
            path,
            bytes: 0,
            sha256: String::new(),
        };
        let overflow = Arc::new(AtomicBool::new(false));
        let reader = {
            let overflow = overflow.clone();
            let mut stdout = child.stdout.take().expect("stdout");
            let mut file = std::io::BufWriter::with_capacity(1 << 20, file);
            std::thread::spawn(move || -> Result<(u64, String), String> {
                let mut buf = vec![0u8; 1 << 16];
                let mut hasher = Sha256::new();
                let mut total = 0u64;
                loop {
                    match stdout.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if total + n as u64 > bound {
                                overflow.store(true, Ordering::SeqCst);
                                break;
                            }
                            hasher.update(&buf[..n]);
                            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                            total += n as u64;
                        }
                    }
                }
                file.flush().map_err(|e| e.to_string())?;
                Ok((total, crate::hex(&hasher.finalize())))
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
        let mut peak: Option<u64> = None;
        let mut sample = |child: &std::process::Child| {
            if let Some(v) = peak_rss(child) {
                peak = Some(peak.map_or(v, |p| p.max(v)));
            }
        };
        let stopped: Option<Refusal> = loop {
            sample(&child);
            if let Some(status) = child
                .try_wait()
                .map_err(|e| Refusal::new("NATIVE_FAILED", e.to_string()))?
            {
                if !status.success() {
                    let _ = err_reader.join();
                    let tail = String::from_utf8_lossy(&err.lock().expect("stderr")).into_owned();
                    break Some(native_refusal(&status.to_string(), &tail));
                }
                // On Windows the exited child's handle still reports its exact peak.
                sample(&child);
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
        let spooled = reader
            .join()
            .map_err(|_| Refusal::new("STORAGE_ERROR", "the stream spool thread failed"))?;
        if let Some(r) = stopped {
            return Err(r);
        }
        if overflow.load(Ordering::SeqCst) {
            return Err(Refusal::new(
                "STREAM_BOUND",
                "native output exceeds the stream bound",
            ));
        }
        let (bytes, sha256) = spooled.map_err(|e| Refusal::new("STORAGE_ERROR", e))?;
        spool.bytes = bytes;
        spool.sha256 = sha256;
        let resources = Resources {
            peak_rss_bytes: peak,
            input_bytes: raw.len(),
            stream_bytes: bytes,
            stream_bound: bound,
            wall_ms: started.elapsed().as_millis() as u64,
        };
        Ok((spool, resources))
    }
}
