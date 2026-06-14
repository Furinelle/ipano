use comfy_table::{Table, presets::UTF8_FULL};
use owo_colors::OwoColorize;
use crate::aggregate::MergedReport;
use crate::i18n::Lang;
use crate::heuristics::conclude;

fn dash(s: &Option<String>) -> String {
    s.clone().unwrap_or_else(|| "—".to_string())
}

pub fn render(r: &MergedReport, no_color: bool, lang: Lang) -> String {
    let mut out = String::new();
    let ip = r.ip.map(|x| x.to_string()).unwrap_or_default();
    let header = format!("═══ IP 全景报告  {} ═══", ip);
    out.push_str(&if no_color { header.clone() } else { header.bold().to_string() });
    out.push('\n');

    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(vec!["字段", "值"]);
    let asn = r.asn.map(|a| format!("AS{}", a)).unwrap_or_else(|| "—".into());
    t.add_row(vec!["ASN".to_string(), format!("{} {}", asn, dash(&r.as_org))]);
    t.add_row(vec!["归属".to_string(), format!("{} {} {}", dash(&r.country), dash(&r.region), dash(&r.city))]);
    let loc = match (r.lat, r.lon) { (Some(a), Some(b)) => format!("{},{}", a, b), _ => "—".into() };
    t.add_row(vec!["经纬度".to_string(), loc]);
    t.add_row(vec!["时区".to_string(), dash(&r.timezone)]);
    t.add_row(vec!["rDNS".to_string(), dash(&r.rdns)]);
    out.push_str(&t.to_string());
    out.push('\n');

    // —— 风险/纯净度区 ——
    if has_risk(r) {
        let mut rt = Table::new();
        rt.load_preset(UTF8_FULL);
        rt.set_header(vec!["风险判定".to_string(), "值".to_string()]);
        if let Some(v) = r.trust_score { rt.add_row(vec!["纯净度(越高越干净)".to_string(), v.to_string()]); }
        if let Some(v) = r.risk_score { rt.add_row(vec!["风控值(越高越危险)".to_string(), v.to_string()]); }
        if let Some(v) = r.fraud_score { rt.add_row(vec!["欺诈分(越高越危险)".to_string(), v.to_string()]); }
        if let Some(v) = r.ipqs_score { rt.add_row(vec!["IPQS 欺诈分".to_string(), v.to_string()]); }
        if let Some(v) = r.abuseipdb_score { rt.add_row(vec!["AbuseIPDB 置信度".to_string(), v.to_string()]); }
        if let Some(v) = r.rep_threat { rt.add_row(vec!["信誉威胁值".to_string(), v.to_string()]); }
        if let Some(s) = &r.abuser_score { rt.add_row(vec!["滥用评分".to_string(), s.clone()]); }
        if let Some(t) = &r.threat_level { rt.add_row(vec!["威胁等级".to_string(), t.clone()]); }
        if r.blacklist_malicious.is_some() || r.blacklist_harmless.is_some() {
            rt.add_row(vec!["VT 黑名单".to_string(), format!(
                "{} 恶意 / {} 可疑 / {} 无害 / {} 未检出",
                r.blacklist_malicious.unwrap_or(0),
                r.blacklist_suspicious.unwrap_or(0),
                r.blacklist_harmless.unwrap_or(0),
                r.blacklist_undetected.unwrap_or(0))]);
        }
        if let (Some(h), Some(b)) = (r.human_traffic_pct, r.bot_traffic_pct) {
            rt.add_row(vec!["人机流量(CF Radar)".to_string(), format!("人类 {h}% / 机器人 {b}%")]);
        }
        if let Some(s) = &r.browser_dist { rt.add_row(vec!["浏览器分布(CF Radar)".to_string(), s.clone()]); }
        if let Some(s) = &r.os_dist { rt.add_row(vec!["系统分布(CF Radar)".to_string(), s.clone()]); }
        if let Some(s) = &r.device_dist { rt.add_row(vec!["设备分布(CF Radar)".to_string(), s.clone()]); }
        rt.add_row(vec!["标记".to_string(), risk_flags(r)]);
        if let Some(v) = &r.ai_verdict {
            rt.add_row(vec!["AI 判定".to_string(),
                format!("{}（{}%）{}", v.label, v.confidence, v.reasoning)]);
        }
        out.push_str(&rt.to_string());
        out.push('\n');
    }

    // —— 启发式结论 ——
    let title = lang.pick("启发式结论", "Heuristic verdict");
    out.push_str(&if no_color { title.to_string() } else { title.bold().to_string() });
    out.push('\n');
    for line in conclude(r, lang) {
        out.push_str(&format!("  • {}\n", line));
    }

    let status: Vec<String> = r.sources.iter().map(|s| {
        let mark = if s.ok { "✓" } else { "✗" };
        format!("{}{}", mark, s.id)
    }).collect();
    out.push_str(&format!("{}  {}\n", lang.pick("源状态", "Sources"), status.join(" ")));
    out
}

