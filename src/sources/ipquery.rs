use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

// 替代原计划的 bigdatacloud(实测需 key)。ipquery.io 免key,且额外提供
// VPN/代理/Tor/数据中心/风险分等质量信号。

#[derive(Deserialize)]
struct Resp {
    isp: Option<Isp>,
    location: Option<Loc>,
    risk: Option<Risk>,
}
#[derive(Deserialize)]
struct Isp { asn: Option<String>, org: Option<String>, isp: Option<String> }
#[derive(Deserialize)]
struct Loc {
    country: Option<String>,
    state: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
}
#[derive(Deserialize)]
struct Risk {
    is_mobile: Option<bool>,
    is_vpn: Option<bool>,
    is_tor: Option<bool>,
    is_proxy: Option<bool>,
    is_datacenter: Option<bool>,
    risk_score: Option<i64>,
}

pub struct IpQuery { pub base: String }
impl Default for IpQuery { fn default() -> Self { IpQuery { base: "https://api.ipquery.io".into() } } }

#[async_trait]
impl Source for IpQuery {
    fn id(&self) -> &'static str { "ipquery" }
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
    let mut d = SourceData::new("ipquery");
    if let Some(i) = r.isp {
        d.asn = i.asn.and_then(|s| s.trim_start_matches("AS").parse().ok());
        d.as_org = i.org; d.isp = i.isp;
    }
    if let Some(l) = r.location {
        d.country = l.country; d.region = l.state; d.city = l.city;
        d.lat = l.latitude; d.lon = l.longitude; d.timezone = l.timezone;
    }
    if let Some(rk) = r.risk {
        d.is_mobile = rk.is_mobile; d.is_vpn = rk.is_vpn; d.is_tor = rk.is_tor;
        d.is_proxy = rk.is_proxy; d.is_datacenter = rk.is_datacenter; d.risk_score = rk.risk_score;
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"ip":"1.1.1.1","isp":{"asn":"AS13335","org":"Cloudflare, Inc.","isp":"Cloudflare, Inc."},"location":{"country":"Australia","country_code":"AU","city":"Sydney","state":"New South Wales","latitude":-33.88,"longitude":151.19,"timezone":"Australia/Sydney"},"risk":{"is_mobile":false,"is_vpn":false,"is_tor":false,"is_proxy":false,"is_datacenter":true,"risk_score":0}}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipquery");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.city.as_deref(), Some("Sydney"));
        assert_eq!(d.is_datacenter, Some(true));
        assert_eq!(d.is_vpn, Some(false));
        assert_eq!(d.risk_score, Some(0));
    }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.path("/1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = IpQuery { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.is_datacenter, Some(true));
    }
}
