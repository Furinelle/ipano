use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult, IpType, AiVerdict};

#[derive(Deserialize)]
struct AiVerdictRaw {
    label: Option<String>,
    confidence: Option<i64>,
    reasoning: Option<String>,
}

#[derive(Deserialize)]
struct Resp {
    asn: Option<u32>,
    #[serde(rename = "asOrganization")]
    as_organization: Option<String>,
    company_name: Option<String>,
    company_type: Option<String>,
    country: Option<String>,
    region: Option<String>,
    city: Option<String>,
    rdns: Option<String>,
    is_proxy: Option<bool>,
    is_vpn: Option<bool>,
    is_tor: Option<bool>,
    is_datacenter: Option<bool>,
    #[serde(rename = "isResidential")]
    is_residential: Option<bool>,
    is_mobile: Option<bool>,
    is_abuser: Option<bool>,
    is_crawler: Option<bool>,
    trust_score: Option<i64>,
    abuser_score: Option<String>,
    rep_threat: Option<i64>,
    ai_verdict: Option<AiVerdictRaw>,
}

/// net.coffee 的 company_type/is_* 字段映射到统一 IpType
fn derive_ip_type(r: &Resp) -> Option<IpType> {
    if r.is_mobile == Some(true) { return Some(IpType::Mobile); }
    if r.is_datacenter == Some(true) || r.company_type.as_deref() == Some("hosting") {
        return Some(IpType::Hosting);
    }
    if r.is_residential == Some(true) { return Some(IpType::Residential); }
    match r.company_type.as_deref() {
        Some("business") => Some(IpType::Business),
        _ => None,
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("netcoffee");
    // derive_ip_type 需借用 r，必须在任何 String 字段移动之前调用
    d.ip_type = derive_ip_type(&r);
    d.asn = r.asn;
    d.as_org = r.as_organization.clone();
    d.isp = r.company_name.clone();
    d.org = r.company_name;
    d.country = r.country;
    d.region = r.region;
    d.city = r.city;
    d.rdns = r.rdns;
    d.is_proxy = r.is_proxy;
    d.is_vpn = r.is_vpn;
    d.is_tor = r.is_tor;
    d.is_hosting = r.is_datacenter;
    d.is_abuser = r.is_abuser;
    d.is_crawler = r.is_crawler;
    d.is_mobile = r.is_mobile;
    d.is_residential = r.is_residential;
    d.trust_score = r.trust_score;
    d.rep_threat = r.rep_threat;
    d.abuser_score = r.abuser_score;
    d.ai_verdict = r.ai_verdict.and_then(|v| match (v.label, v.confidence, v.reasoning) {
        (Some(label), Some(confidence), reasoning) =>
            Some(AiVerdict { label, confidence, reasoning: reasoning.unwrap_or_default() }),
        _ => None,
    });
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"ip":"1.1.1.2","is_datacenter":true,"isResidential":false,
        "is_vpn":false,"is_proxy":false,"is_tor":false,"is_crawler":false,"is_abuser":true,
        "is_mobile":false,"company_type":"hosting","company_name":"APNIC Research and Development",
        "abuser_score":"0.0234 (Elevated)","asn":13335,"asOrganization":"Cloudflare, Inc.",
        "country":"Australia","region":"Queensland","city":"South Brisbane","trust_score":41,
        "rdns":"security.cloudflare-dns.com","rep_threat":29,
        "ai_verdict":{"label":"Suspicious","confidence":60,"reasoning":"Mid-low trust score"}}"#;

    #[test]
    fn parse_extracts_base_and_risk() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "netcoffee");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.city.as_deref(), Some("South Brisbane"));
        assert_eq!(d.rdns.as_deref(), Some("security.cloudflare-dns.com"));
        assert_eq!(d.trust_score, Some(41));
        assert_eq!(d.rep_threat, Some(29));
        assert_eq!(d.abuser_score.as_deref(), Some("0.0234 (Elevated)"));
        assert_eq!(d.is_abuser, Some(true));
        assert_eq!(d.is_hosting, Some(true));
        assert_eq!(d.ip_type, Some(IpType::Hosting));
        let v = d.ai_verdict.unwrap();
        assert_eq!(v.label, "Suspicious");
        assert_eq!(v.confidence, 60);
    }

    #[test]
    fn parse_derives_mobile_type() {
        let body = r#"{"is_mobile":true,"is_datacenter":false,"company_type":"isp"}"#;
        let d = parse(body).unwrap();
        assert_eq!(d.ip_type, Some(IpType::Mobile));
        assert_eq!(d.is_mobile, Some(true));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse("not json").is_err());
    }
}
