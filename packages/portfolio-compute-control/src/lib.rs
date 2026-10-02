//! Cooperative bounded-batch cancellation shared by native calculation stages.
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Control {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl Control {
    pub fn new(timeout_ms: u64) -> Result<Self, String> {
        if !(1..=3_600_000).contains(&timeout_ms) {
            return Err("timeout_ms must be in [1, 3600000]".into());
        }
        Ok(Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_millis(timeout_ms),
        })
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("COMPUTE_CANCELLED".into())
        } else if Instant::now() >= self.deadline {
            Err("COMPUTE_DEADLINE_EXCEEDED".into())
        } else {
            Ok(())
        }
    }
}
thread_local! { static ACTIVE:RefCell<Option<Control>>=const {RefCell::new(None)}; }
pub fn active() -> Option<Control> {
    ACTIVE.with(|slot| slot.borrow().clone())
}
pub fn checkpoint() -> Result<(), String> {
    ACTIVE.with(|slot| slot.borrow().as_ref().map_or(Ok(()), Control::check))
}
pub fn scope<T>(
    control: Option<Control>,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    struct Restore(Option<Control>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(ACTIVE.with(|slot| slot.replace(control)));
    checkpoint()?;
    let result = work()?;
    checkpoint()?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_and_scope_restore() {
        let c = Control::new(1000).unwrap();
        c.cancel();
        assert_eq!(scope(Some(c), || Ok(1)).unwrap_err(), "COMPUTE_CANCELLED");
        assert!(checkpoint().is_ok());
    }
    #[test]
    fn deadline_is_checked() {
        let c = Control {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now(),
        };
        assert_eq!(
            scope(Some(c), || Ok(1)).unwrap_err(),
            "COMPUTE_DEADLINE_EXCEEDED"
        );
    }
}
