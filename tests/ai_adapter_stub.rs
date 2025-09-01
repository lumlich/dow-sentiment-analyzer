use dow_sentiment_analyzer::ai_adapter::{AiClient, AiClientDisabled as NoopAiClient};
use tokio::runtime::Runtime;

#[test]
fn noop_ai_client_returns_none() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let client = NoopAiClient;
        let res = client.analyze("Trump says Dow will soar.").await;
        assert!(res.is_none(), "Noop client must return None in Step 1");
    });
}
