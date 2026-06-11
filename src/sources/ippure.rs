use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult, IpType};

#[derive(Deserialize)]
struct Resp {
    ip: Option<String>,
    asn: Option<u32>,
    #[serde(rename = "asOrganization")]
    as_organization: Option<String>,
    country: Option<String>,
    city: Option<String>,
    timezone: Option<String>,
    longitude: Option<String>,   // ippure 返回字符串如 "114.17469"
    latitude: Option<String>,
    #[serde(rename = "fraudScore")]
    fraud_score: Option<i64>,
    #[serde(rename = "isResidential")]
    is_residential: Option<bool>,
    #[serde(rename = "isBroadcast")]
    is_broadcast: Option<bool>,
}

/// 提取响应里的 ip 字段,用于 egress 守卫(ippure 只返回调用者出口 IP)
pub fn returned_ip(body: &str) -> Option<String> {
    let r: Resp = serde_json::from_str(body).ok()?;
    r.ip
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ippure");
    d.asn = r.asn;
    d.as_org = r.as_organization;
    d.country = r.country;
    d.city = r.city;
    d.timezone = r.timezone;
    d.lat = r.latitude.and_then(|s| s.parse::<f64>().ok());
    d.lon = r.longitude.and_then(|s| s.parse::<f64>().ok());
    d.fraud_score = r.fraud_score;
    d.is_residential = r.is_residential;
    d.ip_type = if r.is_broadcast == Some(true) {
        Some(IpType::Broadcast)
    } else if r.is_residential == Some(true) {
        Some(IpType::Residential)
    } else {
        None
    };
    Ok(d)
}

pub struct IpPure {
    pub base: String,
}

impl Default for IpPure {
    fn default() -> Self {
        IpPure { base: "https://my.ippure.com".to_string() }
    }
}

#[async_trait]
impl Source for IpPure {
    fn id(&self) -> &'static str { "ippure" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/v1/info", self.base);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        // egress 守卫:ippure 只返回本机出口 IP,无法查指定 IP。
        // 返回 ip 与查询 ip 不符 → 当前是"查指定 IP"模式,跳过本源。
        if let Some(rip) = returned_ip(&body) {
            if rip != ip.to_string() {
                return Err(SourceError::Unavailable(
                    format!("ippure 仅返回本机出口 IP({}),与查询 {} 不符,跳过", rip, ip)));
            }
        }
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "ip":"154.37.217.75","asn":979,"asOrganization":"NetLab",
        "country":"Hong Kong SAR China","countryCode":"HK","city":"Hong Kong",
        "timezone":"Asia/Hong_Kong","longitude":"114.17469","latitude":"22.27832",
        "postalCode":"999077","fraudScore":39,"isResidential":false,"isBroadcast":true}"#;

    #[test]
    fn parse_extracts_fraud_and_type() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ippure");
        assert_eq!(d.asn, Some(979));
        assert_eq!(d.as_org.as_deref(), Some("NetLab"));
        assert_eq!(d.fraud_score, Some(39));
        assert_eq!(d.lat, Some(22.27832));
        assert_eq!(d.lon, Some(114.17469));
        assert_eq!(d.ip_type, Some(IpType::Broadcast));
        assert_eq!(d.is_residential, Some(false));
    }

    #[test]
    fn parse_residential_type() {
        let body = r#"{"ip":"1.2.3.4","fraudScore":5,"isResidential":true,"isBroadcast":false}"#;
        let d = parse(body).unwrap();
        assert_eq!(d.ip_type, Some(IpType::Residential));
        assert_eq!(d.fraud_score, Some(5));
    }

    #[test]
    fn returned_ip_extracts() {
        assert_eq!(returned_ip(SAMPLE).as_deref(), Some("154.37.217.75"));
        assert_eq!(returned_ip("garbage"), None);
    }

    #[tokio::test]
    async fn fetch_matching_ip_returns_data() {
        let server = httpmock::MockServer::start();
        // mock 返回 ip 与查询 ip 一致 → egress 模式,数据采用
        let body = r#"{"ip":"1.1.1.1","asn":13335,"fraudScore":7,"isResidential":false,"isBroadcast":false}"#;
        let m = server.mock(|when, then| {
            when.path("/v1/info");
            then.status(200).body(body);
        });
        let src = IpPure { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.fraud_score, Some(7));
    }

    #[tokio::test]
    async fn fetch_mismatched_ip_skips() {
        let server = httpmock::MockServer::start();
        // mock 返回本机出口 IP,但查询的是 8.8.8.8 → 不符,跳过
        let m = server.mock(|when, then| {
            when.path("/v1/info");
            then.status(200).body(SAMPLE);  // ip=154.37.217.75
        });
        let src = IpPure { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "8.8.8.8".parse().unwrap()).await.unwrap_err();
        m.assert();
        assert!(matches!(err, SourceError::Unavailable(_)));
    }
}
