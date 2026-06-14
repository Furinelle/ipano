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

// ===== Claude (Anthropic) =====
// 方法:GET claude.ai/ → 200=可用;403/451=地区封锁。
pub fn classify_claude(status: u16) -> ProbeStatus {
    match status {
        200 => ProbeStatus::Unlocked,
        403 | 451 => ProbeStatus::Blocked,
        _ => ProbeStatus::Unknown,
    }
}

pub struct Claude {
    pub base: String,
}
impl Default for Claude {
    fn default() -> Self { Claude { base: "https://claude.ai".to_string() } }
}

#[async_trait]
impl Probe for Claude {
    fn name(&self) -> &'static str { "Claude" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/", self.base);
        match client.get(&url).send().await {
            Ok(resp) => ProbeResult::new(self.name(), classify_claude(resp.status().as_u16()), None),
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ===== Gemini (Google) =====
// 方法:GET gemini.google.com → 200 解析地区(三码转两码);403/451=封锁。
pub fn parse_gemini(status: u16, body: &str) -> ProbeResult {
    use crate::probe::unlock_util::{between, three_to_two};
    if status == 403 || status == 451 {
        return ProbeResult::new("Gemini", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        let region = between(body, ",2,1,200,\"", "\"").map(three_to_two);
        return ProbeResult::new("Gemini", ProbeStatus::Unlocked, region);
    }
    ProbeResult::unknown("Gemini")
}

pub struct Gemini {
    pub base: String,
}
impl Default for Gemini {
    fn default() -> Self { Gemini { base: "https://gemini.google.com".to_string() } }
}

#[async_trait]
impl Probe for Gemini {
    fn name(&self) -> &'static str { "Gemini" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_gemini(status, &body)
            }
            Err(_) => ProbeResult::unknown("Gemini"),
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

    #[test]
    fn claude_status_mapping() {
        assert_eq!(classify_claude(200), ProbeStatus::Unlocked);
        assert_eq!(classify_claude(403), ProbeStatus::Blocked);
        assert_eq!(classify_claude(500), ProbeStatus::Unknown);
    }

    #[tokio::test]
    async fn claude_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| { when.path("/"); then.status(200).body("ok"); });
        let p = Claude { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.name, "Claude");
    }

    #[tokio::test]
    async fn gemini_blocked_403() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| { when.path("/"); then.status(403).body("no"); });
        let p = Gemini { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[tokio::test]
    async fn gemini_unlocked_with_region() {
        let server = httpmock::MockServer::start();
        let body = r#"window.WIZ=[null,2,1,200,"USA"];"#;
        let m = server.mock(|when, then| { when.path("/"); then.status(200).body(body); });
        let p = Gemini { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }
}
