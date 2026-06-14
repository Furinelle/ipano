use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp { scamalytics: Option<Inner> }

#[derive(Deserialize)]
struct Inner {
    scamalytics_score: Option<i64>,
    scamalytics_risk: Option<String>,
    scamalytics_proxy: Option<Proxy>,
}

#[derive(Deserialize)]
struct Proxy {
    is_vpn: Option<bool>,
    is_tor: Option<bool>,
    is_datacenter: Option<bool>,
    is_anonymous: Option<bool>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let s = r.scamalytics.ok_or_else(|| SourceError::Parse("scamalytics 响应缺 scamalytics".into()))?;
    let mut d = SourceData::new("scam");
    d.fraud_score = s.scamalytics_score;
    d.threat_level = s.scamalytics_risk;
    if let Some(p) = s.scamalytics_proxy {
        d.is_vpn = p.is_vpn;
        d.is_tor = p.is_tor;
        d.is_datacenter = p.is_datacenter;
        d.is_anonymous = p.is_anonymous;
    }
    Ok(d)
}

pub struct Scamalytics {
    pub base: String,
    pub user: Option<String>,
    pub key: Option<String>,
}

impl Default for Scamalytics {
    fn default() -> Self {
        Scamalytics {
            base: std::env::var("IPANO_SCAMALYTICS_BASE").ok().filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api12.scamalytics.com".to_string()),
            user: std::env::var("IPANO_SCAMALYTICS_USER").ok().filter(|s| !s.is_empty()),
            key: std::env::var("IPANO_SCAMALYTICS_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for Scamalytics {
    fn id(&self) -> &'static str { "scam" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_SCAMALYTICS_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(|| SourceError::NeedsKey("IPANO_SCAMALYTICS_KEY".to_string()))?;
        let user = self.user.as_ref().ok_or_else(|| SourceError::NeedsKey("IPANO_SCAMALYTICS_USER".to_string()))?;
        let url = format!("{}/{}/?key={}&ip={}", self.base, user, key, ip);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"scamalytics":{"status":"ok","scamalytics_score":18,"scamalytics_risk":"low",
"scamalytics_proxy":{"is_vpn":false,"is_tor":false,"is_datacenter":true,"is_anonymous":false}}}"#;

    #[test]
    fn parse_extracts_score_and_risk() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "scam");
        assert_eq!(d.fraud_score, Some(18));
        assert_eq!(d.threat_level.as_deref(), Some("low"));
        assert_eq!(d.is_datacenter, Some(true));
        assert_eq!(d.is_vpn, Some(false));
        assert_eq!(d.is_tor, Some(false));
        assert_eq!(d.is_anonymous, Some(false));
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = Scamalytics { base: "https://api12.scamalytics.com".into(), user: Some("u".into()), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn no_user_yields_needs_key() {
        let src = Scamalytics { base: "https://api12.scamalytics.com".into(), user: None, key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/u/").query_param("ip", "1.1.1.1").query_param("key", "secret");
            then.status(200).body(SAMPLE);
        });
        let src = Scamalytics { base: server.base_url(), user: Some("u".into()), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.fraud_score, Some(18));
    }
}
