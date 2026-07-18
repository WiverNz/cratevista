//! An injected clock so graph assembly stays deterministic and only
//! `cratevista-core` reads wall-clock time (recorded in `generation.json`).

/// Provides the current time as an RFC-3339 string.
///
/// `Send + Sync` because watch mode regenerates on a blocking pool: the clock
/// outlives one call and is stamped by whichever thread runs the regeneration.
pub trait Clock: Send + Sync {
    /// The current UTC time formatted as RFC 3339.
    fn now_rfc3339(&self) -> String;
}

/// The real system clock (UTC, RFC 3339).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    }
}

/// A fixed clock for deterministic tests.
#[derive(Debug, Clone)]
pub struct FixedClock(pub String);

impl Clock for FixedClock {
    fn now_rfc3339(&self) -> String {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_produces_rfc3339_like_string() {
        let now = SystemClock.now_rfc3339();
        // "YYYY-MM-DDT..." shape.
        assert!(now.len() >= 20, "{now}");
        assert_eq!(now.as_bytes()[4], b'-');
        assert_eq!(now.as_bytes()[10], b'T');
    }

    #[test]
    fn fixed_clock_is_stable() {
        let clock = FixedClock("2026-07-14T00:00:00Z".into());
        assert_eq!(clock.now_rfc3339(), "2026-07-14T00:00:00Z");
    }
}
