use serde_json::json;
use crate::aggregate::MergedReport;
use crate::probe::ProbeResult;
use crate::probe::mail::MailResult;
use crate::probe::route::RouteResult;
use crate::probe::dnsbl::DnsblResult;
use crate::probe::speedtest::SpeedResult;

pub fn to_json(r: &MergedReport, probes: &[ProbeResult], mail: &[MailResult], routes: &[RouteResult], dnsbl: &[DnsblResult], speedtest: &[SpeedResult]) -> String {
    let sources: Vec<_> = r.sources.iter().map(|s| json!({
        "id": s.id, "ok": s.ok, "error": s.error,
    })).collect();
    let v = json!({
        "ip": r.ip.map(|x| x.to_string()),
        "asn": r.asn,
        "as_org": r.as_org,
        "isp": r.isp,
        "country": r.country,
        "region": r.region,
        "city": r.city,
        "lat": r.lat,
        "lon": r.lon,
        "timezone": r.timezone,
        "rdns": r.rdns,
        "ip_type": r.ip_type,
        "is_proxy": r.is_proxy,
        "is_hosting": r.is_hosting,
        "is_vpn": r.is_vpn,
        "is_tor": r.is_tor,
        "is_abuser": r.is_abuser,
        "is_crawler": r.is_crawler,
        "is_mobile": r.is_mobile,
        "is_residential": r.is_residential,
        "trust_score": r.trust_score,
        "risk_score": r.risk_score,
        "abuser_score": r.abuser_score,
        "rep_threat": r.rep_threat,
        "ai_verdict": r.ai_verdict,
        "fraud_score": r.fraud_score,
        "abuseipdb_score": r.abuseipdb_score,
        "ipqs_score": r.ipqs_score,
        "usage_type": r.usage_type,
        "company_type": r.company_type,
        "asn_abuse_score": r.asn_abuse_score,
        "company_abuse_score": r.company_abuse_score,
        "is_datacenter": r.is_datacenter,
        "threat_level": r.threat_level,
        "human_traffic_pct": r.human_traffic_pct,
        "bot_traffic_pct": r.bot_traffic_pct,
        "browser_dist": r.browser_dist,
        "device_dist": r.device_dist,
        "os_dist": r.os_dist,
        "is_cloud": r.is_cloud,
        "is_relay": r.is_relay,
        "is_anonymous": r.is_anonymous,
        "is_bogon": r.is_bogon,
        "blacklist_harmless": r.blacklist_harmless,
        "blacklist_malicious": r.blacklist_malicious,
        "blacklist_suspicious": r.blacklist_suspicious,
        "blacklist_undetected": r.blacklist_undetected,
        "sources": sources,
        "probes": probes,
        "mail": mail,
        "route": routes,
        "dnsbl": dnsbl,
        "speedtest": speedtest,
    });
    serde_json::to_string_pretty(&v).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::merge;
    use crate::model::SourceData;

    #[test]
    fn json_contains_ip_and_asn() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut d = SourceData::new("ipsb");
        d.asn = Some(13335);
        let report = merge(ip, vec![("ipsb".to_string(), Ok(d))]);
        let s = to_json(&report, &[], &[], &[], &[], &[]);
        assert!(s.contains("\"ip\""));
        assert!(s.contains("13335"));
    }

    #[test]
    fn json_contains_risk_fields() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut d = SourceData::new("netcoffee");
        d.trust_score = Some(41);
        d.is_vpn = Some(false);
        d.is_tor = Some(true);
        d.ai_verdict = Some(crate::model::AiVerdict {
            label: "Suspicious".into(), confidence: 60, reasoning: "x".into(),
        });
        let report = merge(ip, vec![("netcoffee".to_string(), Ok(d))]);
        let s = to_json(&report, &[], &[], &[], &[], &[]);
        assert!(s.contains("trust_score"));
        assert!(s.contains("\"is_tor\""));
        assert!(s.contains("ai_verdict"));
        assert!(s.contains("Suspicious"));
    }

    #[test]
    fn json_contains_phase2_fields() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut d = SourceData::new("vt");
        d.blacklist_malicious = Some(2);
        d.blacklist_harmless = Some(80);
        let mut cf = SourceData::new("cf");
        cf.human_traffic_pct = Some(78.5);
        let report = merge(ip, vec![("vt".into(), Ok(d)), ("cf".into(), Ok(cf))]);
        let s = to_json(&report, &[], &[], &[], &[], &[]);
        assert!(s.contains("blacklist_malicious"));
        assert!(s.contains("human_traffic_pct"));
        assert!(s.contains("threat_level"));
        assert!(s.contains("is_cloud"));
    }

    #[test]
    fn json_contains_quality_fields() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut d = SourceData::new("ipapiis");
        d.usage_type = Some("DCH".into());
        d.company_type = Some("hosting".into());
        d.asn_abuse_score = Some(0.0131);
        d.company_abuse_score = Some(0.015);
        d.is_datacenter = Some(true);
        let report = merge(ip, vec![("ipapiis".to_string(), Ok(d))]);
        let s = to_json(&report, &[], &[], &[], &[], &[]);
        assert!(s.contains("usage_type"));
        assert!(s.contains("asn_abuse_score"));
        assert!(s.contains("company_abuse_score"));
        assert!(s.contains("is_datacenter"));
    }
}
