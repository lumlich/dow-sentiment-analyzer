//! # Rolling Window
//! Simple sliding window for informative metrics (default 48h).
//!
//! Collects `(score, timestamp)` pairs and computes average/count over
//! the last window. This is informational only; notifications are handled
//! in the disruption detector.

use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Thread-safe rolling time window over integer sentiment scores.
#[derive(Debug)]
pub struct RollingWindow {
    inner: Mutex<Inner>,
    window: Duration,
}

#[derive(Debug)]
struct Inner {
    /// Stored samples as `(unix_seconds, score)`.
    buf: VecDeque<(u64, i32)>,
}

impl RollingWindow {
    /// Create a new rolling window with the given duration.
    pub fn with_window(window: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner {
                buf: VecDeque::new(),
            }),
            window,
        }
    }

    /// Convenience constructor for 48h window.
    pub fn new_48h() -> Self {
        Self::with_window(Duration::from_secs(48 * 3600))
    }

    /// Record a new observation. If `ts_unix` is `None`, current time is used.
    ///
    /// Automatically discards entries older than the window.
    pub fn record(&self, score: i32, ts_unix: Option<u64>) {
        let now = now_unix();
        let ts = ts_unix.unwrap_or(now);
        let cutoff = now.saturating_sub(self.window.as_secs());

        let mut inner = self.inner.lock().expect("rolling window mutex poisoned");

        inner.buf.push_back((ts, score));
        while let Some(&(t, _)) = inner.buf.front() {
            if t < cutoff {
                inner.buf.pop_front();
            } else {
                break;
            }
        }
    }

    /// Return the average score and number of samples within the window.
    pub fn average_and_count(&self) -> (f32, usize) {
        let now = now_unix();
        let cutoff = now.saturating_sub(self.window.as_secs());

        let inner = self.inner.lock().expect("rolling window mutex poisoned");
        let mut sum: i64 = 0;
        let mut n: usize = 0;

        for &(t, s) in inner.buf.iter().rev() {
            if t < cutoff {
                break; // older values are at the front; can stop early
            }
            sum += s as i64;
            n += 1;
        }

        let avg = if n > 0 { sum as f32 / n as f32 } else { 0.0 };
        (avg, n)
    }

    /// Length of the window in seconds (useful for diagnostics/telemetry).
    pub fn window_secs(&self) -> u64 {
        self.window.as_secs()
    }
}

/// Current UNIX time in seconds.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}