fn has_risk(r: &MergedReport) -> bool {
    r.trust_score.is_some() || r.risk_score.is_some() || r.rep_threat.is_some()
        || r.abuser_score.is_some() || r.ai_verdict.is_some() || r.fraud_score.is_some()
        || r.abuseipdb_score.is_some() || r.ipqs_score.is_some()
        || r.is_proxy == Some(true) || r.is_vpn == Some(true) || r.is_tor == Some(true)
        || r.is_abuser == Some(true) || r.ip_type.is_some()
        || r.blacklist_malicious.is_some() || r.human_traffic_pct.is_some()
        || r.threat_level.is_some() || r.is_cloud == Some(true)
}

fn risk_flags(r: &MergedReport) -> String {
    let mut f = Vec::new();
    if r.ip_type == Some(crate::model::IpType::Hosting) { f.push("机房"); }
    if r.ip_type == Some(crate::model::IpType::Native) { f.push("原生"); }
    if r.ip_type == Some(crate::model::IpType::Residential) { f.push("家宽"); }
    if r.ip_type == Some(crate::model::IpType::Mobile) { f.push("移动"); }
    if r.is_proxy == Some(true) { f.push("代理"); }
    if r.is_vpn == Some(true) { f.push("VPN"); }
    if r.is_tor == Some(true) { f.push("Tor"); }
    if r.is_abuser == Some(true) { f.push("滥用史"); }
    if r.is_crawler == Some(true) { f.push("爬虫"); }
    if r.is_cloud == Some(true) { f.push("云"); }
    if r.is_anonymous == Some(true) { f.push("匿名"); }
    if r.is_bogon == Some(true) { f.push("Bogon"); }
    if f.is_empty() { "—".to_string() } else { f.join(" ") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::merge;
    use crate::model::SourceData;

    #[test]
    fn render_contains_header_and_source_status() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut d = SourceData::new("ipsb");
        d.city = Some("Los Angeles".into());
        d.asn = Some(13335);
        let report = merge(ip, vec![
            ("ipsb".to_string(), Ok(d)),
            ("ipapi".to_string(), Err(crate::model::SourceError::Timeout)),
        ]);
        let out = render(&report, true, crate::i18n::Lang::Zh);
        assert!(out.contains("1.1.1.1"));
        assert!(out.contains("13335"));
        assert!(out.contains("ipsb"));
        assert!(out.contains("ipapi"));
    }

    #[test]
    fn render_shows_blacklist_and_traffic() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut vt = SourceData::new("vt");
        vt.blacklist_malicious = Some(2);
        vt.blacklist_harmless = Some(80);
        let mut cf = SourceData::new("cf");
        cf.human_traffic_pct = Some(78.5);
        cf.bot_traffic_pct = Some(21.5);
        let report = merge(ip, vec![("vt".into(), Ok(vt)), ("cf".into(), Ok(cf))]);
        let s = render(&report, true, crate::i18n::Lang::Zh);
        assert!(s.contains("VT 黑名单"));
        assert!(s.contains("2 恶意"));
        assert!(s.contains("人机流量"));
    }

    #[test]
    fn render_shows_risk_section() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut d = SourceData::new("netcoffee");
        d.trust_score = Some(41);
        d.rep_threat = Some(29);
        d.is_abuser = Some(true);
        d.ai_verdict = Some(crate::model::AiVerdict {
            label: "Suspicious".into(), confidence: 60, reasoning: "front possible".into(),
        });
        let report = merge(ip, vec![("netcoffee".to_string(), Ok(d))]);
        let out = render(&report, true, crate::i18n::Lang::Zh);
        assert!(out.contains("纯净度") || out.contains("可信"));
        assert!(out.contains("41"));
        assert!(out.contains("Suspicious"));
    }

    #[test]
    fn render_shows_dist_and_undetected_and_bogon() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut cf = SourceData::new("cf");
        cf.browser_dist = Some("Chrome 78% 其他 22%".into());
        cf.os_dist = Some("Windows 93% 其他 7%".into());
        cf.device_dist = Some("桌面 73% 移动 26%".into());
        let mut vt = SourceData::new("vt");
        vt.blacklist_malicious = Some(0);
        vt.blacklist_undetected = Some(91);
        let mut ipreg = SourceData::new("ipreg");
        ipreg.is_bogon = Some(true);
        let report = merge(ip, vec![
            ("cf".into(), Ok(cf)), ("vt".into(), Ok(vt)), ("ipreg".into(), Ok(ipreg)),
        ]);
        let s = render(&report, true, crate::i18n::Lang::Zh);
        assert!(s.contains("浏览器分布"));
        assert!(s.contains("系统分布"));
        assert!(s.contains("设备分布"));
        assert!(s.contains("未检出"));
        assert!(s.contains("Bogon"));
    }
}
