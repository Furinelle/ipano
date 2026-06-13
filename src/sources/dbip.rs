use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    #[serde(rename = "countryName")] country_name: Option<String>,
    #[serde(rename = "stateProv")] state_prov: Option<String>,
    city: Option<String>,
    error: Option<String>,
}

pub struct DbIp { pub base: String }
impl Default for DbIp { fn default() -> Self { DbIp { base: "https://api.db-ip.com/v2/free".into() } } }

#[async_trait]
impl Source for DbIp {
    fn id(&self) -> &'static str { "dbip" }
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
    if let Some(e) = r.error { return Err(SourceError::Unavailable(e)); }
    let mut d = SourceData::new("dbip");
    d.country = r.country_name; d.region = r.state_prov; d.city = r.city;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"ipAddress":"1.1.1.1","continentCode":"OC","countryName":"Australia","stateProv":"Queensland","city":"Brisbane"}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "dbip");
        assert_eq!(d.country.as_deref(), Some("Australia"));
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
    }
    #[test]
    fn parse_error() { assert!(parse(r#"{"error":"quota exceeded"}"#).is_err()); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.path("/1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = DbIp { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
    }
}
