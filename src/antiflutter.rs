use chrono::{DateTime, Duration, Utc};

use crate::notify::DecisionKind;

#[derive(Debug, Clone)]
pub struct AntiFlutter {
    cooldown: Duration,
    pub last_alert_at: Option<DateTime<Utc>>,
    pub last_alert_kind: Option<DecisionKind>,
}

impl AntiFlutter {
    pub fn new(cooldown_secs: i64) -> Self {
        Self {
            cooldown: Duration::seconds(cooldown_secs),
            last_alert_at: None,
            last_alert_kind: None,
        }
    }

    /// Returns true if we should alert given the new decision at time `now`.
    pub fn should_alert(&self, new_kind: DecisionKind, now: DateTime<Utc>) -> bool {
        match (self.last_alert_at, self.last_alert_kind) {
            (None, _) => true, // first after quiet period
            (Some(last_at), Some(last_kind)) => {
                if now - last_at >= self.cooldown {
                    return true; // cooldown expired
                }
                // During cooldown: only BUY <-> SELL pass; HOLD or same-kind suppressed
                matches!(
                    (last_kind, new_kind),
                    (DecisionKind::BUY, DecisionKind::SELL)
                        | (DecisionKind::SELL, DecisionKind::BUY)
                )
            }
            _ => true,
        }
    }

    pub fn record_alert(&mut self, kind: DecisionKind, now: DateTime<Utc>) {
        self.last_alert_kind = Some(kind);
        self.last_alert_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oscillation_max_one_within_window() {
        let mut af = AntiFlutter::new(10); // 10s window
        let t0 = Utc::now();
        assert!(af.should_alert(DecisionKind::HOLD, t0)); // first passes
        af.record_alert(DecisionKind::HOLD, t0);

        // Within cooldown: HOLD->SELL should pass (kind change BUY/SELL only) => SELL passes, HOLD suppressed
        let t1 = t0 + Duration::seconds(3);
        assert!(af.should_alert(DecisionKind::SELL, t1));
        af.record_alert(DecisionKind::SELL, t1);

        let t2 = t1 + Duration::seconds(3);
        assert!(!af.should_alert(DecisionKind::HOLD, t2));

        // After window: alerts pass again
        let t3 = t0 + Duration::seconds(12);
        assert!(af.should_alert(DecisionKind::HOLD, t3));
    }
}
