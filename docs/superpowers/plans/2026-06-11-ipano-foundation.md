# ipano 地基(P0–P1)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 ipano 的可用 MVP:查本机/指定 IP,并发抓 ip-api / ipinfo / ip.sb 三个免 key 源,合并基础信息,输出彩色终端报告与 JSON。

**Architecture:** 每个数据源拆成「纯解析函数 `parse(body)` + 异步抓取 `fetch()`」两层——解析逻辑用样例 JSON 单测,抓取用 httpmock 本地模拟,均不依赖真网络。源实现统一 `Source` trait,由 orchestrator 并发执行、单源失败不拖垮整体;`aggregate` 按源优先级把多源 `SourceData` 合并成一条 `MergedReport`,再交给 terminal / json 渲染器。

**Tech Stack:** Rust 2021 · tokio · reqwest(rustls + gzip)· serde / serde_json · clap(derive)· comfy-table · owo-colors · async-trait · thiserror。测试:httpmock。

> 范围:本计划只覆盖设计文档 P0–P1。ping0(P2)、ippure/ip.net.coffee(P3)、key 源(P4)、对比表/结论(P5)、流媒体/AI/邮局探测(P6–P7)、browser 后端(P8)、三网回程路由(P9)各自另起计划。

---

## File Structure

```
ipano/
  Cargo.toml                  依赖与元数据
  .gitignore                  忽略 target/
  src/
    main.rs                   入口:解析 CLI → 取 IP → 跑源 → 合并 → 渲染
    cli.rs                    clap Args 结构与 flag
    model.rs                  IpType / SourceData / SourceError / SourceResult
    fetch.rs                  共享 reqwest Client 构造器
    egress.rs                 本机出口 IP 探测(v4/v6)
    aggregate.rs              merge():多源 SourceData → MergedReport
    sources/
      mod.rs                  Source trait + run_all() 并发执行 + all_sources()
      ipapi.rs                ip-api.com
      ipinfo.rs               ipinfo.io
      ipsb.rs                 ip.sb
    render/
      mod.rs                  渲染分发
      terminal.rs             彩色终端报告
      json.rs                 JSON 输出
```

每个文件单一职责;源文件互相隔离,新增源 = 加一个文件 + 在 `sources/mod.rs::all_sources()` 注册。

---

## Task 1: 项目脚手架

**Files:**
- Create: `ipano/Cargo.toml`
- Create: `ipano/.gitignore`
- Create: `ipano/src/main.rs`

- [ ] **Step 1: 写 Cargo.toml**

```toml
[package]
name = "ipano"
version = "0.1.0"
edition = "2021"
description = "一站式 IP 全景聚合检测工具"

[dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "gzip", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
comfy-table = "7"
owo-colors = "4"
async-trait = "0.1"
thiserror = "1"
futures = "0.3"

[dev-dependencies]
httpmock = "0.7"
```

- [ ] **Step 2: 写 .gitignore**

```
/target
```

- [ ] **Step 3: 写最小 main.rs**

```rust
fn main() {
    println!("ipano {}", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 4: 验证编译运行**

Run: `cd ipano && cargo run`
Expected: 输出 `ipano 0.1.0`,编译成功无错误。

- [ ] **Step 5: 提交**

```bash
cd ipano && git add Cargo.toml .gitignore src/main.rs && git commit -m "chore: ipano 项目脚手架"
```

---

## Task 2: 核心数据模型

**Files:**
- Create: `ipano/src/model.rs`
- Modify: `ipano/src/main.rs`(加 `mod model;`)

- [ ] **Step 1: 写失败测试(在 model.rs 末尾)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sourcedata_default_is_empty() {
        let d = SourceData::new("ipapi");
        assert_eq!(d.source_id, "ipapi");
        assert!(d.asn.is_none());
        assert!(d.country.is_none());
    }

    #[test]
    fn iptype_serializes_to_lowercase_tag() {
        let j = serde_json::to_string(&IpType::Hosting).unwrap();
        assert_eq!(j, "\"hosting\"");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test model::`
