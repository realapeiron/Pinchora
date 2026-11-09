use chrono::{DateTime, Utc};
use std::sync::{
    RwLock,
    atomic::{AtomicI64, Ordering},
};

#[derive(Default)]
pub struct Clock {
    shift: AtomicI64,
    fixed: RwLock<Option<DateTime<Utc>>>,
}

impl Clock {
    pub fn now(&self) -> DateTime<Utc> {
        let now = self.fixed.read().expect("clock").unwrap_or_else(Utc::now);
        now + chrono::Duration::seconds(self.shift.load(Ordering::Relaxed))
    }

    /// Fixes wall time for deterministic integration tests; shifts still apply to every reader.
    pub fn freeze(&self, at: DateTime<Utc>) {
        *self.fixed.write().expect("clock") = Some(at);
    }

    /// Retains App's clock control for integration tests during the service transition.
    pub fn shift(&self, by: chrono::Duration) {
        self.shift.fetch_add(by.num_seconds(), Ordering::Relaxed);
    }
}
