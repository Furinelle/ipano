use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::probe::{Probe, ProbeResult, ProbeStatus};

// ===== Netflix CDN =====
// GET api.fast.com/netflix/speedtest/v2 → JSON targets[0].location.country。
// 403/451 = IP 被 Netflix 封禁。token 为 fast.com 公开测速 token。
#[derive(Deserialize)]
struct FastLocation { country: Option<String> }
#[derive(Deserialize)]
struct FastTarget { location: Option<FastLocation> }
#[derive(Deserialize)]
struct FastResp { targets: Option<Vec<FastTarget>> }

pub fn parse_netflix_cdn(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Netflix CDN", ProbeStatus::Blocked, None).with_info("IP Banned By Netflix");
    }
    if status == 200 {
        if let Ok(r) = serde_json::from_str::<FastResp>(body) {
            if let Some(country) = r.targets.and_then(|t| t.into_iter().next())
                .and_then(|t| t.location).and_then(|l| l.country) {
                if !country.is_empty() {
                    return ProbeResult::new("Netflix CDN", ProbeStatus::Unlocked, Some(country.to_lowercase()));
                }
            }
        }
    }
    ProbeResult::unknown("Netflix CDN")
}

pub struct NetflixCdn { pub base: String }
impl Default for NetflixCdn {
    fn default() -> Self { NetflixCdn { base: "https://api.fast.com".to_string() } }
}
#[async_trait]
impl Probe for NetflixCdn {
    fn name(&self) -> &'static str { "Netflix CDN" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/netflix/speedtest/v2?https=true&token=YXNkZmFzZGxmbnNkYWZoYXNkZmhrYWxm&urlCount=5", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_netflix_cdn(st, &body)
            }
            Err(_) => ProbeResult::unknown("Netflix CDN"),
        }
    }
}

// ===== YouTube CDN =====
// GET redirector.googlevideo.com/report_mapping → 文本响应,含落地 CDN 节点信息。
// 简化:200+非空 = 可达(info 给 CDN 描述);403/451 = 封锁;空/异常 = Unknown。
// 注:report_mapping 文本格式随时变动,以实跑为准(见 Task 7)。
pub fn parse_youtube_cdn(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("YouTube CDN", ProbeStatus::Blocked, None);
    }
    if status == 200 && !body.trim().is_empty() {
        let info = if body.contains("=>") { "GGC / Video Server" } else { "Reachable" };
        return ProbeResult::new("YouTube CDN", ProbeStatus::Unlocked, None).with_info(info);
    }
    ProbeResult::unknown("YouTube CDN")
}

pub struct YoutubeCdn { pub base: String }
impl Default for YoutubeCdn {
    fn default() -> Self { YoutubeCdn { base: "https://redirector.googlevideo.com".to_string() } }
}
#[async_trait]
impl Probe for YoutubeCdn {
    fn name(&self) -> &'static str { "YouTube CDN" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/report_mapping", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_youtube_cdn(st, &body)
            }
            Err(_) => ProbeResult::unknown("YouTube CDN"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netflix_cdn_parse() {
        let body = r#"{"targets":[{"location":{"city":"LA","country":"US"}}]}"#;
        let r = parse_netflix_cdn(200, body);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        let b = parse_netflix_cdn(403, "");
        assert_eq!(b.status, ProbeStatus::Blocked);
        assert_eq!(b.info.as_deref(), Some("IP Banned By Netflix"));
        assert_eq!(parse_netflix_cdn(200, "{}").status, ProbeStatus::Unknown);
    }

    #[test]
    fn youtube_cdn_parse() {
        let r = parse_youtube_cdn(200, "router 1.2.3.4 => sault.<...>.googlevideo.com");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert!(r.info.is_some());
        assert_eq!(parse_youtube_cdn(403, "").status, ProbeStatus::Blocked);
        assert_eq!(parse_youtube_cdn(200, "").status, ProbeStatus::Unknown);
    }
}
