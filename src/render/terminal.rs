use comfy_table::{Table, presets::UTF8_FULL};
use owo_colors::OwoColorize;
use crate::aggregate::MergedReport;

fn dash(s: &Option<String>) -> String {
    s.clone().unwrap_or_else(|| "—".to_string())
}

pub fn render(r: &MergedReport, no_color: bool) -> String {
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
        if let Some(v) = r.rep_threat { rt.add_row(vec!["信誉威胁值".to_string(), v.to_string()]); }
        if let Some(s) = &r.abuser_score { rt.add_row(vec!["滥用评分".to_string(), s.clone()]); }
        rt.add_row(vec!["标记".to_string(), risk_flags(r)]);
        if let Some(v) = &r.ai_verdict {
            rt.add_row(vec!["AI 判定".to_string(),
                format!("{}（{}%）{}", v.label, v.confidence, v.reasoning)]);
        }
        out.push_str(&rt.to_string());
        out.push('\n');
    }

    let status: Vec<String> = r.sources.iter().map(|s| {
        let mark = if s.ok { "✓" } else { "✗" };
        format!("{}{}", mark, s.id)
    }).collect();
    out.push_str(&format!("源状态  {}\n", status.join(" ")));
    out
}

fn has_risk(r: &MergedReport) -> bool {
    r.trust_score.is_some() || r.risk_score.is_some() || r.rep_threat.is_some()
        || r.abuser_score.is_some() || r.ai_verdict.is_some() || r.fraud_score.is_some()
        || r.is_proxy == Some(true) || r.is_vpn == Some(true) || r.is_tor == Some(true)
        || r.is_abuser == Some(true) || r.ip_type.is_some()
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
        let out = render(&report, true);
        assert!(out.contains("1.1.1.1"));
        assert!(out.contains("13335"));
        assert!(out.contains("ipsb"));
        assert!(out.contains("ipapi"));
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
        let out = render(&report, true);
        assert!(out.contains("纯净度") || out.contains("可信"));
        assert!(out.contains("41"));
        assert!(out.contains("Suspicious"));
    }
}
