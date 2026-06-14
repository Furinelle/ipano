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
    line!("威胁等级", threat_level, |v: &String| v.clone());
    line!("是否云", is_cloud, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否中继", is_relay, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否匿名", is_anonymous, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("人类流量", human_traffic_pct, |v: &f64| format!("{v}"));
    line!("机器人流量", bot_traffic_pct, |v: &f64| format!("{v}"));
    line!("设备分布", device_dist, |v: &String| v.clone());
    line!("VT无害", blacklist_harmless, |v: &u32| format!("{v}"));
    line!("VT恶意", blacklist_malicious, |v: &u32| format!("{v}"));
    line!("VT可疑", blacklist_suspicious, |v: &u32| format!("{v}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceData;
    #[test]
    fn raw_lists_phase2_fields() {
        let mut vt = SourceData::new("vt");
        vt.blacklist_malicious = Some(2);
        let mut cf = SourceData::new("cf");
        cf.human_traffic_pct = Some(78.5);
        cf.bot_traffic_pct = Some(21.5);
        let mut ipreg = SourceData::new("ipreg");
        ipreg.is_cloud = Some(true);
        ipreg.threat_level = Some("high".into());
        let report = MergedReport { raw: vec![vt, cf, ipreg], ..Default::default() };
        let s = render(&report);
        assert!(s.contains("VT恶意"));
        assert!(s.contains("2 [vt]"));
        assert!(s.contains("人类流量"));
        assert!(s.contains("78.5 [cf]"));
        assert!(s.contains("是否云"));
        assert!(s.contains("Yes [ipreg]"));
        assert!(s.contains("威胁等级"));
        assert!(s.contains("high [ipreg]"));
    }

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
