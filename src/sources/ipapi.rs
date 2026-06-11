use serde::Deserialize;
use crate::model::{SourceData, SourceError, IpType};

#[derive(Deserialize)]
struct Resp {
    status: String,
    message: Option<String>,
    country: Option<String>,
    #[serde(rename = "regionName")]
    region_name: Option<String>,
    city: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    timezone: Option<String>,
    isp: Option<String>,
    org: Option<String>,
    #[serde(rename = "as")]
    as_field: Option<String>,
    proxy: Option<bool>,
    hosting: Option<bool>,
    mobile: Option<bool>,
}

/// 从 "AS13335 Cloudflare, Inc." 拆出 (asn, org)
pub(crate) fn split_as(s: &str) -> (Option<u32>, Option<String>) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("AS") {
        let mut it = rest.splitn(2, ' ');
        let num = it.next().and_then(|n| n.parse::<u32>().ok());
        let org = it.next().map(|o| o.trim().to_string()).filter(|o| !o.is_empty());
        if num.is_some() { return (num, org); }
    }
    (None, Some(s.to_string()))
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if r.status != "success" {
        return Err(SourceError::Unavailable(r.message.unwrap_or_default()));
    }
    let mut d = SourceData::new("ipapi");
    if let Some(a) = r.as_field {
        let (asn, org) = split_as(&a);
        d.asn = asn;
        d.as_org = org;
    }
    d.country = r.country;
    d.region = r.region_name;
    d.city = r.city;
    d.lat = r.lat;
    d.lon = r.lon;
    d.timezone = r.timezone;
    d.isp = r.isp;
    d.org = r.org;
    d.is_proxy = r.proxy;
    d.is_hosting = r.hosting;
    d.ip_type = match (r.hosting, r.mobile) {
        (Some(true), _) => Some(IpType::Hosting),
        (_, Some(true)) => Some(IpType::Mobile),
        _ => None,
    };
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IpType;

    const SAMPLE: &str = r#"{
        "status":"success","country":"United States","regionName":"California",
        "city":"Los Angeles","lat":34.05,"lon":-118.24,"timezone":"America/Los_Angeles",
        "isp":"Cloudflare","org":"Cloudflare","as":"AS13335 Cloudflare, Inc.",
        "proxy":false,"hosting":true,"mobile":false,"query":"1.1.1.1"}"#;

    #[test]
    fn parse_extracts_fields() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipapi");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.city.as_deref(), Some("Los Angeles"));
        assert_eq!(d.lat, Some(34.05));
        assert_eq!(d.ip_type, Some(IpType::Hosting));
        assert_eq!(d.is_hosting, Some(true));
        assert_eq!(d.is_proxy, Some(false));
    }

    #[test]
    fn parse_fail_status_is_err() {
        let body = r#"{"status":"fail","message":"reserved range","query":"127.0.0.1"}"#;
        assert!(parse(body).is_err());
    }

    #[test]
    fn split_as_parses_asn_and_org() {
        assert_eq!(split_as("AS13335 Cloudflare, Inc."), (Some(13335), Some("Cloudflare, Inc.".into())));
        assert_eq!(split_as("Cloudflare"), (None, Some("Cloudflare".into())));
    }
}