Expected: 编译失败(`SourceData` / `IpType` 未定义)。

- [ ] **Step 3: 写实现(model.rs 顶部)**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IpType {
    Native,
    Broadcast,
    Hosting,
    Residential,
    Mobile,
    Business,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceData {
    pub source_id: String,
    pub asn: Option<u32>,
    pub as_org: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub timezone: Option<String>,
    pub rdns: Option<String>,
    pub ip_type: Option<IpType>,
    pub is_proxy: Option<bool>,
    pub is_vpn: Option<bool>,
    pub is_tor: Option<bool>,
    pub is_hosting: Option<bool>,
}

impl SourceData {
    pub fn new(source_id: &str) -> Self {
        SourceData { source_id: source_id.to_string(), ..Default::default() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("不可用: {0}")]
    Unavailable(String),
    #[error("触发限流")]
    RateLimited,
    #[error("需要 key: {0}")]
    NeedsKey(String),
    #[error("反爬挑战失败")]
    ChallengeFailed,
    #[error("超时")]
    Timeout,
    #[error("解析失败: {0}")]
    Parse(String),
}

pub type SourceResult = Result<SourceData, SourceError>;
```

- [ ] **Step 4: 注册模块并运行测试**

在 `src/main.rs` 顶部加 `mod model;`。
Run: `cargo test model::`
Expected: 2 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src/model.rs src/main.rs && git commit -m "feat: 核心数据模型 SourceData/IpType/SourceError"
```

---

## Task 3: 共享 HTTP 客户端

**Files:**
- Create: `ipano/src/fetch.rs`
- Modify: `ipano/src/main.rs`(加 `mod fetch;`)

- [ ] **Step 1: 写失败测试(fetch.rs 末尾)**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn builds_client_with_timeout() {
        // 仅验证构造不 panic
        let _c = super::build_client(5);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test fetch::`
Expected: 编译失败(`build_client` 未定义)。

- [ ] **Step 3: 写实现(fetch.rs 顶部)**

```rust
use std::time::Duration;
use reqwest::Client;

pub const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) ipano/0.1";

pub fn build_client(timeout_secs: u64) -> Client {
    Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .expect("构造 reqwest Client 失败")
}
```

- [ ] **Step 4: 运行测试通过**

在 `src/main.rs` 加 `mod fetch;`。
Run: `cargo test fetch::`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/fetch.rs src/main.rs && git commit -m "feat: 共享 reqwest 客户端构造器"
```

---

## Task 4: Source trait 与并发 run_all

**Files:**
- Create: `ipano/src/sources/mod.rs`
- Modify: `ipano/src/main.rs`(加 `mod sources;`)

- [ ] **Step 1: 写失败测试(sources/mod.rs 末尾)**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test sources::`
Expected: 编译失败(`Source` / `run_all` 未定义)。

- [ ] **Step 3: 写实现(sources/mod.rs 顶部)**

```rust
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

/// 默认启用的全部免 key 源
pub fn all_sources() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(ipapi::IpApi::default()),
        Box::new(ipinfo::IpInfo::default()),
        Box::new(ipsb::IpSb::default()),
    ]
}
```

> 注:Task 4 同时引用 `ipapi`/`ipinfo`/`ipsb` 模块,这些文件在 Task 5–8 创建。为先让 Task 4 单测通过,可临时注释掉 `pub mod ipapi/ipinfo/ipsb;` 和 `all_sources()` 两处,待 Task 5 起逐步放开。或先创建三个空模块文件占位。推荐:先放空占位文件(`touch src/sources/ipapi.rs` 等并加最简 `pub struct` 桩),实际实现在后续 Task 覆盖。

- [ ] **Step 4: 创建空占位避免编译断裂**

```bash
: > src/sources/ipapi.rs
: > src/sources/ipinfo.rs
: > src/sources/ipsb.rs
```

在三个占位文件各写一行最简桩,使 `all_sources()` 能编译:

`src/sources/ipapi.rs`:
```rust
#[derive(Default)] pub struct IpApi { pub base: String }
```
`src/sources/ipinfo.rs`:
```rust
#[derive(Default)] pub struct IpInfo { pub base: String }
```
`src/sources/ipsb.rs`:
```rust
#[derive(Default)] pub struct IpSb { pub base: String }
```

> 这些桩缺少 `impl Source`,因此 `all_sources()` 暂时无法 `Box::new` 它们为 `dyn Source`。为让 Task 4 独立通过,**先注释掉 `all_sources()` 函数体内三行 `Box::new(...)` 返回空 `vec![]`**,在 Task 9 恢复完整实现。

修正后的 `all_sources()`(Task 4 临时版):
```rust
pub fn all_sources() -> Vec<Box<dyn Source>> {
    vec![]   // 临时:Task 9 恢复为三个源
}
```

- [ ] **Step 5: 运行测试通过 + 提交**

在 `src/main.rs` 加 `mod sources;`。
Run: `cargo test sources::tests::run_all_collects_results`
Expected: PASS。

```bash
git add src/sources/ src/main.rs && git commit -m "feat: Source trait 与并发 run_all(源桩占位)"
```

---

## Task 5: ip-api 源 —— 解析层

**Files:**
- Modify: `ipano/src/sources/ipapi.rs`(替换 Task 4 的桩)

- [ ] **Step 1: 写失败测试(ipapi.rs 末尾)**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test ipapi::`
Expected: 编译失败(`parse` / `split_as` 未定义)。

