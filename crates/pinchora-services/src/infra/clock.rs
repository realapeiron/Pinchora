use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicI64, Ordering};

#[derive(Default)]
pub struct Clock {
    shift: AtomicI64,
}

impl Clock {
    pub fn now(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(self.shift.load(Ordering::Relaxed))
    }

    /// Retains App's clock control for integration tests during the service transition.
    pub fn shift(&self, by: chrono::Duration) {
        self.shift.fetch_add(by.num_seconds(), Ordering::Relaxed);
    }
}
