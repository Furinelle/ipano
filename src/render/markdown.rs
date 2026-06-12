use crate::aggregate::MergedReport;
use crate::i18n::Lang;
use crate::heuristics::conclude;
use crate::model::SourceData;

fn opt(s: &Option<String>) -> String {
    s.clone().unwrap_or_else(|| "—".to_string())
}

fn flag(b: Option<bool>, lang: Lang) -> String {
    match b {
        Some(true) => lang.pick("是", "yes").to_string(),
        Some(false) => lang.pick("否", "no").to_string(),
        None => "—".to_string(),
    }
}

fn type_str(d: &SourceData) -> String {
    d.ip_type.map(|t| format!("{:?}", t)).unwrap_or_else(|| "—".to_string())
}

/// 该源任一风险分(优先级:ipqs > fraud > risk > abuseipdb > trust)
fn risk_cell(d: &SourceData) -> String {
    if let Some(v) = d.ipqs_score { return format!("ipqs:{}", v); }
    if let Some(v) = d.fraud_score { return format!("fraud:{}", v); }
    if let Some(v) = d.risk_score { return format!("risk:{}", v); }
    if let Some(v) = d.abuseipdb_score { return format!("abuse:{}", v); }
    if let Some(v) = d.trust_score { return format!("trust:{}", v); }
    "—".to_string()
}

pub fn to_markdown(r: &MergedReport, lang: Lang) -> String {
    let mut out = String::new();
    let ip = r.ip.map(|x| x.to_string()).unwrap_or_default();
    out.push_str(&format!("# {} {}\n\n", lang.pick("IP 全景报告", "IP Panorama Report"), ip));

    // 基础信息
    out.push_str(&format!("## {}\n\n", lang.pick("基础信息", "Basics")));
    out.push_str(&format!("| {} | {} |\n|---|---|\n", lang.pick("字段", "Field"), lang.pick("值", "Value")));
    let asn = r.asn.map(|a| format!("AS{}", a)).unwrap_or_else(|| "—".into());
    out.push_str(&format!("| ASN | {} {} |\n", asn, opt(&r.as_org)));
    out.push_str(&format!("| {} | {} {} {} |\n", lang.pick("归属", "Location"), opt(&r.country), opt(&r.region), opt(&r.city)));
    out.push_str(&format!("| rDNS | {} |\n", opt(&r.rdns)));
    out.push('\n');

    // 横向对比表
    if !r.raw.is_empty() {
        out.push_str(&format!("## {}\n\n", lang.pick("各源关键判定对比", "Per-source verdicts")));
        out.push_str(&format!(
            "| {} | {} | VPN | Tor | {} | {} |\n|---|---|---|---|---|---|\n",
            lang.pick("源", "Source"),
            lang.pick("代理", "Proxy"),
            lang.pick("类型", "Type"),
            lang.pick("风险分", "Risk"),
        ));
        for d in &r.raw {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                d.source_id,
                flag(d.is_proxy, lang),
                flag(d.is_vpn, lang),
                flag(d.is_tor, lang),
                type_str(d),
                risk_cell(d),
            ));
        }
        out.push('\n');
    }

    // 启发式结论
    out.push_str(&format!("## {}\n\n", lang.pick("启发式结论", "Heuristic verdict")));
    for line in conclude(r, lang) {
        out.push_str(&format!("- {}\n", line));
    }
    out.push('\n');

    // 源状态
    let status: Vec<String> = r.sources.iter()
        .map(|s| format!("{}{}", if s.ok { "✓" } else { "✗" }, s.id))
        .collect();
    out.push_str(&format!("> {} {}\n", lang.pick("源状态", "Source status"), status.join(" ")));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::merge;
    use crate::model::{SourceData, IpType};

    #[test]
    fn markdown_has_sections_and_comparison() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut a = SourceData::new("netcoffee");
        a.is_proxy = Some(false);
        a.trust_score = Some(41);
        a.ip_type = Some(IpType::Hosting);
        let mut b = SourceData::new("ipqs");
        b.is_vpn = Some(true);
        b.ipqs_score = Some(80);
        let r = merge(ip, vec![("netcoffee".into(), Ok(a)), ("ipqs".into(), Ok(b))]);

        let md = to_markdown(&r, Lang::Zh);
        assert!(md.contains("# IP 全景报告"));
        assert!(md.contains("各源关键判定对比"));
        assert!(md.contains("netcoffee"));
        assert!(md.contains("ipqs"));
        assert!(md.contains("ipqs:80"));
        assert!(md.contains("启发式结论"));
    }

    #[test]
    fn markdown_english() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut a = SourceData::new("netcoffee");
        a.trust_score = Some(85);
        a.ip_type = Some(IpType::Residential);
        let r = merge(ip, vec![("netcoffee".into(), Ok(a))]);
        let md = to_markdown(&r, Lang::En);
        assert!(md.contains("# IP Panorama Report"));
        assert!(md.contains("Per-source verdicts"));
        assert!(md.contains("Heuristic verdict"));
    }
}
