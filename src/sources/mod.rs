pub mod ipapi;
pub mod ipwhois;
pub mod dbip;
pub mod ipquery;
pub mod ipapiis;
pub mod ipinfo;
pub mod ipsb;
pub mod netcoffee;
pub mod ping0;
pub mod ippure;
pub mod abuseipdb;
pub mod ipqs;

use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use futures::future::join_all;
use crate::model::SourceResult;

#[async_trait]
pub trait Source: Send + Sync {
    fn id(&self) -> &'static str;
    /// 该源所需的环境变量名(用于未来配置提示);当前仅作元数据
    #[allow(dead_code)]
    fn needs_key(&self) -> Option<&'static str> { None }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult;
}

/// 并发执行所有源,返回 (source_id, 结果)。单源失败不影响其它。
pub async fn run_all(
    client: &Client,
    ip: IpAddr,
    sources: &[Box<dyn Source>],
) -> Vec<(String, SourceResult)> {
    let futs = sources.iter().map(|s| async move {
        (s.id().to_string(), s.fetch(client, ip).await)
    });
    join_all(futs).await
}

/// 默认启用的全部免 key 源(Task 9 恢复为三源)
pub fn all_sources(ping0_token: Option<String>) -> Vec<Box<dyn Source>> {
    // CLI --ping0-token 优先于环境变量;两者皆无则 ping0 运行期降级
    let mut p = ping0::Ping0::default();
    if let Some(t) = ping0_token {
        if !t.is_empty() { p.token = Some(t); }
    }
    vec![
        Box::new(p),
        Box::new(ipapi::IpApi::default()),
        Box::new(ipinfo::IpInfo::default()),
        Box::new(ipsb::IpSb::default()),
        Box::new(netcoffee::NetCoffee::default()),
        Box::new(ippure::IpPure::default()),
        Box::new(abuseipdb::AbuseIpdb::default()),
        Box::new(ipqs::Ipqs::default()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SourceData, SourceResult};
    use async_trait::async_trait;
    use std::net::IpAddr;
    use reqwest::Client;

    struct Dummy;
    #[async_trait]
    impl Source for Dummy {
        fn id(&self) -> &'static str { "dummy" }
        async fn fetch(&self, _c: &Client, _ip: IpAddr) -> SourceResult {
            Ok(SourceData::new("dummy"))
        }
    }

    #[tokio::test]
    async fn run_all_collects_results() {
        let client = crate::fetch::build_client(5);
        let srcs: Vec<Box<dyn Source>> = vec![Box::new(Dummy)];
        let ip: IpAddr = "1.1.1.1".parse().unwrap();
        let out = run_all(&client, ip, &srcs).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "dummy");
        assert!(out[0].1.is_ok());
    }

    #[test]
    fn all_sources_includes_netcoffee() {
        let s = all_sources(None);
        let ids: Vec<&str> = s.iter().map(|x| x.id()).collect();
        assert!(ids.contains(&"ipapi"));
        assert!(ids.contains(&"ipinfo"));
        assert!(ids.contains(&"ipsb"));
        assert!(ids.contains(&"netcoffee"));
    }
}
