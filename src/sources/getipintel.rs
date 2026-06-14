use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    status: Option<String>,
    result: Option<String>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    // status=error 优先于数值解析:出错时 result 可能非数值,先判状态才能给出准确错误
    if r.status.as_deref() == Some("error") {
        return Err(SourceError::Unavailable("getipintel 返回 error 状态".into()));
    }
    let prob: f64 = r.result.as_deref().and_then(|s| s.parse().ok())
        .ok_or_else(|| SourceError::Parse("getipintel result 非数值".into()))?;
    if prob < 0.0 {
        return Err(SourceError::Unavailable(format!("getipintel 错误码 {prob}")));
    }
    let mut d = SourceData::new("ipintel");
    d.risk_score = Some((prob * 100.0).round() as i64);
    // getipintel 官方推荐:概率 >= 0.95 视为高置信度代理/VPN
    d.is_proxy = Some(prob >= 0.95);
    Ok(d)
}

pub struct GetIpIntel {
    pub base: String,
    pub key: Option<String>,
}

impl Default for GetIpIntel {
    fn default() -> Self {
        GetIpIntel {
            base: "http://check.getipintel.net".to_string(),
            key: std::env::var("IPANO_IPINTEL_EMAIL").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for GetIpIntel {
    fn id(&self) -> &'static str { "ipintel" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_IPINTEL_EMAIL") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(||
            SourceError::NeedsKey("IPANO_IPINTEL_EMAIL".to_string()))?;
        let url = format!("{}/check.php?ip={}&contact={}&flags=f&format=json", self.base, ip, key);
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

    const SAMPLE: &str = r#"{"status":"success","result":"0.97","queryIP":"1.1.1.1","queryFlags":"f","queryFormat":"json","contact":"x@example.com"}"#;

    #[test]
    fn parse_maps_probability_to_risk() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipintel");
        assert_eq!(d.risk_score, Some(97));
        assert_eq!(d.is_proxy, Some(true));
    }

    #[test]
    fn parse_low_probability_not_proxy() {
        let body = r#"{"status":"success","result":"0.10"}"#;
        let d = parse(body).unwrap();
        assert_eq!(d.risk_score, Some(10));
        assert_eq!(d.is_proxy, Some(false));
    }

    #[test]
    fn parse_error_status_is_err() {
        let body = r#"{"status":"error","result":"-3"}"#;
        assert!(parse(body).is_err());
    }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = GetIpIntel { base: "http://check.getipintel.net".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_email_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/check.php").query_param("contact", "me@example.com");
            then.status(200).body(SAMPLE);
        });
        let src = GetIpIntel { base: server.base_url(), key: Some("me@example.com".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.is_proxy, Some(true));
    }
}
