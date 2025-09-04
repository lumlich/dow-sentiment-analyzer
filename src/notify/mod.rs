pub mod discord;
pub mod slack;

#[derive(Debug, Clone)]
pub struct AlertPayload {
    pub decision: String,      // "BUY" | "SELL" | "HOLD" | "TEST"
    pub confidence: f32,       // 0.0 .. 1.0
    pub reasons: Vec<String>,  // short reasons
    pub timestamp_iso: String, // UTC ISO 8601
}