- [ ] **Step 3: 写实现(替换 ipapi.rs 全部内容,顶部部分)**

```rust
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
```

> 注:`split_as` 对 "Cloudflare"(无 AS 前缀)返回 `(None, Some("Cloudflare"))`;对 "AS13335 …" 返回数字+组织。

- [ ] **Step 4: 运行测试通过**

Run: `cargo test ipapi::`
Expected: 3 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src/sources/ipapi.rs && git commit -m "feat: ip-api 解析层 + split_as"
```

---

## Task 6: ip-api 源 —— 抓取层

**Files:**
- Modify: `ipano/src/sources/ipapi.rs`(在 parse 之后追加 IpApi 实现)

- [ ] **Step 1: 写失败测试(追加到 ipapi.rs 的 tests 模块内)**

```rust
    #[tokio::test]
    async fn fetch_hits_endpoint_and_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/json/1.1.1.1");
            then.status(200).body(SAMPLE);
        });
        let src = IpApi { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.asn, Some(13335));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test ipapi::tests::fetch_hits_endpoint_and_parses`
Expected: 编译失败(`IpApi` 现为桩,缺 `fetch`)。

- [ ] **Step 3: 写实现(ipapi.rs,parse 之后追加;删除 Task 4 的桩 struct)**

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::SourceResult;

const FIELDS: &str = "status,message,country,regionName,city,lat,lon,timezone,isp,org,as,proxy,hosting,mobile,query";

pub struct IpApi {
    pub base: String,
}

impl Default for IpApi {
    fn default() -> Self {
        IpApi { base: "http://ip-api.com".to_string() }
    }
}

#[async_trait]
impl Source for IpApi {
    fn id(&self) -> &'static str { "ipapi" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/json/{}?fields={}", self.base, ip, FIELDS);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        let body = resp.text().await
            .map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}
```

> 注:把顶部 `use crate::model::{SourceData, SourceError, IpType};` 补上 `SourceResult` 或在 fetch 内用全路径;`SourceError` 已在顶部 import。

- [ ] **Step 4: 运行测试通过**

Run: `cargo test ipapi::`
Expected: 全部 PASS(解析 3 + 抓取 1)。

- [ ] **Step 5: 提交**

```bash
git add src/sources/ipapi.rs && git commit -m "feat: ip-api 抓取层 IpApi"
```

---

## Task 7: ipinfo 源(解析 + 抓取)

**Files:**
- Modify: `ipano/src/sources/ipinfo.rs`(替换 Task 4 的桩)

