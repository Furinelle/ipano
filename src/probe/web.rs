use async_trait::async_trait;
use reqwest::Client;
use crate::probe::{Probe, ProbeResult, ProbeStatus};
use crate::probe::unlock_util::between;

// ===== Bing =====
// GET bing.com → 200 解析 Region:"XX";cn.bing.com → cn;403/451=封锁。
pub fn parse_bing(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Bing", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        if body.contains("cn.bing.com") {
            return ProbeResult::new("Bing", ProbeStatus::Unlocked, Some("cn".into()));
        }
        let region = between(body, "Region:\"", "\"").map(|r| r.to_lowercase());
        return ProbeResult::new("Bing", ProbeStatus::Unlocked, region);
    }
    ProbeResult::unknown("Bing")
}

pub struct Bing { pub base: String }
impl Default for Bing {
    fn default() -> Self { Bing { base: "https://www.bing.com".to_string() } }
}
#[async_trait]
impl Probe for Bing {
    fn name(&self) -> &'static str { "Bing" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_bing(st, &body)
            }
            Err(_) => ProbeResult::unknown("Bing"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bing_region_parse() {
        let r = parse_bing(200, r#"x Region:"US" y"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        assert_eq!(parse_bing(403, "").status, ProbeStatus::Blocked);
    }

    #[test]
    fn bing_cn_detect() {
        let r = parse_bing(200, "redirect to cn.bing.com here");
        assert_eq!(r.region.as_deref(), Some("cn"));
    }
}
