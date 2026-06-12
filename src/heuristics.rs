use crate::aggregate::MergedReport;
use crate::i18n::Lang;
use crate::model::IpType;

/// 基于合并报告给出启发式结论(双语)。各判据独立,可叠加。
pub fn conclude(r: &MergedReport, lang: Lang) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    if r.is_tor == Some(true) {
        out.push(lang.pick("⚠ Tor 出口节点", "⚠ Tor exit node").into());
    }
    if r.is_vpn == Some(true) {
        out.push(lang.pick("⚠ 检测到 VPN", "⚠ VPN detected").into());
    }
    if r.is_proxy == Some(true) {
        out.push(lang.pick("⚠ 检测到代理", "⚠ Proxy detected").into());
    }

    match r.ip_type {
        Some(IpType::Hosting) => out.push(lang.pick("机房/IDC IP(非住宅)", "Datacenter/IDC IP (non-residential)").into()),
        Some(IpType::Native) => out.push(lang.pick("原生 IP", "Native IP").into()),
        Some(IpType::Residential) => out.push(lang.pick("住宅宽带 IP", "Residential broadband IP").into()),
        Some(IpType::Mobile) => out.push(lang.pick("移动网络 IP", "Mobile network IP").into()),
        Some(IpType::Broadcast) => out.push(lang.pick("广播/数据中心 IP", "Broadcast/datacenter IP").into()),
        Some(IpType::Business) => out.push(lang.pick("商业 IP", "Business IP").into()),
        _ => {}
    }

    if r.is_abuser == Some(true) {
        out.push(lang.pick("存在历史滥用记录", "Has prior abuse reports").into());
    }

    if let Some(t) = r.trust_score {
        if t < 40 {
            out.push(lang.pick("纯净度偏低(<40)", "Low purity score (<40)").into());
        }
    }

    let high_fraud = r.ipqs_score.map_or(false, |v| v >= 75)
        || r.fraud_score.map_or(false, |v| v >= 75)
        || r.abuseipdb_score.map_or(false, |v| v >= 50)
        || r.risk_score.map_or(false, |v| v >= 75);
    if high_fraud {
        out.push(lang.pick("高欺诈/滥用风险", "High fraud/abuse risk").into());
    }

    // 综合:无任何风险标记且纯净度高/属住宅原生
    let no_flags = r.is_proxy != Some(true) && r.is_vpn != Some(true)
        && r.is_tor != Some(true) && r.is_abuser != Some(true) && !high_fraud;
    let looks_clean = r.trust_score.map_or(false, |t| t >= 60)
        || matches!(r.ip_type, Some(IpType::Residential) | Some(IpType::Native));
    if no_flags && looks_clean {
        out.push(lang.pick("未见明显风险", "No obvious risk detected").into());
    }

    if out.is_empty() {
        out.push(lang.pick("数据不足,无法判定", "Insufficient data for a verdict").into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceData;
    use crate::aggregate::merge;

    fn report_from(d: SourceData) -> MergedReport {
        merge("1.1.1.1".parse().unwrap(), vec![("x".to_string(), Ok(d))])
    }

    #[test]
    fn flags_vpn_and_hosting() {
        let mut d = SourceData::new("x");
        d.is_vpn = Some(true);
        d.ip_type = Some(IpType::Hosting);
        let c = conclude(&report_from(d), Lang::Zh);
        assert!(c.iter().any(|s| s.contains("VPN")));
        assert!(c.iter().any(|s| s.contains("机房")));
    }

    #[test]
    fn high_fraud_flagged_en() {
        let mut d = SourceData::new("x");
        d.ipqs_score = Some(90);
        let c = conclude(&report_from(d), Lang::En);
        assert!(c.iter().any(|s| s.contains("High fraud")));
    }

    #[test]
    fn clean_residential() {
        let mut d = SourceData::new("x");
        d.ip_type = Some(IpType::Residential);
        d.trust_score = Some(85);
        let c = conclude(&report_from(d), Lang::Zh);
        assert!(c.iter().any(|s| s.contains("未见明显风险")));
        assert!(c.iter().any(|s| s.contains("住宅")));
    }

    #[test]
    fn empty_report_insufficient() {
        let r = merge("1.1.1.1".parse().unwrap(), vec![]);
        let c = conclude(&r, Lang::En);
        assert!(c.iter().any(|s| s.contains("Insufficient")));
    }
}
