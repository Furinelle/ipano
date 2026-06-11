use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    data: Option<Data>,
}

#[derive(Deserialize)]
struct Data {
    #[serde(rename = "abuseConfidenceScore")]
    abuse_confidence_score: Option<i64>,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
    isp: Option<String>,
    #[serde(rename = "totalReports")]
    total_reports: Option<i64>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let data = r.data.ok_or_else(|| SourceError::Parse("AbuseIPDB 响应缺 data".into()))?;
    let mut d = SourceData::new("abuseipdb");
    d.abuseipdb_score = data.abuse_confidence_score;
    d.country = data.country_code;
    d.isp = data.isp;
    d.is_abuser = data.total_reports.map(|n| n > 0);
    Ok(d)
}

pub struct AbuseIpdb {
    pub base: String,
    pub key: Option<String>,
}

impl Default for AbuseIpdb {
    fn default() -> Self {
        AbuseIpdb {
            base: "https://api.abuseipdb.com".to_string(),
            key: std::env::var("IPANO_ABUSEIPDB_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for AbuseIpdb {
    fn id(&self) -> &'static str { "abuseipdb" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_ABUSEIPDB_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_ABUSEIPDB_KEY".to_string()))?;
        let url = format!("{}/api/v2/check?ipAddress={}&maxAgeInDays=90", self.base, ip);
        let resp = client.get(&url)
            .header("Key", key)
            .header(reqwest::header::ACCEPT, "application/json")
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

    const SAMPLE: &str = r#"{"data":{"ipAddress":"1.1.1.1","isPublic":true,
        "abuseConfidenceScore":17,"countryCode":"AU","usageType":"Content Delivery Network",
        "isp":"Cloudflare, Inc.","domain":"cloudflare.com","totalReports":3,"numDistinctUsers":2}}"#;

    #[test]
    fn parse_extracts_confidence() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "abuseipdb");
        assert_eq!(d.abuseipdb_score, Some(17));
        assert_eq!(d.country.as_deref(), Some("AU"));
        assert_eq!(d.isp.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.is_abuser, Some(true)); // totalReports=3 > 0
    }

    #[test]
    fn parse_no_reports_not_abuser() {
        let body = r#"{"data":{"abuseConfidenceScore":0,"totalReports":0}}"#;
        let d = parse(body).unwrap();
        assert_eq!(d.abuseipdb_score, Some(0));
        assert_eq!(d.is_abuser, Some(false));
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = AbuseIpdb { base: "https://api.abuseipdb.com".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_sends_header_and_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/api/v2/check").header("key", "secret");
            then.status(200).body(SAMPLE);
        });
        let src = AbuseIpdb { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.abuseipdb_score, Some(17));
    }
}
