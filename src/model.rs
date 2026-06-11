use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IpType {
    Native,
    Broadcast,
    Hosting,
    Residential,
    Mobile,
    Business,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceData {
    pub source_id: String,
    pub asn: Option<u32>,
    pub as_org: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub timezone: Option<String>,
    pub rdns: Option<String>,
    pub ip_type: Option<IpType>,
    pub is_proxy: Option<bool>,
    pub is_vpn: Option<bool>,
    pub is_tor: Option<bool>,
    pub is_hosting: Option<bool>,
}

impl SourceData {
    pub fn new(source_id: &str) -> Self {
        SourceData { source_id: source_id.to_string(), ..Default::default() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("不可用: {0}")]
    Unavailable(String),
    #[error("触发限流")]
    RateLimited,
    #[error("需要 key: {0}")]
    NeedsKey(String),
    #[error("反爬挑战失败")]
    ChallengeFailed,
    #[error("超时")]
    Timeout,
    #[error("解析失败: {0}")]
    Parse(String),
}

pub type SourceResult = Result<SourceData, SourceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sourcedata_default_is_empty() {
        let d = SourceData::new("ipapi");
        assert_eq!(d.source_id, "ipapi");
        assert!(d.asn.is_none());
        assert!(d.country.is_none());
    }

    #[test]
    fn iptype_serializes_to_lowercase_tag() {
        let j = serde_json::to_string(&IpType::Hosting).unwrap();
        assert_eq!(j, "\"hosting\"");
    }
}