- [ ] **Step 1: 写失败测试(ipinfo.rs 末尾)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"ip":"1.1.1.1","hostname":"one.one.one.one",
        "city":"Los Angeles","region":"California","country":"US",
        "loc":"34.05,-118.24","org":"AS13335 Cloudflare, Inc.",
        "timezone":"America/Los_Angeles"}"#;

    #[test]
    fn parse_extracts_fields() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipinfo");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.lat, Some(34.05));
        assert_eq!(d.lon, Some(-118.24));
        assert_eq!(d.rdns.as_deref(), Some("one.one.one.one"));
    }

    #[tokio::test]
    async fn fetch_hits_endpoint() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/1.1.1.1/json");
            then.status(200).body(SAMPLE);
        });
        let src = IpInfo { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.city.as_deref(), Some("Los Angeles"));
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test ipinfo::`
Expected: 编译失败(`parse` / 完整 `IpInfo` 未定义)。

- [ ] **Step 3: 写实现(替换 ipinfo.rs 全部内容)**

```rust
use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::sources::ipapi::split_as;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    hostname: Option<String>,
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
    loc: Option<String>,
    org: Option<String>,
    timezone: Option<String>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipinfo");
    if let Some(o) = r.org {
        let (asn, org) = split_as(&o);
        d.asn = asn;
        d.as_org = org;
    }
    if let Some(loc) = r.loc {
        let mut it = loc.splitn(2, ',');
        d.lat = it.next().and_then(|v| v.trim().parse().ok());
        d.lon = it.next().and_then(|v| v.trim().parse().ok());
    }
    d.country = r.country;
    d.region = r.region;
    d.city = r.city;
    d.timezone = r.timezone;
    d.rdns = r.hostname;
    Ok(d)
}

pub struct IpInfo {
    pub base: String,
}

impl Default for IpInfo {
    fn default() -> Self {
        IpInfo { base: "https://ipinfo.io".to_string() }
    }
}

#[async_trait]
impl Source for IpInfo {
    fn id(&self) -> &'static str { "ipinfo" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/{}/json", self.base, ip);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}
```

- [ ] **Step 4: 运行测试通过**

Run: `cargo test ipinfo::`
Expected: 2 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src/sources/ipinfo.rs && git commit -m "feat: ipinfo 源(解析+抓取)"
```

---

## Task 8: ip.sb 源(解析 + 抓取)

**Files:**
- Modify: `ipano/src/sources/ipsb.rs`(替换 Task 4 的桩)

- [ ] **Step 1: 写失败测试(ipsb.rs 末尾)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"ip":"1.1.1.1","country":"United States","country_code":"US",
        "asn":13335,"asn_organization":"Cloudflare, Inc.","isp":"Cloudflare",
        "city":"Los Angeles","region":"California","latitude":34.05,"longitude":-118.24,
        "timezone":"America/Los_Angeles"}"#;

    #[test]
    fn parse_extracts_fields() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipsb");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.city.as_deref(), Some("Los Angeles"));
        assert_eq!(d.lat, Some(34.05));
    }

    #[tokio::test]
    async fn fetch_hits_endpoint() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/geoip/1.1.1.1");
            then.status(200).body(SAMPLE);
        });
        let src = IpSb { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.asn, Some(13335));
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test ipsb::`
Expected: 编译失败(`parse` / 完整 `IpSb` 未定义)。

- [ ] **Step 3: 写实现(替换 ipsb.rs 全部内容)**

```rust
use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct Resp {
    country: Option<String>,
    asn: Option<u32>,
    asn_organization: Option<String>,
    isp: Option<String>,
    city: Option<String>,
    region: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipsb");
    d.asn = r.asn;
    d.as_org = r.asn_organization;
    d.isp = r.isp;
    d.country = r.country;
    d.region = r.region;
    d.city = r.city;
    d.lat = r.latitude;
    d.lon = r.longitude;
    d.timezone = r.timezone;
    Ok(d)
}

pub struct IpSb {
    pub base: String,
}

