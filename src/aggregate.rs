use std::net::IpAddr;
use crate::model::{SourceData, SourceResult, IpType};

/// 源优先级(靠前更可信),合并基础字段时按此顺序取首个非空值
const PRIORITY: [&str; 3] = ["ipinfo", "ipsb", "ipapi"];

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
    pub sources: Vec<SourceStatus>,
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
    m
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
}
