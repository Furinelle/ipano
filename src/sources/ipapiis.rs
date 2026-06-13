use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    is_datacenter: Option<bool>,
    is_vpn: Option<bool>,
    is_proxy: Option<bool>,
    is_abuser: Option<bool>,
    asn: Option<Asn>,
    company: Option<Company>,
}
#[derive(Deserialize)]
struct Asn { asn: Option<u32>, abuser_score: Option<String>, org: Option<String> }
#[derive(Deserialize)]
struct Company { #[serde(rename = "type")] ctype: Option<String>, abuser_score: Option<String> }

/// "0.0131 (Elevated)" → 0.0131
fn lead_f64(s: &str) -> Option<f64> { s.trim().split_whitespace().next()?.parse().ok() }

pub struct IpApiIs { pub base: String }
impl Default for IpApiIs { fn default() -> Self { IpApiIs { base: "https://api.ipapi.is".into() } } }

#[async_trait]
impl Source for IpApiIs {
    fn id(&self) -> &'static str { "ipapiis" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/?q={}", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipapiis");
    d.is_datacenter = r.is_datacenter; d.is_vpn = r.is_vpn; d.is_proxy = r.is_proxy; d.is_abuser = r.is_abuser;
    if let Some(a) = r.asn { d.asn = a.asn; d.as_org = a.org; d.asn_abuse_score = a.abuser_score.as_deref().and_then(lead_f64); }
    if let Some(c) = r.company { d.company_type = c.ctype; d.company_abuse_score = c.abuser_score.as_deref().and_then(lead_f64); }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"is_datacenter":true,"is_vpn":false,"is_proxy":false,"is_abuser":false,"asn":{"asn":13335,"abuser_score":"0.0131 (Elevated)","org":"Cloudflare"},"company":{"type":"hosting","abuser_score":"0.015 (Elevated)"}}"#;
    #[test]
    fn parse_extracts_abuse_scores() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipapiis");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.asn_abuse_score, Some(0.0131));
        assert_eq!(d.company_abuse_score, Some(0.015));
        assert_eq!(d.company_type.as_deref(), Some("hosting"));
        assert_eq!(d.is_datacenter, Some(true));
    }
    #[test]
    fn lead_f64_parses() { assert_eq!(lead_f64("0.0131 (Elevated)"), Some(0.0131)); assert_eq!(lead_f64("x"), None); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.query_param("q", "1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = IpApiIs { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.asn_abuse_score, Some(0.0131));
    }
}
