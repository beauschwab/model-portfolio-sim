//! Shared bounded compute pool and lifecycle execution scope.
use std::sync::{Arc, Mutex, OnceLock};

type PoolSlot = Option<(usize, Arc<rayon::ThreadPool>)>;
static POOL: OnceLock<Mutex<PoolSlot>> = OnceLock::new();

pub(crate) fn pool(threads: usize) -> Result<Arc<rayon::ThreadPool>, i32> {
    let mut slot = POOL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| 3)?;
    if let Some((n, value)) = slot.as_ref() {
        if *n == threads {
            return Ok(value.clone());
        }
    }
    let value = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|_| 3)?,
    );
    *slot = Some((threads, value.clone()));
    Ok(value)
}

/// Run an owned lifecycle on the bounded shared compute pool.
pub fn with_compute_threads<T: Send>(
    threads: usize,
    work: impl FnOnce() -> Result<T, String> + Send,
) -> Result<T, String> {
    if !(1..=256).contains(&threads) {
        return Err("invalid compute thread budget".into());
    }
    let control = portfolio_compute_control::active();
    pool(threads)
        .map_err(|_| "native thread pool unavailable")?
        .install(|| portfolio_compute_control::scope(control, work))
}
