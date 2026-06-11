use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    success: Option<bool>,
    message: Option<String>,
    fraud_score: Option<i64>,
    country_code: Option<String>,
    region: Option<String>,
    city: Option<String>,
    #[serde(rename = "ISP")]
    isp: Option<String>,
    #[serde(rename = "ASN")]
    asn: Option<u32>,
    organization: Option<String>,
    proxy: Option<bool>,
    vpn: Option<bool>,
    tor: Option<bool>,
    is_crawler: Option<bool>,
    mobile: Option<bool>,
    recent_abuse: Option<bool>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if r.success == Some(false) {
        return Err(SourceError::Unavailable(r.message.unwrap_or_else(|| "IPQS 失败".into())));
    }
    let mut d = SourceData::new("ipqs");
    d.ipqs_score = r.fraud_score;
    d.country = r.country_code;
    d.region = r.region;
    d.city = r.city;
    d.isp = r.isp;
    d.asn = r.asn;
    d.org = r.organization;
    d.is_proxy = r.proxy;
    d.is_vpn = r.vpn;
    d.is_tor = r.tor;
    d.is_crawler = r.is_crawler;
    d.is_mobile = r.mobile;
    d.is_abuser = r.recent_abuse;
    Ok(d)
}

pub struct Ipqs {
    pub base: String,
    pub key: Option<String>,
}

impl Default for Ipqs {
    fn default() -> Self {
        Ipqs {
            base: "https://ipqualityscore.com".to_string(),
            key: std::env::var("IPANO_IPQS_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for Ipqs {
    fn id(&self) -> &'static str { "ipqs" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_IPQS_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_IPQS_KEY".to_string()))?;
        let url = format!("{}/api/json/ip/{}/{}", self.base, key, ip);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"success":true,"message":"Success","fraud_score":75,
        "country_code":"US","region":"California","city":"Los Angeles","ISP":"Cloudflare",
        "ASN":13335,"organization":"Cloudflare, Inc.","proxy":true,"vpn":true,"tor":false,
        "is_crawler":false,"mobile":false,"recent_abuse":true}"#;

    #[test]
    fn parse_extracts_fraud_and_flags() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipqs");
        assert_eq!(d.ipqs_score, Some(75));
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.is_proxy, Some(true));
        assert_eq!(d.is_vpn, Some(true));
        assert_eq!(d.is_tor, Some(false));
        assert_eq!(d.is_abuser, Some(true)); // recent_abuse
    }

    #[test]
    fn parse_failure_is_err() {
        let body = r#"{"success":false,"message":"Invalid or expired key."}"#;
        assert!(parse(body).is_err());
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = Ipqs { base: "https://ipqualityscore.com".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_in_path_and_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/api/json/ip/secret/1.1.1.1");
            then.status(200).body(SAMPLE);
        });
        let src = Ipqs { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.ipqs_score, Some(75));
    }
}
