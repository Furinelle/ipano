use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    country: Option<String>,
    city: Option<String>,
    asn: Option<u32>,
    security: Option<Sec>,
    connection: Option<Conn>,
}

#[derive(Deserialize)]
struct Sec {
    vpn: Option<bool>,
    proxy: Option<bool>,
    tor: Option<bool>,
    threat: Option<bool>,
}

#[derive(Deserialize)]
struct Conn {
    #[serde(rename = "type")]
    ctype: Option<String>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("dkly");
    d.country = r.country;
    d.city = r.city;
    d.asn = r.asn;
    if let Some(s) = r.security {
        d.is_vpn = s.vpn;
        d.is_proxy = s.proxy;
        d.is_tor = s.tor;
        if s.threat == Some(true) { d.is_abuser = Some(true); }
    }
    if let Some(c) = r.connection { d.company_type = c.ctype; }
    Ok(d)
}

pub struct Dkly {
    pub base: String,
    pub key: Option<String>,
}

impl Default for Dkly {
    fn default() -> Self {
        Dkly {
            base: "https://ipinfo.dkly.net".to_string(),
            key: std::env::var("IPANO_DKLY_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for Dkly {
    fn id(&self) -> &'static str { "dkly" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_DKLY_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_DKLY_KEY".to_string()))?;
        let url = format!("{}/api/?key={}&ip={}", self.base, key, ip);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"country":"AU","city":"Brisbane","asn":13335,
"security":{"vpn":false,"proxy":false,"tor":false,"threat":false},
"connection":{"type":"hosting"}}"#;

    #[test]
    fn parse_extracts_geo_and_security() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "dkly");
        assert_eq!(d.country.as_deref(), Some("AU"));
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.is_vpn, Some(false));
        assert_eq!(d.is_proxy, Some(false));
        assert_eq!(d.is_tor, Some(false));
        assert_eq!(d.company_type.as_deref(), Some("hosting"));
    }

    #[test]
    fn parse_threat_sets_abuser() {
        let body = r#"{"security":{"threat":true}}"#;
        let d = parse(body).unwrap();
        assert_eq!(d.is_abuser, Some(true));
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = Dkly { base: "https://ipinfo.dkly.net".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/api/").query_param("key", "secret").query_param("ip", "1.1.1.1");
            then.status(200).body(SAMPLE);
        });
        let src = Dkly { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.asn, Some(13335));
    }
}
