use serde_json::json;
use crate::aggregate::MergedReport;

pub fn to_json(r: &MergedReport) -> String {
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
        "sources": sources,
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
        let s = to_json(&report);
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
        let s = to_json(&report);
        assert!(s.contains("trust_score"));
        assert!(s.contains("\"is_tor\""));
        assert!(s.contains("ai_verdict"));
        assert!(s.contains("Suspicious"));
    }
}
