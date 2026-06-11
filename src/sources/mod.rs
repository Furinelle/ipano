pub mod ipapi;
pub mod ipinfo;
pub mod ipsb;

use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use futures::future::join_all;
use crate::model::SourceResult;

#[async_trait]
pub trait Source: Send + Sync {
    fn id(&self) -> &'static str;
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

/// 默认启用的全部免 key 源(Task 9 恢复为三源,本任务临时返回空)
pub fn all_sources() -> Vec<Box<dyn Source>> {
    vec![]
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
}
