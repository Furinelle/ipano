use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    country: Option<String>,
    asn: Option<u32>,
    asn_organization: Option<String>,
    isp: Option<String>,
    city: Option<String>,
    region: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipsb");
    d.asn = r.asn;
    d.as_org = r.asn_organization;
    d.isp = r.isp;
    d.country = r.country;
    d.region = r.region;
    d.city = r.city;
    d.lat = r.latitude;
    d.lon = r.longitude;
    d.timezone = r.timezone;
    Ok(d)
}

pub struct IpSb {
    pub base: String,
}

impl Default for IpSb {
    fn default() -> Self {
        IpSb { base: "https://api.ip.sb".to_string() }
    }
}

#[async_trait]
impl Source for IpSb {
    fn id(&self) -> &'static str { "ipsb" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/geoip/{}", self.base, ip);
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

    const SAMPLE: &str = r#"{"ip":"1.1.1.1","country":"United States","country_code":"US",
        "asn":13335,"asn_organization":"Cloudflare, Inc.","isp":"Cloudflare",
        "city":"Los Angeles","region":"California","latitude":34.05,"longitude":-118.24,
        "timezone":"America/Los_Angeles"}"#;

    #[test]
    fn parse_extracts_fields() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipsb");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.city.as_deref(), Some("Los Angeles"));
        assert_eq!(d.lat, Some(34.05));
    }

    #[tokio::test]
    async fn fetch_hits_endpoint() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/geoip/1.1.1.1");
            then.status(200).body(SAMPLE);
        });
        let src = IpSb { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.asn, Some(13335));
    }
}
