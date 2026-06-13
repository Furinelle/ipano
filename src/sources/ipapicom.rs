use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    error: Option<bool>,
    reason: Option<String>,
    country_name: Option<String>,
    region: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
    asn: Option<String>,   // "AS13335"
    org: Option<String>,
}

pub struct IpApiCom { pub base: String }
impl Default for IpApiCom { fn default() -> Self { IpApiCom { base: "https://ipapi.co".into() } } }

#[async_trait]
impl Source for IpApiCom {
    fn id(&self) -> &'static str { "ipapicom" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/{}/json/", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if r.error == Some(true) { return Err(SourceError::Unavailable(r.reason.unwrap_or_default())); }
    let mut d = SourceData::new("ipapicom");
    d.country = r.country_name; d.region = r.region; d.city = r.city;
    d.lat = r.latitude; d.lon = r.longitude; d.timezone = r.timezone; d.org = r.org;
    if let Some(a) = r.asn { d.asn = a.trim_start_matches("AS").parse().ok(); }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"country_name":"Australia","region":"Queensland","city":"Brisbane","latitude":-27.46,"longitude":153.02,"timezone":"Australia/Brisbane","asn":"AS13335","org":"CLOUDFLARENET"}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipapicom");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
    }
    #[test]
    fn parse_error() { assert!(parse(r#"{"error":true,"reason":"RateLimited"}"#).is_err()); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.path("/1.1.1.1/json/"); then.status(200).body(SAMPLE); });
        let d = IpApiCom { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.asn, Some(13335));
    }
}
