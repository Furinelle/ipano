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
}