impl Default for IpSb {
    fn default() -> Self {
        IpSb { base: "https://api.ip.sb".to_string() }
    }
}

#[async_trait]
impl Source for IpSb {
    fn id(&self) -> &'static str { "ipsb" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/geoip/{}", self.base, ip);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}
```

- [ ] **Step 4: 运行测试通过**

Run: `cargo test ipsb::`
Expected: 2 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src/sources/ipsb.rs && git commit -m "feat: ip.sb 源(解析+抓取)"
```

---

## Task 9: 恢复完整 all_sources()

**Files:**
- Modify: `ipano/src/sources/mod.rs`

- [ ] **Step 1: 写失败测试(追加到 sources/mod.rs 的 tests 模块)**

```rust
    #[test]
    fn all_sources_has_three() {
        let s = all_sources();
        let ids: Vec<&str> = s.iter().map(|x| x.id()).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"ipapi"));
        assert!(ids.contains(&"ipinfo"));
        assert!(ids.contains(&"ipsb"));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test sources::tests::all_sources_has_three`
Expected: FAIL(当前 `all_sources()` 返回空 `vec![]`,长度为 0)。

- [ ] **Step 3: 恢复实现(把 Task 4 临时版替换回真实三源)**

```rust
pub fn all_sources() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(ipapi::IpApi::default()),
        Box::new(ipinfo::IpInfo::default()),
        Box::new(ipsb::IpSb::default()),
    ]
}
```

- [ ] **Step 4: 运行测试通过**

Run: `cargo test sources::`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/sources/mod.rs && git commit -m "feat: 恢复完整 all_sources(三源)"
```

---

## Task 10: 聚合合并 merge()

**Files:**
- Create: `ipano/src/aggregate.rs`
- Modify: `ipano/src/main.rs`(加 `mod aggregate;`)

- [ ] **Step 1: 写失败测试(aggregate.rs 末尾)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceData;

    #[test]
    fn merge_picks_by_priority_and_records_status() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut ipinfo = SourceData::new("ipinfo");
        ipinfo.city = Some("LA-ipinfo".into());
        let mut ipsb = SourceData::new("ipsb");
        ipsb.city = Some("LA-ipsb".into());
        ipsb.asn = Some(13335);  // ipinfo 无 asn,应回落到 ipsb
        let results = vec![
            ("ipsb".to_string(), Ok(ipsb)),
            ("ipinfo".to_string(), Ok(ipinfo)),
            ("ipapi".to_string(), Err(crate::model::SourceError::Timeout)),
        ];
        let m = merge(ip, results);
        // 优先级 ipinfo > ipsb > ipapi:city 取 ipinfo
        assert_eq!(m.city.as_deref(), Some("LA-ipinfo"));
        // asn ipinfo 缺,回落 ipsb
        assert_eq!(m.asn, Some(13335));
        // 状态:3 条,ipapi 失败
        assert_eq!(m.sources.len(), 3);
        let failed = m.sources.iter().find(|s| s.id == "ipapi").unwrap();
        assert!(!failed.ok);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test aggregate::`
Expected: 编译失败(`merge` / `MergedReport` 未定义)。

- [ ] **Step 3: 写实现(aggregate.rs 顶部)**

```rust
use std::net::IpAddr;
use crate::model::{SourceData, SourceResult, IpType};

/// 源优先级(靠前更可信),合并基础字段时按此顺序取首个非空值
const PRIORITY: [&str; 3] = ["ipinfo", "ipsb", "ipapi"];

pub struct SourceStatus {
    pub id: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct MergedReport {
    pub ip: Option<IpAddr>,
    pub asn: Option<u32>,
    pub as_org: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub timezone: Option<String>,
    pub rdns: Option<String>,
    pub ip_type: Option<IpType>,
    pub is_proxy: Option<bool>,
    pub is_vpn: Option<bool>,
    pub is_tor: Option<bool>,
    pub is_hosting: Option<bool>,
    pub sources: Vec<SourceStatus>,
}

pub fn merge(ip: IpAddr, results: Vec<(String, SourceResult)>) -> MergedReport {
    let mut ok: Vec<SourceData> = Vec::new();
    let mut statuses: Vec<SourceStatus> = Vec::new();
    for (id, res) in results {
        match res {
            Ok(d) => {
                statuses.push(SourceStatus { id: id.clone(), ok: true, error: None });
                ok.push(d);
            }
            Err(e) => statuses.push(SourceStatus { id, ok: false, error: Some(e.to_string()) }),
        }
    }
    ok.sort_by_key(|d| PRIORITY.iter().position(|p| *p == d.source_id).unwrap_or(usize::MAX));

    let mut m = MergedReport { ip: Some(ip), sources: statuses, ..Default::default() };
    macro_rules! pick {
        ($field:ident) => {
            for d in &ok { if m.$field.is_none() && d.$field.is_some() { m.$field = d.$field.clone(); } }
        };
    }
    pick!(asn); pick!(as_org); pick!(isp); pick!(org);
    pick!(country); pick!(region); pick!(city);
    pick!(lat); pick!(lon); pick!(timezone); pick!(rdns);
    pick!(ip_type); pick!(is_proxy); pick!(is_vpn); pick!(is_tor); pick!(is_hosting);
    m
}
```

- [ ] **Step 4: 注册并运行测试**

在 `src/main.rs` 加 `mod aggregate;`。
Run: `cargo test aggregate::`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/aggregate.rs src/main.rs && git commit -m "feat: 聚合合并 merge + MergedReport"
```

---

## Task 11: 出口 IP 探测

**Files:**
- Create: `ipano/src/egress.rs`
- Modify: `ipano/src/main.rs`(加 `mod egress;`)

- [ ] **Step 1: 写失败测试(egress.rs 末尾)**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test egress::`
Expected: 编译失败(`majority` / `fetch_one` 未定义)。

- [ ] **Step 3: 写实现(egress.rs 顶部)**

```rust
use std::collections::HashMap;
use std::net::IpAddr;
use reqwest::Client;

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
```

- [ ] **Step 4: 注册并运行测试**

在 `src/main.rs` 加 `mod egress;`。
Run: `cargo test egress::`
Expected: 3 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src/egress.rs src/main.rs && git commit -m "feat: 出口 IP 探测 egress"
```

---

## Task 12: JSON 渲染器

**Files:**
- Create: `ipano/src/render/mod.rs`
- Create: `ipano/src/render/json.rs`
- Modify: `ipano/src/main.rs`(加 `mod render;`)

- [ ] **Step 1: 写失败测试(render/json.rs 末尾)**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test render::json`
Expected: 编译失败(`to_json` 未定义)。

- [ ] **Step 3: 写实现**

`render/mod.rs`:
```rust
pub mod json;
pub mod terminal;
```

`render/json.rs`:
```rust
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
```

- [ ] **Step 4: 注册并运行测试**

在 `src/main.rs` 加 `mod render;`。
Run: `cargo test render::json`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/render/mod.rs src/render/json.rs src/main.rs && git commit -m "feat: JSON 渲染器"
```

---

## Task 13: 终端渲染器

**Files:**
- Create: `ipano/src/render/terminal.rs`

- [ ] **Step 1: 写失败测试(render/terminal.rs 末尾)**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test render::terminal`
Expected: 编译失败(`render` 未定义)。

- [ ] **Step 3: 写实现(render/terminal.rs 顶部)**

```rust
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
```

- [ ] **Step 4: 运行测试通过**

Run: `cargo test render::terminal`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/render/terminal.rs && git commit -m "feat: 终端渲染器"
```

---

## Task 14: CLI 与主流程串联

**Files:**
- Create: `ipano/src/cli.rs`
- Modify: `ipano/src/main.rs`(改写为完整 orchestrator)

- [ ] **Step 1: 写 cli.rs**

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ipano", version, about = "一站式 IP 全景聚合检测工具")]
pub struct Args {
    /// 要查询的 IP;省略则查本机出口 IP
    pub ip: Option<String>,
    /// 仅 IPv4
    #[arg(short = '4', long)]
    pub four: bool,
    /// 仅 IPv6
    #[arg(short = '6', long)]
    pub six: bool,
    /// 输出 JSON
    #[arg(long)]
    pub json: bool,
    /// 关闭彩色
    #[arg(long)]
    pub no_color: bool,
    /// 单源超时(秒)
    #[arg(long, default_value_t = 8)]
    pub timeout: u64,
}
```

- [ ] **Step 2: 改写 main.rs(完整 orchestrator)**

```rust
mod model;
mod fetch;
mod egress;
mod aggregate;
mod sources;
mod render;
mod cli;

use std::net::IpAddr;
use clap::Parser;

#[tokio::main]
async fn main() {
    let args = cli::Args::parse();
    let client = fetch::build_client(args.timeout);

    let targets: Vec<IpAddr> = match &args.ip {
        Some(s) => match s.parse() {
            Ok(ip) => vec![ip],
            Err(_) => { eprintln!("无效 IP: {}", s); std::process::exit(2); }
        },
        None => {
            let (v4, v6) = egress::detect(&client).await;
            let mut v = Vec::new();
            if !args.six { if let Some(ip) = v4 { v.push(ip); } }
            if !args.four { if let Some(ip) = v6 { v.push(ip); } }
            if v.is_empty() { eprintln!("无法探测本机出口 IP"); std::process::exit(1); }
            v
        }
    };

    for ip in targets {
        let srcs = sources::all_sources();
        let results = sources::run_all(&client, ip, &srcs).await;
        let report = aggregate::merge(ip, results);
        if args.json {
            println!("{}", render::json::to_json(&report));
        } else {
            println!("{}", render::terminal::render(&report, args.no_color));
        }
    }
}
```

- [ ] **Step 3: 验证编译与单测全绿**

Run: `cargo test`
Expected: 所有测试 PASS。

Run: `cargo build`
Expected: 编译成功(警告可后续清理)。

- [ ] **Step 4: 真实冒烟测试(需联网)**

Run: `cargo run -- 1.1.1.1`
Expected: 打印 1.1.1.1 全景报告(AS13335 Cloudflare、源状态行 `✓ipapi ✓ipinfo ✓ipsb`)。

Run: `cargo run -- --json 8.8.8.8`
Expected: 合法 JSON,含 `"asn": 15169`。

Run: `cargo run`
Expected: 打印本机出口 IP 报告。

- [ ] **Step 5: 提交**

```bash
git add src/cli.rs src/main.rs && git commit -m "feat: CLI 与主流程串联,P0-P1 MVP 完成"
```

---

## Self-Review 记录

- **Spec 覆盖**:本计划对应设计文档 P0–P1(免 key 源 MVP)。P2–P9 明确划归各自计划,已在 Tech Stack 下声明,非遗漏。
- **占位符**:无 TODO/TBD;每个代码步骤均为完整可编译代码。Task 4 引入"桩占位 → 后续 Task 替换"的衔接处已显式说明,非占位符遗留。
- **类型一致性**:`SourceData`/`SourceError`/`SourceResult`(Task 2)、`Source` trait(Task 4)、`MergedReport`/`merge`(Task 10)、`SourceStatus.id/ok/error`(Task 10,被 Task 12/13 使用)签名一致;`split_as`(Task 5 定义,Task 7 复用)、`to_json`/`render` 与 Task 14 调用一致;渲染器测试用 `no_color=true` 断言纯文本。
- **构建顺序依赖**:Task 4 因 `all_sources()` 引用尚未实现的源,采用「空 `vec![]` 临时版 + 桩文件」,Task 9 恢复真实三源——避免任一 Task 结束时编译断裂。
- **后续衔接点**:`SourceData` 预留 `is_vpn`/`is_tor`/`ip_type` 等字段,供 P2+ ping0/欺诈库填充;risk/purity 评分字段待 P5 对比表时扩展。
```