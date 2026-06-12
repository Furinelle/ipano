use std::collections::HashMap;
use std::net::IpAddr;
use reqwest::Client;
use serde_json::Value;

const V4_ENDPOINTS: [&str; 2] = ["https://api-ipv4.ip.sb/ip", "https://ipv4.icanhazip.com"];
const V6_ENDPOINTS: [&str; 2] = ["https://api-ipv6.ip.sb/ip", "https://ipv6.icanhazip.com"];

/// 取众数(出现次数最多的 IP)
pub fn majority(ips: &[String]) -> Option<IpAddr> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for s in ips { *counts.entry(s.as_str()).or_default() += 1; }
    counts.into_iter().max_by_key(|(_, c)| *c)
        .and_then(|(s, _)| s.parse().ok())
}

/// 抓单个端点,返回去空白后解析的 IP
pub async fn fetch_one(client: &Client, url: &str) -> Option<IpAddr> {
    let body = client.get(url).send().await.ok()?.text().await.ok()?;
    body.trim().parse().ok()
}

async fn discover(client: &Client, endpoints: &[&str]) -> Option<IpAddr> {
    let mut found = Vec::new();
    for url in endpoints {
        if let Some(ip) = fetch_one(client, url).await {
            found.push(ip.to_string());
        }
    }
    majority(&found)
}

/// 探测本机出口 v4 与 v6(任一可能为 None)
pub async fn detect(client: &Client) -> (Option<IpAddr>, Option<IpAddr>) {
    let v4 = discover(client, &V4_ENDPOINTS).await;
    let v6 = discover(client, &V6_ENDPOINTS).await;
    (v4, v6)
}

/// 探测本机出口 IP 的 ISO 国家码(用于流媒体 Native/DNS 区分)。
/// 使用 ip.sb geoip API;失败返回 None。
pub async fn detect_country(client: &Client) -> Option<String> {
    let resp = client.get("https://api.ip.sb/geoip").send().await.ok()?;
    let json: Value = resp.json().await.ok()?;
    json["country_code"].as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majority_picks_most_common() {
        let v = vec!["1.1.1.1".to_string(), "1.1.1.1".to_string(), "2.2.2.2".to_string()];
        assert_eq!(majority(&v), Some("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn majority_empty_is_none() {
        assert_eq!(majority(&[]), None);
    }

    #[tokio::test]
    async fn fetch_one_parses_trimmed_ip() {
        let server = httpmock::MockServer::start();
        server.mock(|when, then| { when.path("/ip"); then.status(200).body("1.1.1.1\n"); });
        let client = crate::fetch::build_client(5);
        let ip = fetch_one(&client, &format!("{}/ip", server.base_url())).await.unwrap();
        assert_eq!(ip, "1.1.1.1".parse::<std::net::IpAddr>().unwrap());
    }
}
