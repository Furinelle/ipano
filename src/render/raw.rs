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
    line!("VT未检出", blacklist_undetected, |v: &u32| format!("{v}"));
    line!("信任分", trust_score, |v: &i64| format!("{v}"));
    line!("欺诈分", fraud_score, |v: &i64| format!("{v}"));
    line!("AbuseIPDB分", abuseipdb_score, |v: &i64| format!("{v}"));
    line!("是否Tor", is_tor, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否托管", is_hosting, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否爬虫", is_crawler, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否移动", is_mobile, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否住宅", is_residential, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否滥用者", is_abuser, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否Bogon", is_bogon, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("浏览器分布", browser_dist, |v: &String| v.clone());
    line!("系统分布", os_dist, |v: &String| v.clone());
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

    #[test]
    fn raw_lists_added_quality_fields() {
        let mut a = SourceData::new("ipapiis");
        a.trust_score = Some(72);
        a.fraud_score = Some(15);
        a.abuseipdb_score = Some(3);
        a.is_tor = Some(false);
        a.is_hosting = Some(true);
        a.is_crawler = Some(false);
        a.is_mobile = Some(false);
        a.is_residential = Some(false);
        a.is_abuser = Some(true);
        a.is_bogon = Some(false);
        a.browser_dist = Some("Chrome 78% 其他 22%".into());
        a.os_dist = Some("Windows 93% 其他 7%".into());
        a.blacklist_undetected = Some(91);
        let report = MergedReport { raw: vec![a], ..Default::default() };
        let s = render(&report);
        assert!(s.contains("信任分"));
        assert!(s.contains("72 [ipapiis]"));
        assert!(s.contains("欺诈分"));
        assert!(s.contains("AbuseIPDB分"));
        assert!(s.contains("是否Tor"));
        assert!(s.contains("是否托管"));
        assert!(s.contains("是否爬虫"));
        assert!(s.contains("是否移动"));
        assert!(s.contains("是否住宅"));
        assert!(s.contains("是否滥用者"));
        assert!(s.contains("Yes [ipapiis]"));
        assert!(s.contains("是否Bogon"));
        assert!(s.contains("浏览器分布"));
        assert!(s.contains("Chrome 78% 其他 22% [ipapiis]"));
        assert!(s.contains("系统分布"));
        assert!(s.contains("VT未检出"));
        assert!(s.contains("91 [ipapiis]"));
    }
}
