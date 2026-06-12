use async_trait::async_trait;
use reqwest::Client;
use crate::probe::{Probe, ProbeResult, ProbeStatus};

// ===== ChatGPT (OpenAI) =====
// 方法:请求 OpenAI 合规端点。200=该地区可用;403=受限地区封锁。

pub fn classify_openai(status: u16) -> ProbeStatus {
    match status {
        200 => ProbeStatus::Unlocked,
        403 | 451 => ProbeStatus::Blocked,
        _ => ProbeStatus::Unknown,
    }
}

pub struct ChatGpt {
    pub base: String,
}
impl Default for ChatGpt {
    fn default() -> Self { ChatGpt { base: "https://api.openai.com".to_string() } }
}

#[async_trait]
impl Probe for ChatGpt {
    fn name(&self) -> &'static str { "ChatGPT" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/compliance/cookie_settings", self.base);
        match client.get(&url).send().await {
            Ok(resp) => ProbeResult::new(self.name(), classify_openai(resp.status().as_u16()), None),
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_status_mapping() {
        assert_eq!(classify_openai(200), ProbeStatus::Unlocked);
        assert_eq!(classify_openai(403), ProbeStatus::Blocked);
        assert_eq!(classify_openai(500), ProbeStatus::Unknown);
    }

    #[tokio::test]
    async fn chatgpt_check_blocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/compliance/cookie_settings");
            then.status(403).body("unsupported_country");
        });
        let p = ChatGpt { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Blocked);
        assert_eq!(r.name, "ChatGPT");
    }
}
