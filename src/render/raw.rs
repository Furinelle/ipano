use crate::aggregate::MergedReport;

/// securityCheck 同款逐字段逐源 [源缩写] 详表(纯文本)
pub fn render(report: &MergedReport) -> String {
    let mut out = String::from("═══ IP 质量检测(逐源) ═══\n");
    // 每个字段:遍历各源,列出有值的 (值 [源])
    macro_rules! line {
        ($label:expr, $field:ident, $fmt:expr) => {{
            let parts: Vec<String> = report.raw.iter()
                .filter_map(|d| d.$field.as_ref().map(|v| format!("{} [{}]", $fmt(v), d.source_id)))
                .collect();
            if !parts.is_empty() { out.push_str(&format!("{}: {}\n", $label, parts.join("  "))); }
        }};
    }
    line!("国家", country, |v: &String| v.clone());
    line!("使用类型", usage_type, |v: &String| v.clone());
    line!("公司类型", company_type, |v: &String| v.clone());
    line!("ASN滥用分", asn_abuse_score, |v: &f64| format!("{v}"));
    line!("公司滥用分", company_abuse_score, |v: &f64| format!("{v}"));
    line!("是否代理", is_proxy, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否VPN", is_vpn, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否数据中心", is_datacenter, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceData;
    #[test]
    fn raw_lists_per_source() {
        let mut a = SourceData::new("ipapiis"); a.is_proxy = Some(true); a.asn_abuse_score = Some(0.0131);
        let mut b = SourceData::new("ip2loc"); b.is_proxy = Some(false); b.usage_type = Some("DCH".into());
        let report = MergedReport { raw: vec![a, b], ..Default::default() };
        let s = render(&report);
        assert!(s.contains("是否代理"));
        assert!(s.contains("Yes [ipapiis]"));
        assert!(s.contains("No [ip2loc]"));
        assert!(s.contains("0.0131 [ipapiis]"));
        assert!(s.contains("DCH [ip2loc]"));
    }
}
