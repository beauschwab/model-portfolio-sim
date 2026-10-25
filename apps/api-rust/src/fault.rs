//! Fault points for process tests. They compile to nothing unless the crate is built with the
//! `fault-injection` feature, which release builds and deployments never enable.

/// Delays at a named point when `WORKBENCH_FAULT_<POINT>_MS` is set (feature builds only).
pub fn pause(point: &str) {
    #[cfg(feature = "fault-injection")]
    if let Ok(ms) = std::env::var(format!("WORKBENCH_FAULT_{}_MS", point.to_ascii_uppercase())) {
        if let Ok(ms) = ms.parse::<u64>() {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
    }
    #[cfg(not(feature = "fault-injection"))]
    let _ = point;
}

/// Whether the heartbeat should stop renewing (simulates a stalled worker; feature builds only).
pub fn heartbeat_stalled() -> bool {
    #[cfg(feature = "fault-injection")]
    {
        std::env::var("WORKBENCH_FAULT_STALL_HEARTBEAT").as_deref() == Ok("1")
    }
    #[cfg(not(feature = "fault-injection"))]
    false
}

pub const ENABLED: bool = cfg!(feature = "fault-injection");
