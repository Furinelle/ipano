use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    error: Option<serde_json::Value>,
    country_name: Option<String>,
    region_name: Option<String>,
    city_name: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    time_zone: Option<String>,
    #[serde(rename = "as")] as_name: Option<String>,
    asn: Option<String>,
    is_proxy: Option<bool>,
    usage_type: Option<String>,
}

pub struct Ip2Location { pub base: String }
impl Default for Ip2Location { fn default() -> Self { Ip2Location { base: "https://api.ip2location.io".into() } } }

#[async_trait]
impl Source for Ip2Location {
    fn id(&self) -> &'static str { "ip2loc" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/?ip={}", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if r.error.is_some() { return Err(SourceError::Unavailable("ip2location error".into())); }
    let mut d = SourceData::new("ip2loc");
    d.country = r.country_name; d.region = r.region_name; d.city = r.city_name;
    d.lat = r.latitude; d.lon = r.longitude; d.timezone = r.time_zone;
    d.is_proxy = r.is_proxy; d.usage_type = r.usage_type;
    d.asn = r.asn.and_then(|s| s.parse().ok());
    if let Some(a) = r.as_name { d.as_org = Some(a); }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"country_name":"Australia","region_name":"Queensland","city_name":"Brisbane","latitude":-27.46,"longitude":153.02,"time_zone":"+10:00","asn":"13335","as":"Cloudflare Inc","is_proxy":false,"usage_type":"DCH"}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ip2loc");
        assert_eq!(d.usage_type.as_deref(), Some("DCH"));
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.is_proxy, Some(false));
    }
    #[test]
    fn parse_error() { assert!(parse(r#"{"error":{"error_code":10001}}"#).is_err()); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.query_param("ip", "1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = Ip2Location { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.usage_type.as_deref(), Some("DCH"));
    }
}
