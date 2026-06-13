use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp { data: Option<Data> }
#[derive(Deserialize)]
struct Data { attributes: Option<Attr> }
#[derive(Deserialize)]
struct Attr {
    as_owner: Option<String>,
    asn: Option<u32>,
    country: Option<String>,
    last_analysis_stats: Option<Stats>,
}
#[derive(Deserialize)]
struct Stats {
    harmless: Option<u32>,
    malicious: Option<u32>,
    suspicious: Option<u32>,
    undetected: Option<u32>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let attr = r.data.and_then(|d| d.attributes)
        .ok_or_else(|| SourceError::Parse("VirusTotal 响应缺 data.attributes".into()))?;
    let mut d = SourceData::new("vt");
    d.as_org = attr.as_owner;
    d.asn = attr.asn;
    d.country = attr.country;
    if let Some(s) = attr.last_analysis_stats {
        d.blacklist_harmless = s.harmless;
        d.blacklist_malicious = s.malicious;
        d.blacklist_suspicious = s.suspicious;
        d.blacklist_undetected = s.undetected;
        d.is_abuser = Some(s.malicious.unwrap_or(0) + s.suspicious.unwrap_or(0) > 0);
    }
    Ok(d)
}

pub struct VirusTotal {
    pub base: String,
    pub key: Option<String>,
}

impl Default for VirusTotal {
    fn default() -> Self {
        VirusTotal {
            base: "https://www.virustotal.com".to_string(),
            key: std::env::var("IPANO_VIRUSTOTAL_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for VirusTotal {
    fn id(&self) -> &'static str { "vt" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_VIRUSTOTAL_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_VIRUSTOTAL_KEY".to_string()))?;
        let url = format!("{}/api/v3/ip_addresses/{}", self.base, ip);
        let resp = client.get(&url)
            .header("x-apikey", key)
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

    const SAMPLE: &str = r#"{"data":{"id":"1.1.1.1","type":"ip_address","attributes":{
"as_owner":"Cloudflare, Inc.","asn":13335,"country":"AU",
"last_analysis_stats":{"harmless":80,"malicious":2,"suspicious":1,"undetected":11,"timeout":0}}}}"#;

    #[test]
    fn parse_extracts_blacklist_stats() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "vt");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.country.as_deref(), Some("AU"));
        assert_eq!(d.blacklist_harmless, Some(80));
        assert_eq!(d.blacklist_malicious, Some(2));
        assert_eq!(d.blacklist_suspicious, Some(1));
        assert_eq!(d.blacklist_undetected, Some(11));
        assert_eq!(d.is_abuser, Some(true)); // malicious+suspicious > 0
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = VirusTotal { base: "https://www.virustotal.com".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_sends_header_and_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/api/v3/ip_addresses/1.1.1.1").header("x-apikey", "secret");
            then.status(200).body(SAMPLE);
        });
        let src = VirusTotal { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.blacklist_malicious, Some(2));
    }
}
