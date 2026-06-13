use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    success: bool,
    message: Option<String>,
    country: Option<String>,
    region: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    connection: Option<Conn>,
    timezone: Option<Tz>,
}
#[derive(Deserialize)]
struct Conn { asn: Option<u32>, isp: Option<String>, org: Option<String> }
#[derive(Deserialize)]
struct Tz { id: Option<String> }

pub struct IpWhois { pub base: String }
impl Default for IpWhois { fn default() -> Self { IpWhois { base: "http://ipwho.is".into() } } }

#[async_trait]
impl Source for IpWhois {
    fn id(&self) -> &'static str { "ipwhois" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/{}", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if !r.success { return Err(SourceError::Unavailable(r.message.unwrap_or_default())); }
    let mut d = SourceData::new("ipwhois");
    d.country = r.country; d.region = r.region; d.city = r.city;
    d.lat = r.latitude; d.lon = r.longitude;
    d.timezone = r.timezone.and_then(|t| t.id);
    if let Some(c) = r.connection { d.asn = c.asn; d.isp = c.isp; d.org = c.org; }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"success":true,"country":"United States","region":"California","city":"Los Angeles","latitude":34.05,"longitude":-118.24,"connection":{"asn":13335,"isp":"Cloudflare","org":"Cloudflare Inc"},"timezone":{"id":"America/Los_Angeles"}}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipwhois");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.city.as_deref(), Some("Los Angeles"));
        assert_eq!(d.timezone.as_deref(), Some("America/Los_Angeles"));
    }
    #[test]
    fn parse_fail() {
        assert!(parse(r#"{"success":false,"message":"reserved"}"#).is_err());
    }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.path("/1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = IpWhois { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.asn, Some(13335));
    }
}
