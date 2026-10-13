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
    /// Fresh request clock begins at parser entry, rather than after decoding.
    pub fn from_started(timeout_ms: u64, started: Instant) -> Result<Self, String> {
        if !(1..=3_600_000).contains(&timeout_ms) {
            return Err("timeout_ms must be in [1, 3600000]".into());
        }
        let deadline = started
            .checked_add(Duration::from_millis(timeout_ms))
            .ok_or("request deadline overflow")?;
        Ok(Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline,
        })
    }
    /// Same cancellation authority; the tighter original parent/request deadline wins.
    pub fn child_started(&self, timeout_ms: u64, started: Instant) -> Result<Self, String> {
        let requested = Self::from_started(timeout_ms, started)?;
        Ok(Self {
            cancelled: self.cancelled.clone(),
            deadline: self.deadline.min(requested.deadline),
        })
    }

    pub fn new(timeout_ms: u64) -> Result<Self, String> {
        if !(1..=3_600_000).contains(&timeout_ms) {
            return Err("timeout_ms must be in [1, 3600000]".into());
        }
        Ok(Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_millis(timeout_ms),
        })
    }
    /// Fresh deadline sharing only the current native-owned cancellation flag.
    pub fn with_cancel_flag(timeout_ms: u64, cancelled: Arc<AtomicBool>) -> Result<Self, String> {
        if !(1..=3_600_000).contains(&timeout_ms) {
            return Err("timeout_ms must be in [1, 3600000]".into());
        }
        Ok(Self {
            cancelled,
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
    #[test]
    fn child_preserves_both_deadlines_and_cancellation() {
        let now = Instant::now();
        let parent = Control::from_started(1000, now).unwrap();
        let tight = parent.child_started(1, now).unwrap();
        assert_eq!(tight.deadline, now + Duration::from_millis(1));
        let longer = parent.child_started(2000, now).unwrap();
        assert_eq!(longer.deadline, parent.deadline);
        tight.cancel();
        assert_eq!(parent.check().unwrap_err(), "COMPUTE_CANCELLED");
        assert_eq!(longer.check().unwrap_err(), "COMPUTE_CANCELLED");
        assert!(parent.child_started(0, now).is_err());
        assert!(parent.child_started(3_600_001, now).is_err());
    }
    #[test]
    fn parser_time_and_expired_parent_cannot_renew_budget() {
        let past = Instant::now() - Duration::from_secs(2);
        assert_eq!(
            Control::from_started(1, past).unwrap().check().unwrap_err(),
            "COMPUTE_DEADLINE_EXCEEDED"
        );
        let parent = Control::from_started(1, past).unwrap();
        assert_eq!(
            parent
                .child_started(30000, Instant::now())
                .unwrap()
                .check()
                .unwrap_err(),
            "COMPUTE_DEADLINE_EXCEEDED"
        );
    }
    #[test]
    fn child_scope_restores_parent_on_error_and_panic() {
        let parent = Control::new(30000).unwrap();
        scope(Some(parent.clone()), || {
            let child = parent.child_started(10000, Instant::now()).unwrap();
            assert!(scope(Some(child.clone()), || Err::<(), _>("expected".into())).is_err());
            assert_eq!(active().unwrap().deadline, parent.deadline);
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                scope::<()>(Some(child), || panic!("expected panic"))
            }));
            assert!(caught.is_err());
            assert_eq!(active().unwrap().deadline, parent.deadline);
            Ok(())
        })
        .unwrap();
        assert!(active().is_none());
    }
}
