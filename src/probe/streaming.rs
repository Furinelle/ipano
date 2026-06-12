use async_trait::async_trait;
use reqwest::Client;
use crate::probe::{Probe, ProbeResult, ProbeStatus};

// ===== Netflix =====
// 方法:请求一个非自制剧标题页。200=完全解锁;404=仅自制剧;403=封锁。

pub fn classify_netflix(status: u16) -> ProbeStatus {
    match status {
        200 => ProbeStatus::Unlocked,
        404 => ProbeStatus::Restricted,
        403 | 451 => ProbeStatus::Blocked,
        _ => ProbeStatus::Unknown,
    }
}

pub struct Netflix {
    pub base: String,
}
impl Default for Netflix {
    fn default() -> Self { Netflix { base: "https://www.netflix.com".to_string() } }
}

#[async_trait]
impl Probe for Netflix {
    fn name(&self) -> &'static str { "Netflix" }
    async fn check(&self, client: &Client) -> ProbeResult {
        // 81280792 = 非自制剧(《绝命毒师》),仅完整版图库可访问
        let url = format!("{}/title/81280792", self.base);
        match client.get(&url).send().await {
            Ok(resp) => ProbeResult::new(self.name(), classify_netflix(resp.status().as_u16()), None),
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ===== YouTube Premium =====
// 方法:请求 /premium 页;含 "Premium is not available" → 封锁;否则解析 countryCode。

pub fn classify_youtube(body: &str) -> ProbeResult {
    if body.contains("Premium is not available") || body.contains("不可用") {
        return ProbeResult::new("YouTube Premium", ProbeStatus::Blocked, None);
    }
    let region = extract_country_code(body);
    ProbeResult::new("YouTube Premium", ProbeStatus::Unlocked, region)
}

/// 从页面提取 "countryCode":"XX"
fn extract_country_code(body: &str) -> Option<String> {
    let key = "\"countryCode\":\"";
    let idx = body.find(key)? + key.len();
    let tail = &body[idx..];
    let cc: String = tail.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if cc.is_empty() { None } else { Some(cc) }
}

pub struct YouTube {
    pub base: String,
}
impl Default for YouTube {
    fn default() -> Self { YouTube { base: "https://www.youtube.com".to_string() } }
}

#[async_trait]
impl Probe for YouTube {
    fn name(&self) -> &'static str { "YouTube Premium" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/premium", self.base);
        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => classify_youtube(&body),
                Err(_) => ProbeResult::unknown(self.name()),
            },
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netflix_status_mapping() {
        assert_eq!(classify_netflix(200), ProbeStatus::Unlocked);
        assert_eq!(classify_netflix(404), ProbeStatus::Restricted);
        assert_eq!(classify_netflix(403), ProbeStatus::Blocked);
        assert_eq!(classify_netflix(500), ProbeStatus::Unknown);
    }

    #[test]
    fn youtube_blocked_and_region() {
        let blocked = classify_youtube("...Premium is not available in your country...");
        assert_eq!(blocked.status, ProbeStatus::Blocked);

        let ok = classify_youtube(r#"...{"countryCode":"JP","other":1}..."#);
        assert_eq!(ok.status, ProbeStatus::Unlocked);
        assert_eq!(ok.region.as_deref(), Some("JP"));
    }

    #[tokio::test]
    async fn netflix_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/title/81280792");
            then.status(200).body("ok");
        });
        let p = Netflix { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.name, "Netflix");
    }

    #[tokio::test]
    async fn youtube_check_parses_region() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/premium");
            then.status(200).body(r#"<html>{"countryCode":"US"}</html>"#);
        });
        let p = YouTube { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }
}
