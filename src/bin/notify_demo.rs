//! Demo that simulates a few events through the multiplexer (stdout/log only when channels disabled).

use chrono::Utc;
use dow_sentiment_analyzer::{DecisionKind, NotificationEvent, NotifierMux};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();
    let mux = NotifierMux::from_env();

    let seq = [
        DecisionKind::HOLD,
        DecisionKind::SELL,
        DecisionKind::HOLD,
        DecisionKind::BUY,
    ];

    for k in seq {
        let ev = NotificationEvent {
            decision: k,
            confidence: 0.66,
            reasons: vec!["demo reason".into()],
            ts: Utc::now(),
        };
        mux.notify(&ev).await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }

    println!("notify-demo done");
}
