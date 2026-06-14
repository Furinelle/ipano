use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    country_code: Option<String>,
    region: Option<String>,
    city: Option<String>,
    asn: Option<Asn>,
    threat: Option<Threat>,
}

#[derive(Deserialize)]
struct Asn {
    asn: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct Threat {
    is_tor: Option<bool>,
    is_icloud_relay: Option<bool>,
    is_proxy: Option<bool>,
    is_datacenter: Option<bool>,
    is_anonymous: Option<bool>,
    is_bogon: Option<bool>,
    is_known_attacker: Option<bool>,
    is_known_abuser: Option<bool>,
    is_threat: Option<bool>,
}

/// "AS13335" → 13335
fn asn_num(s: &str) -> Option<u32> {
    s.trim_start_matches("AS").parse().ok()
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipdata");
    d.country = r.country_code;
    d.region = r.region;
    d.city = r.city;
    if let Some(a) = r.asn {
        d.asn = a.asn.as_deref().and_then(asn_num);
        d.as_org = a.name;
    }
    if let Some(t) = r.threat {
        d.is_tor = t.is_tor;
        d.is_proxy = t.is_proxy;
        d.is_datacenter = t.is_datacenter;
        d.is_anonymous = t.is_anonymous;
        d.is_bogon = t.is_bogon;
        d.is_relay = t.is_icloud_relay;
        d.is_abuser = Some(t.is_known_abuser.unwrap_or(false) || t.is_known_attacker.unwrap_or(false));
        if t.is_threat == Some(true) {
            d.threat_level = Some("high".into());
        }
    }
    Ok(d)
}

pub struct IpData {
    pub base: String,
    pub key: Option<String>,
}

impl Default for IpData {
    fn default() -> Self {
        IpData {
            base: "https://api.ipdata.co".to_string(),
            key: std::env::var("IPANO_IPDATA_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for IpData {
    fn id(&self) -> &'static str { "ipdata" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_IPDATA_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_IPDATA_KEY".to_string()))?;
        let url = format!("{}/{}?api-key={}", self.base, ip, key);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 {
            return Err(SourceError::RateLimited);
        }
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"ip":"1.1.1.1","country_code":"AU","region":"Queensland","city":"Brisbane",
"asn":{"asn":"AS13335","name":"Cloudflare, Inc."},
"threat":{"is_tor":false,"is_icloud_relay":false,"is_proxy":false,"is_datacenter":true,
"is_anonymous":false,"is_known_attacker":false,"is_known_abuser":false,"is_threat":false,"is_bogon":false,
"blocklists":[]}}"#;

    #[test]
    fn parse_extracts_threat_flags() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipdata");
        assert_eq!(d.country.as_deref(), Some("AU"));
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.is_datacenter, Some(true));
        assert_eq!(d.is_tor, Some(false));
        assert_eq!(d.is_relay, Some(false)); // is_icloud_relay
        assert_eq!(d.is_anonymous, Some(false));
        assert_eq!(d.is_bogon, Some(false));
        assert_eq!(d.is_abuser, Some(false)); // is_known_abuser || is_known_attacker
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = IpData { base: "https://api.ipdata.co".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/1.1.1.1").query_param("api-key", "secret");
            then.status(200).body(SAMPLE);
        });
        let src = IpData { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.is_datacenter, Some(true));
    }
}
