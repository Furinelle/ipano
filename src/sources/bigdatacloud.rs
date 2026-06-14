use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    country: Option<Country>,
    location: Option<Loc>,
    network: Option<Net>,
    #[serde(rename = "hazardReport")]
    hazard: Option<Hazard>,
}
#[derive(Deserialize)]
struct Country { #[serde(rename = "isoAlpha2")] iso: Option<String> }
#[derive(Deserialize)]
struct Loc { city: Option<String> }
#[derive(Deserialize)]
struct Net { organisation: Option<String> }
#[derive(Deserialize)]
struct Hazard {
    #[serde(rename = "isKnownAsVpn")] is_vpn: Option<bool>,
    #[serde(rename = "isKnownAsTorServer")] is_tor: Option<bool>,
    #[serde(rename = "isKnownAsProxy")] is_proxy: Option<bool>,
    #[serde(rename = "hazardScore")] hazard_score: Option<i64>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("bdc");
    d.country = r.country.and_then(|c| c.iso);
    d.city = r.location.and_then(|l| l.city);
    d.as_org = r.network.and_then(|n| n.organisation);
    if let Some(h) = r.hazard {
        d.is_vpn = h.is_vpn;
        d.is_tor = h.is_tor;
        d.is_proxy = h.is_proxy;
        d.risk_score = h.hazard_score;
    }
    Ok(d)
}

pub struct BigDataCloud {
    pub base: String,
    pub key: Option<String>,
}

impl Default for BigDataCloud {
    fn default() -> Self {
        BigDataCloud {
            base: "https://api.bigdatacloud.net".to_string(),
            key: std::env::var("IPANO_BDC_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for BigDataCloud {
    fn id(&self) -> &'static str { "bdc" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_BDC_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_BDC_KEY".to_string()))?;
        let url = format!(
            "{}/data/ip-geolocation-full?ip={}&localityLanguage=en&key={}",
            self.base, ip, key
        );
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SourceError::RateLimited);
        }
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"country":{"isoAlpha2":"AU","name":"Australia"},
"location":{"city":"Brisbane"},"network":{"organisation":"Cloudflare, Inc.","registeredCountry":{}},
"hazardReport":{"isKnownAsVpn":false,"isKnownAsTorServer":false,"isKnownAsProxy":false,"hazardScore":12}}"#;

    #[test]
    fn parse_extracts_hazard() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "bdc");
        assert_eq!(d.country.as_deref(), Some("AU"));
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.is_vpn, Some(false));
        assert_eq!(d.is_tor, Some(false));
        assert_eq!(d.is_proxy, Some(false));
        assert_eq!(d.risk_score, Some(12));
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = BigDataCloud { base: "https://api.bigdatacloud.net".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/data/ip-geolocation-full").query_param("key", "secret");
            then.status(200).body(SAMPLE);
        });
        let src = BigDataCloud { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.risk_score, Some(12));
    }
}
