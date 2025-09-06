// src/notify/antiflutter.rs
use chrono::{DateTime, Duration as ChronoDuration, Utc};

use super::DecisionKind;

/// Simple cooldown gate to prevent notification spam.
/// - First alert always allowed.
/// - Inside cooldown, alerts are suppressed.
/// - State is updated explicitly via `record_alert` after a successful send.
#[derive(Debug, Clone, Default)]
pub struct AntiFlutter {
    cooldown: ChronoDuration,
    last_alert_ts: Option<DateTime<Utc>>,
    last_kind: Option<DecisionKind>,
}

impl AntiFlutter {
    /// `cooldown_secs` < 0 is treated as 0 (no cooldown).
    pub fn new(cooldown_secs: i64) -> Self {
        let secs = cooldown_secs.max(0);
        Self {
            cooldown: ChronoDuration::seconds(secs),
            last_alert_ts: None,
            last_kind: None,
        }
    }

    /// Check if we may alert at `now` for `kind`. Does NOT mutate state.
    pub fn should_alert(&self, _kind: DecisionKind, now: DateTime<Utc>) -> bool {
        match self.last_alert_ts {
            None => true,
            Some(ts) => now.signed_duration_since(ts) >= self.cooldown,
        }
    }

    /// Record that an alert was sent at `now` for `kind`.
    pub fn record_alert(&mut self, kind: DecisionKind, now: DateTime<Utc>) {
        self.last_alert_ts = Some(now);
        self.last_kind = Some(kind);
    }

    #[cfg(test)]
    pub fn last_kind(&self) -> Option<DecisionKind> {
        self.last_kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn first_alert_passes() {
        let af = AntiFlutter::new(10_800);
        let now = Utc.with_ymd_and_hms(2025, 9, 6, 9, 0, 0).unwrap();
        assert!(af.should_alert(DecisionKind::BUY, now));
    }

    #[test]
    fn inside_cooldown_blocked() {
        let mut af = AntiFlutter::new(10_800);
        let t0 = Utc.with_ymd_and_hms(2025, 9, 6, 9, 0, 0).unwrap();
        assert!(af.should_alert(DecisionKind::BUY, t0));
        af.record_alert(DecisionKind::BUY, t0);
        let t1 = t0 + ChronoDuration::seconds(120);
        assert!(!af.should_alert(DecisionKind::BUY, t1));
    }

    #[test]
    fn after_cooldown_passes() {
        let mut af = AntiFlutter::new(10_800);
        let t0 = Utc.with_ymd_and_hms(2025, 9, 6, 9, 0, 0).unwrap();
        assert!(af.should_alert(DecisionKind::BUY, t0));
        af.record_alert(DecisionKind::BUY, t0);
        let t_after = t0 + ChronoDuration::seconds(10_800 + 5);
        assert!(af.should_alert(DecisionKind::SELL, t_after));
    }
}
