use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct AsnResp { result: Option<AsnResult> }
#[derive(Deserialize)]
struct AsnResult { asn: Option<AsnInner> }
#[derive(Deserialize)]
struct AsnInner { asn: Option<u32> }

#[derive(Deserialize)]
struct BotResp { result: Option<BotResult> }
#[derive(Deserialize)]
struct BotResult { summary_0: Option<BotSummary> }
#[derive(Deserialize)]
struct BotSummary { bot: Option<String>, human: Option<String> }

#[derive(Deserialize)]
struct DevResp { result: Option<DevResult> }
#[derive(Deserialize)]
struct DevResult { summary_0: Option<std::collections::BTreeMap<String, String>> }

pub fn parse_asn(body: &str) -> Result<u32, SourceError> {
    let r: AsnResp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    r.result.and_then(|x| x.asn).and_then(|x| x.asn)
        .ok_or_else(|| SourceError::Parse("CF: 无法从响应解析 ASN".into()))
}

/// 返回 (human_pct, bot_pct)
pub fn parse_bot(body: &str) -> Result<(f64, f64), SourceError> {
    let r: BotResp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let s = r.result.and_then(|x| x.summary_0)
        .ok_or_else(|| SourceError::Parse("CF: bot_class 缺 summary_0".into()))?;
    let h = s.human.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let b = s.bot.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    Ok((h, b))
}

/// "desktop 70.0% mobile 28.0% other 2.0%"
pub fn parse_device(body: &str) -> Option<String> {
    let r: DevResp = serde_json::from_str(body).ok()?;
    let m = r.result?.summary_0?;
    let parts: Vec<String> = m.iter().map(|(k, v)| format!("{k} {v}%")).collect();
    if parts.is_empty() { None } else { Some(parts.join(" ")) }
}

pub fn build_data(traffic: Option<(f64, f64)>, device_dist: Option<String>) -> SourceData {
    let mut d = SourceData::new("cf");
    if let Some((h, b)) = traffic { d.human_traffic_pct = Some(h); d.bot_traffic_pct = Some(b); }
    d.device_dist = device_dist;
    d
}

pub struct Cloudflare { pub base: String, pub key: Option<String> }
impl Default for Cloudflare {
    fn default() -> Self {
        Cloudflare {
            base: "https://api.cloudflare.com/client/v4".to_string(),
            key: std::env::var("IPANO_CF_TOKEN").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for Cloudflare {
    fn id(&self) -> &'static str { "cf" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_CF_TOKEN") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(|| SourceError::NeedsKey("IPANO_CF_TOKEN".to_string()))?;
        let bearer = format!("Bearer {key}");
        let get = |url: String| {
            let c = client.clone(); let b = bearer.clone();
            async move {
                c.get(&url).header(reqwest::header::AUTHORIZATION, b).send().await
                    .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
                    .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))
            }
        };
        let asn_body = get(format!("{}/radar/entities/asns/ip?ip={}", self.base, ip)).await?;
        let asn = parse_asn(&asn_body)?;
        let traffic = get(format!("{}/radar/http/summary/bot_class?asn={}&dateRange=7d", self.base, asn))
            .await.ok().and_then(|b| parse_bot(&b).ok());
        let device = get(format!("{}/radar/http/summary/device_type?asn={}&dateRange=7d", self.base, asn))
            .await.ok().and_then(|b| parse_device(&b));
        if traffic.is_none() && device.is_none() {
            return Err(SourceError::Unavailable("CF: 无可用 Radar 聚合数据".into()));
        }
        Ok(build_data(traffic, device))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE_ASN: &str = r#"{"result":{"asn":{"asn":13335,"name":"CLOUDFLARENET"}},"success":true}"#;
    const SAMPLE_BOT: &str = r#"{"result":{"summary_0":{"bot":"21.5","human":"78.5"}},"success":true}"#;
    const SAMPLE_DEV: &str = r#"{"result":{"summary_0":{"desktop":"70.0","mobile":"28.0","other":"2.0"}},"success":true}"#;

    #[test]
    fn parse_asn_extracts_number() {
        assert_eq!(parse_asn(SAMPLE_ASN).unwrap(), 13335);
    }

    #[test]
    fn parse_bot_pcts() {
        let (h, b) = parse_bot(SAMPLE_BOT).unwrap();
        assert_eq!(h, 78.5);
        assert_eq!(b, 21.5);
    }

    #[test]
    fn build_data_merges_summaries() {
        let d = build_data(Some((78.5, 21.5)), Some("desktop 70.0% mobile 28.0% other 2.0%".into()));
        assert_eq!(d.source_id, "cf");
        assert_eq!(d.human_traffic_pct, Some(78.5));
        assert_eq!(d.bot_traffic_pct, Some(21.5));
        assert_eq!(d.device_dist.as_deref(), Some("desktop 70.0% mobile 28.0% other 2.0%"));
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = Cloudflare { base: "https://api.cloudflare.com/client/v4".into(), key: None };
        let err = src.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_chains_asn_then_summaries() {
        let server = httpmock::MockServer::start();
        server.mock(|w, t| { w.path("/radar/entities/asns/ip"); t.status(200).body(SAMPLE_ASN); });
        server.mock(|w, t| { w.path("/radar/http/summary/bot_class"); t.status(200).body(SAMPLE_BOT); });
        server.mock(|w, t| { w.path("/radar/http/summary/device_type"); t.status(200).body(SAMPLE_DEV); });
        let src = Cloudflare { base: server.base_url(), key: Some("secret".into()) };
        let d = src.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.human_traffic_pct, Some(78.5));
        assert!(d.device_dist.is_some());
    }
}
