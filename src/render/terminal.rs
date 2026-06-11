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

    let status: Vec<String> = r.sources.iter().map(|s| {
        let mark = if s.ok { "✓" } else { "✗" };
        format!("{}{}", mark, s.id)
    }).collect();
    out.push_str(&format!("源状态  {}\n", status.join(" ")));
    out
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
        let out = render(&report, true); // no_color=true 便于断言纯文本
        assert!(out.contains("1.1.1.1"));
        assert!(out.contains("13335"));
        assert!(out.contains("ipsb"));
        assert!(out.contains("ipapi"));
    }
}
