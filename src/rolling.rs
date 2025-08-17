//! rolling.rs — Jednoduché klouzavé okno pro informativní metriky (48h).
//! Sběr score s časovou značkou a výpočet průměru/počtu za poslední okno.
//! Neřeší notifikace; ty přijdou v disruption detektoru.

use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub struct RollingWindow {
    inner: Mutex<Inner>,
    window: Duration,
}

#[derive(Debug)]
struct Inner {
    // Ukládáme (unix_seconds, score)
    buf: VecDeque<(u64, i32)>,
}

impl RollingWindow {
    /// Vytvoří okno o zadané délce (např. 48h).
    pub fn with_window(window: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner {
                buf: VecDeque::new(),
            }),
            window,
        }
    }

    /// Pohodlný konstruktor pro 48h.
    pub fn new_48h() -> Self {
        Self::with_window(Duration::from_secs(48 * 3600))
    }

    /// Přidá pozorování. Pokud `ts_unix` není zadán, použije "teď".
    pub fn record(&self, score: i32, ts_unix: Option<u64>) {
        let now = now_unix();
        let ts = ts_unix.unwrap_or(now);
        let cutoff = now.saturating_sub(self.window.as_secs());

        let mut inner = self.inner.lock().expect("rolling window mutex poisoned");

        // udržujme velikost a čistíme staré
        inner.buf.push_back((ts, score));
        while let Some(&(t, _)) = inner.buf.front() {
            if t < cutoff {
                inner.buf.pop_front();
            } else {
                break;
            }
        }
    }

    /// Vrátí průměrné score a počet vzorků v okně.
    pub fn average_and_count(&self) -> (f32, usize) {
        let now = now_unix();
        let cutoff = now.saturating_sub(self.window.as_secs());

        let inner = self.inner.lock().expect("rolling window mutex poisoned");
        let mut sum: i64 = 0;
        let mut n: usize = 0;

        for &(t, s) in inner.buf.iter().rev() {
            if t < cutoff {
                break; // starší hodnoty jsou na začátku; můžeme skončit
            }
            sum += s as i64;
            n += 1;
        }

        let avg = if n > 0 { sum as f32 / n as f32 } else { 0.0 };
        (avg, n)
    }

    /// Vratná délka okna v sekundách (pro debug/telemetrii).
    pub fn window_secs(&self) -> u64 {
        self.window.as_secs()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}
