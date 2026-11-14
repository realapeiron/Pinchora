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
        now + chrono::Duration::nanoseconds(self.shift.load(Ordering::Relaxed))
    }

    /// Fixes wall time for deterministic integration tests; shifts still apply to every reader.
    pub fn freeze(&self, at: DateTime<Utc>) {
        *self.fixed.write().expect("clock") = Some(at);
    }

    /// Retains App's clock control for integration tests during the service transition.
    pub fn shift(&self, by: chrono::Duration) {
        let nanos = by.num_nanoseconds().expect("a clock shift fits in nanoseconds");
        self.shift.fetch_add(nanos, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::clock::Clock;
    use chrono::{DateTime, Utc};

    #[test]
    fn a_shift_keeps_fractions_of_a_second() {
        let clock = Clock::default();
        let at = DateTime::parse_from_rfc3339("2026-01-01T00:00:59.700Z")
            .expect("time")
            .with_timezone(&Utc);
        clock.freeze(at);
        clock.shift(chrono::Duration::milliseconds(1_500));
        assert_eq!(clock.now(), at + chrono::Duration::milliseconds(1_500));
    }
}
