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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiVerdict {
    pub label: String,
    pub confidence: i64,
    pub reasoning: String,
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
    // —— P2 风险/纯净度字段 ——
    pub trust_score: Option<i64>,   // 可信/纯净分 0-100,越高越干净(net.coffee)
    pub risk_score: Option<i64>,    // 风控值 0-100,越高越危险(ping0)
    pub abuser_score: Option<String>,
    pub rep_threat: Option<i64>,    // 信誉威胁值(net.coffee)
    pub ai_verdict: Option<AiVerdict>,
    pub is_abuser: Option<bool>,
    pub is_crawler: Option<bool>,
    pub is_mobile: Option<bool>,
    pub is_residential: Option<bool>,
    // —— P3 ——
    pub fraud_score: Option<i64>,   // 欺诈分 0-100,越高越危险(ippure)
    // —— P4 西方欺诈库(各源独立保留)——
    pub abuseipdb_score: Option<i64>,  // 滥用置信度 0-100(AbuseIPDB,需 key)
    pub ipqs_score: Option<i64>,       // 欺诈分 0-100(IPQS,需 key)
    // —— 阶段一 多源质量字段 ——
    pub usage_type: Option<String>,       // Commercial/hosting/business/ISP
    pub company_type: Option<String>,     // isp/hosting/business
    pub asn_abuse_score: Option<f64>,     // ipapi.is ASN 滥用分
    pub company_abuse_score: Option<f64>, // ipapi.is 公司滥用分
    pub is_datacenter: Option<bool>,
    // —— 阶段二 keyed 源字段 ——
    pub threat_level: Option<String>,        // low/medium/high(ipdata/scamalytics/fraudlogix)
    pub human_traffic_pct: Option<f64>,      // cloudflare radar 人类流量占比
    pub bot_traffic_pct: Option<f64>,        // cloudflare radar 机器人流量占比
    pub browser_dist: Option<String>,        // cloudflare radar 浏览器分布摘要
    pub device_dist: Option<String>,         // cloudflare radar 设备类型分布摘要
    pub os_dist: Option<String>,             // cloudflare radar 操作系统分布摘要
    pub is_cloud: Option<bool>,              // 云服务商(ipregistry/ipdata)
    pub is_relay: Option<bool>,              // 中继(ipregistry,如 iCloud Relay)
    pub is_anonymous: Option<bool>,          // 匿名网络(ipregistry/ipdata)
    pub is_bogon: Option<bool>,              // bogon/保留地址(ipregistry/ipdata)
    pub blacklist_harmless: Option<u32>,     // virustotal 无害引擎数
    pub blacklist_malicious: Option<u32>,    // virustotal 恶意引擎数
    pub blacklist_suspicious: Option<u32>,   // virustotal 可疑引擎数
    pub blacklist_undetected: Option<u32>,   // virustotal 未检出引擎数
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

    #[test]
    fn sourcedata_has_risk_fields() {
        let mut d = SourceData::new("netcoffee");
        d.trust_score = Some(41);
        d.risk_score = Some(80);
        d.rep_threat = Some(29);
        d.abuser_score = Some("0.0234 (Elevated)".into());
        d.is_abuser = Some(true);
        d.ai_verdict = Some(AiVerdict {
            label: "Suspicious".into(), confidence: 60,
            reasoning: "Mid-low trust score".into(),
        });
        assert_eq!(d.trust_score, Some(41));
        assert_eq!(d.ai_verdict.as_ref().unwrap().confidence, 60);
    }

    #[test]
    fn sourcedata_has_quality_fields() {
        let mut d = SourceData::new("ipapiis");
        d.usage_type = Some("hosting".into());
        d.company_type = Some("hosting".into());
        d.asn_abuse_score = Some(0.0131);
        d.company_abuse_score = Some(0.015);
        d.is_datacenter = Some(true);
        assert_eq!(d.usage_type.as_deref(), Some("hosting"));
        assert_eq!(d.asn_abuse_score, Some(0.0131));
        assert_eq!(d.is_datacenter, Some(true));
    }

    #[test]
    fn ai_verdict_roundtrips_json() {
        let v = AiVerdict { label: "Clean".into(), confidence: 90, reasoning: "ok".into() };
        let s = serde_json::to_string(&v).unwrap();
        let back: AiVerdict = serde_json::from_str(&s).unwrap();
        assert_eq!(back.label, "Clean");
        assert_eq!(back.confidence, 90);
    }

    #[test]
    fn sourcedata_has_phase2_fields() {
        let mut d = SourceData::new("vt");
        d.threat_level = Some("high".into());
        d.human_traffic_pct = Some(78.5);
        d.bot_traffic_pct = Some(21.5);
        d.browser_dist = Some("Chrome 64% 其他 36%".into());
        d.device_dist = Some("desktop 70% mobile 30%".into());
        d.os_dist = Some("Windows 55% Android 25%".into());
        d.is_cloud = Some(true);
        d.is_relay = Some(false);
        d.is_anonymous = Some(false);
        d.is_bogon = Some(false);
        d.blacklist_harmless = Some(80);
        d.blacklist_malicious = Some(2);
        d.blacklist_suspicious = Some(1);
        d.blacklist_undetected = Some(11);
        assert_eq!(d.threat_level.as_deref(), Some("high"));
        assert_eq!(d.human_traffic_pct, Some(78.5));
        assert_eq!(d.is_cloud, Some(true));
        assert_eq!(d.blacklist_malicious, Some(2));
    }
}
