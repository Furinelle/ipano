use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::sources::ipapi::split_as;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    hostname: Option<String>,
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
    loc: Option<String>,
    org: Option<String>,
    timezone: Option<String>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipinfo");
    if let Some(o) = r.org {
        let (asn, org) = split_as(&o);
        d.asn = asn;
        d.as_org = org;
    }
    if let Some(loc) = r.loc {
        let mut it = loc.splitn(2, ',');
        d.lat = it.next().and_then(|v| v.trim().parse().ok());
        d.lon = it.next().and_then(|v| v.trim().parse().ok());
    }
    d.country = r.country;
    d.region = r.region;
    d.city = r.city;
    d.timezone = r.timezone;
    d.rdns = r.hostname;
    Ok(d)
}

pub struct IpInfo {
    pub base: String,
}

impl Default for IpInfo {
    fn default() -> Self {
        IpInfo { base: "https://ipinfo.io".to_string() }
    }
}

#[async_trait]
impl Source for IpInfo {
    fn id(&self) -> &'static str { "ipinfo" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/{}/json", self.base, ip);
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

    const SAMPLE: &str = r#"{"ip":"1.1.1.1","hostname":"one.one.one.one",
        "city":"Los Angeles","region":"California","country":"US",
        "loc":"34.05,-118.24","org":"AS13335 Cloudflare, Inc.",
        "timezone":"America/Los_Angeles"}"#;

    #[test]
    fn parse_extracts_fields() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipinfo");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.lat, Some(34.05));
        assert_eq!(d.lon, Some(-118.24));
        assert_eq!(d.rdns.as_deref(), Some("one.one.one.one"));
    }

    #[tokio::test]
    async fn fetch_hits_endpoint() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/1.1.1.1/json");
            then.status(200).body(SAMPLE);
        });
        let src = IpInfo { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.city.as_deref(), Some("Los Angeles"));
    }
}
