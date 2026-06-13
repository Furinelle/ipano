use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    company: Option<Company>,
    connection: Option<Connection>,
    location: Option<Location>,
    security: Option<Security>,
}

#[derive(Deserialize)]
struct Company {
    #[serde(rename = "type")]
    ctype: Option<String>,
}

#[derive(Deserialize)]
struct Connection {
    asn: Option<u32>,
    organization: Option<String>,
}

#[derive(Deserialize)]
struct Location {
    country: Option<CodeName>,
    region: Option<CodeName>,
    city: Option<String>,
}

#[derive(Deserialize)]
struct CodeName {
    code: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct Security {
    is_proxy: Option<bool>,
    is_vpn: Option<bool>,
    is_tor: Option<bool>,
    is_abuser: Option<bool>,
    is_bogon: Option<bool>,
    is_relay: Option<bool>,
    is_anonymous: Option<bool>,
    is_cloud_provider: Option<bool>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipreg");
    if let Some(c) = r.company { d.company_type = c.ctype; }
    if let Some(c) = r.connection { d.asn = c.asn; d.as_org = c.organization; }
    if let Some(l) = r.location {
        d.country = l.country.and_then(|x| x.code);
        d.region = l.region.and_then(|x| x.name);
        d.city = l.city;
    }
    if let Some(s) = r.security {
        d.is_proxy = s.is_proxy;
        d.is_vpn = s.is_vpn;
        d.is_tor = s.is_tor;
        d.is_abuser = s.is_abuser;
        d.is_bogon = s.is_bogon;
        d.is_relay = s.is_relay;
        d.is_anonymous = s.is_anonymous;
        d.is_cloud = s.is_cloud_provider;
    }
    Ok(d)
}

pub struct IpRegistry {
    pub base: String,
    pub key: Option<String>,
}

impl Default for IpRegistry {
    fn default() -> Self {
        IpRegistry {
            base: "https://api.ipregistry.co".to_string(),
            key: std::env::var("IPANO_IPREGISTRY_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for IpRegistry {
    fn id(&self) -> &'static str { "ipreg" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_IPREGISTRY_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_IPREGISTRY_KEY".to_string()))?;
        let url = format!("{}/{}?key={}", self.base, ip, key);
        let resp = client.get(&url)
            .send().await
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

    const SAMPLE: &str = r#"{"company":{"domain":"apnic.net","name":"Apnic R&D","type":"hosting"},
"connection":{"asn":13335,"domain":"cloudflare.com","organization":"Cloudflare, Inc.","route":"1.1.1.0/24","type":"hosting"},
"ip":"1.1.1.1","location":{"country":{"code":"AU","name":"Australia"},"region":{"code":"AU-QLD","name":"Queensland"},"city":"Brisbane","latitude":-27.46798,"longitude":153.02809},
"security":{"is_abuser":false,"is_attacker":false,"is_bogon":false,"is_cloud_provider":true,"is_proxy":false,"is_relay":false,"is_tor":false,"is_tor_exit":false,"is_vpn":false,"is_anonymous":false,"is_threat":false},
"time_zone":{"id":"Australia/Brisbane"},"type":"IPv4"}"#;

    #[test]
    fn parse_extracts_security_flags() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipreg");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.country.as_deref(), Some("AU"));
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
        assert_eq!(d.company_type.as_deref(), Some("hosting"));
        assert_eq!(d.is_cloud, Some(true));
        assert_eq!(d.is_relay, Some(false));
        assert_eq!(d.is_anonymous, Some(false));
        assert_eq!(d.is_bogon, Some(false));
        assert_eq!(d.is_proxy, Some(false));
        assert_eq!(d.is_abuser, Some(false));
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = IpRegistry { base: "https://api.ipregistry.co".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| { when.path("/1.1.1.1"); then.status(200).body(SAMPLE); });
        let src = IpRegistry { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.is_cloud, Some(true));
    }
}
