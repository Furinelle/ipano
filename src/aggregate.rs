use std::net::IpAddr;
use crate::model::{SourceData, SourceResult, IpType};

/// 源优先级(靠前更可信),合并基础字段时按此顺序取首个非空值
const PRIORITY: [&str; 7] = ["ipinfo", "ipsb", "netcoffee", "ippure", "ipapi", "ipqs", "abuseipdb"];

pub struct SourceStatus {
    pub id: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct MergedReport {
    pub ip: Option<IpAddr>,
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
    pub trust_score: Option<i64>,
    pub risk_score: Option<i64>,
    pub abuser_score: Option<String>,
    pub rep_threat: Option<i64>,
    pub ai_verdict: Option<crate::model::AiVerdict>,
    pub is_abuser: Option<bool>,
    pub is_crawler: Option<bool>,
    pub is_mobile: Option<bool>,
    pub is_residential: Option<bool>,
    pub fraud_score: Option<i64>,
    pub abuseipdb_score: Option<i64>,
    pub ipqs_score: Option<i64>,
    // —— 阶段一 多源质量字段 ——
    pub usage_type: Option<String>,
    pub company_type: Option<String>,
    pub asn_abuse_score: Option<f64>,
    pub company_abuse_score: Option<f64>,
    pub is_datacenter: Option<bool>,
    pub sources: Vec<SourceStatus>,
    /// 各成功源的原始数据(供横向对比表)
    pub raw: Vec<SourceData>,
}

pub fn merge(ip: IpAddr, results: Vec<(String, SourceResult)>) -> MergedReport {
    let mut ok: Vec<SourceData> = Vec::new();
    let mut statuses: Vec<SourceStatus> = Vec::new();
    for (id, res) in results {
        match res {
            Ok(d) => {
                statuses.push(SourceStatus { id: id.clone(), ok: true, error: None });
                ok.push(d);
            }
            Err(e) => statuses.push(SourceStatus { id, ok: false, error: Some(e.to_string()) }),
        }
    }
    ok.sort_by_key(|d| PRIORITY.iter().position(|p| *p == d.source_id).unwrap_or(usize::MAX));

    let mut m = MergedReport { ip: Some(ip), sources: statuses, ..Default::default() };
    macro_rules! pick {
        ($field:ident) => {
            for d in &ok {
                if m.$field.is_none() && d.$field.is_some() {
                    m.$field = d.$field.clone();
                }
            }
        };
    }
    pick!(asn); pick!(as_org); pick!(isp); pick!(org);
    pick!(country); pick!(region); pick!(city);
    pick!(lat); pick!(lon); pick!(timezone); pick!(rdns);
    pick!(ip_type); pick!(is_proxy); pick!(is_vpn); pick!(is_tor); pick!(is_hosting);
    pick!(trust_score); pick!(risk_score); pick!(abuser_score); pick!(rep_threat);
    pick!(ai_verdict); pick!(is_abuser); pick!(is_crawler); pick!(is_mobile); pick!(is_residential);
    pick!(fraud_score); pick!(abuseipdb_score); pick!(ipqs_score);
    pick!(usage_type); pick!(company_type);
    pick!(asn_abuse_score); pick!(company_abuse_score);
    m.is_datacenter = majority_bool(&ok, |d| d.is_datacenter);
    m.raw = ok;
    m
}

/// 多数决:多源布尔取多数;平票或无值返回 None。少数派由渲染层另行展示。
fn majority_bool(ok: &[SourceData], f: impl Fn(&SourceData) -> Option<bool>) -> Option<bool> {
    let (mut t, mut fa) = (0u32, 0u32);
    for d in ok { match f(d) { Some(true) => t += 1, Some(false) => fa += 1, None => {} } }
    if t == 0 && fa == 0 { None } else if t > fa { Some(true) } else if fa > t { Some(false) } else { Some(false) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceData;

    #[test]
    fn merge_picks_by_priority_and_records_status() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut ipinfo = SourceData::new("ipinfo");
        ipinfo.city = Some("LA-ipinfo".into());
        let mut ipsb = SourceData::new("ipsb");
        ipsb.city = Some("LA-ipsb".into());
        ipsb.asn = Some(13335); // ipinfo 无 asn,应回落到 ipsb
        let results = vec![
            ("ipsb".to_string(), Ok(ipsb)),
            ("ipinfo".to_string(), Ok(ipinfo)),
            ("ipapi".to_string(), Err(crate::model::SourceError::Timeout)),
        ];
        let m = merge(ip, results);
        // 优先级 ipinfo > ipsb > ipapi:city 取 ipinfo
        assert_eq!(m.city.as_deref(), Some("LA-ipinfo"));
        // asn ipinfo 缺,回落 ipsb
        assert_eq!(m.asn, Some(13335));
        // 状态:3 条,ipapi 失败
        assert_eq!(m.sources.len(), 3);
        let failed = m.sources.iter().find(|s| s.id == "ipapi").unwrap();
        assert!(!failed.ok);
    }

    #[test]
    fn merge_datacenter_majority() {
        let ip = "1.1.1.1".parse().unwrap();
        let mk = |id: &str, dc: bool| { let mut d = SourceData::new(id); d.is_datacenter = Some(dc); d };
        let m = merge(ip, vec![
            ("bdc".into(), Ok(mk("bdc", true))),
            ("ipapiis".into(), Ok(mk("ipapiis", true))),
            ("ip2loc".into(), Ok(mk("ip2loc", false))),
        ]);
        assert_eq!(m.is_datacenter, Some(true)); // 2:1 多数
    }

    #[test]
    fn merge_carries_risk_fields() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut nc = SourceData::new("netcoffee");
        nc.trust_score = Some(41);
        nc.rep_threat = Some(29);
        nc.is_abuser = Some(true);
        nc.ai_verdict = Some(crate::model::AiVerdict {
            label: "Suspicious".into(), confidence: 60, reasoning: "x".into(),
        });
        let m = merge(ip, vec![("netcoffee".to_string(), Ok(nc))]);
        assert_eq!(m.trust_score, Some(41));
        assert_eq!(m.rep_threat, Some(29));
        assert_eq!(m.is_abuser, Some(true));
        assert_eq!(m.ai_verdict.as_ref().unwrap().confidence, 60);
    }
}
