use std::time::{Duration, Instant};

#[derive(Default)]
pub struct IdleTracker {
    empty_since: Option<Instant>,
}

impl IdleTracker {
    /// `true` when the hotspot should be turned off for being idle.
    pub fn tick(&mut self, on: bool, clients: usize, now: Instant, timeout_min: u32) -> bool {
        if !on || clients > 0 || timeout_min == 0 {
            self.empty_since = None;
            return false;
        }
        let since = *self.empty_since.get_or_insert(now);
        now.duration_since(since) >= Duration::from_secs(u64::from(timeout_min) * 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn min(m: u64) -> Duration { Duration::from_secs(m * 60) }

    #[test]
    fn shuts_down_after_timeout_without_clients() {
        let mut t = IdleTracker::default();
        let base = Instant::now();
        assert!(!t.tick(true, 0, base, 10));
        assert!(!t.tick(true, 0, base + min(9), 10));
        assert!(t.tick(true, 0, base + min(10), 10));
    }

    #[test]
    fn client_resets_the_clock() {
        let mut t = IdleTracker::default();
        let base = Instant::now();
        t.tick(true, 0, base, 10);
        assert!(!t.tick(true, 1, base + min(9), 10));
        assert!(!t.tick(true, 0, base + min(12), 10)); // counting restarts at +12
        assert!(t.tick(true, 0, base + min(22), 10));
    }

    #[test]
    fn disabled_when_zero_or_off() {
        let mut t = IdleTracker::default();
        let base = Instant::now();
        assert!(!t.tick(true, 0, base, 0));
        assert!(!t.tick(true, 0, base + min(500), 0));
        assert!(!t.tick(false, 0, base + min(500), 10));
    }
}
