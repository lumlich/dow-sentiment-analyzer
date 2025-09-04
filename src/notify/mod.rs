pub mod discord;

#[derive(Debug, Clone)]
pub struct AlertPayload {
    pub decision: String,      // "BUY" | "SELL" | "HOLD"
    pub confidence: f32,       // 0.0 .. 1.0
    pub reasons: Vec<String>,  // top reasons (short)
    pub timestamp_iso: String, // e.g. UTC ISO 8601
}
