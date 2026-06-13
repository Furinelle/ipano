# IP 质量多源扩充 阶段一(免 key 源)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 ipano 接入 6 个免 key IP 数据源 + 新质量字段 + `--raw` 逐源输出 + DNSBL 扩到 ~300,默认报告即更全(对标 securityCheck 阶段一)。

**Architecture:** 顺着现有 `trait Source`(`id()` + `async fn fetch()`)+ `all_sources()` 注册 + `aggregate::merge`(`pick!` 宏按优先级取值)。每源一个文件(parse 纯函数 + Source impl + httpmock 测试),不改核心流。新字段加进 `SourceData`/`MergedReport`,merge 对多源重合的布尔用多数决。

**Tech Stack:** Rust, tokio, reqwest(已有 rustls-tls+json), serde, async-trait, httpmock(dev), comfy-table。设计见 [`docs/superpowers/specs/2026-06-13-ipano-ipquality-multisource-design.md`](../specs/2026-06-13-ipano-ipquality-multisource-design.md)。

**约定:**
- 每个源任务先 `curl` 真实端点确认 JSON 字段名(端点见下),再据实际响应微调 `Resp` struct——计划给出的样本基于公开文档,字段名以实测为准。
- 每任务跑 `cargo test <模块>` 全绿后 commit。不 push(发布在 Task 11)。
- 仿照 `src/sources/ipapi.rs` 的结构(`Resp` + `parse()` + `impl Source` + `#[cfg(test)]` 含 SAMPLE/parse 断言/httpmock fetch 测试)。

---

## File Structure

| 文件 | 职责 | 改动 |
|---|---|---|
| `src/model.rs` | `SourceData` 加阶段一新字段 | 改 |
| `src/aggregate.rs` | `MergedReport` 加字段 + merge(pick! + 多数决 helper) | 改 |
| `src/sources/ipwhois.rs` | ipwhois.io 源 | 新建 |
| `src/sources/dbip.rs` | db-ip 免费 API 源 | 新建 |
| `src/sources/bigdatacloud.rs` | bigdatacloud 源 | 新建 |
| `src/sources/ipapiis.rs` | ipapi.is 源(ASN/公司滥用分) | 新建 |
| `src/sources/ipapicom.rs` | ipapi.co 源 | 新建 |
| `src/sources/ip2location.rs` | ip2location.io 源 | 新建 |
| `src/sources/mod.rs` | `all_sources()` 注册 6 新源 | 改 |
| `src/render/raw.rs` | `--raw` 逐源详表 | 新建 |
| `src/render/mod.rs` | 导出 raw 模块 | 改 |
| `src/cli.rs` | `--raw` 标志 | 改 |
| `src/main.rs` | `--raw` 时走 raw 渲染 | 改 |
| `src/probe/dnsbl.rs` | DNSBL 列表 12 → ~300 | 改 |
| `README.md`/`CHANGELOG.md`/`Cargo.toml` | 文档 + v0.17.0 | 改 |

---

## Task 1: 新质量字段(model + merge,TDD)

**Files:** Modify `src/model.rs`、`src/aggregate.rs`

- [ ] **Step 1: 写失败测试** — `src/model.rs` 的 `mod tests` 加:

```rust
#[test]
fn sourcedata_has_quality_fields() {
    let mut d = SourceData::new("ipapiis");
    d.usage_type = Some("hosting".into());
    d.company_type = Some("hosting".into());
    d.asn_abuse_score = Some(0.0131);
    d.company_abuse_score = Some(0.015);
    d.is_datacenter = Some(true);
    assert_eq!(d.usage_type.as_deref(), Some("hosting"));
    assert_eq!(d.asn_abuse_score, Some(0.0131));
    assert_eq!(d.is_datacenter, Some(true));
}
```

- [ ] **Step 2: 跑测试确认失败** — Run: `cargo test model::tests::sourcedata_has_quality 2>&1 | tail -8` → Expected: 编译错(字段未定义)

- [ ] **Step 3: 加字段** — 在 `src/model.rs` 的 `SourceData` 末尾(`ipqs_score` 后)加:

```rust
    // —— 阶段一 多源质量字段 ——
    pub usage_type: Option<String>,       // Commercial/hosting/business/ISP
    pub company_type: Option<String>,     // isp/hosting/business
    pub asn_abuse_score: Option<f64>,     // ipapi.is ASN 滥用分
    pub company_abuse_score: Option<f64>, // ipapi.is 公司滥用分
    pub is_datacenter: Option<bool>,
```

并在 `src/aggregate.rs` 的 `MergedReport` 末尾(`ipqs_score` 后、`sources` 前)加同样 5 个字段。

- [ ] **Step 4: merge 取值** — 在 `src/aggregate.rs` 的 `pick!(ipqs_score);` 后加:

```rust
    pick!(usage_type); pick!(company_type);
    pick!(asn_abuse_score); pick!(company_abuse_score);
```

`is_datacenter` 用多数决(多个源提供)——在 `m.raw = ok;` 前加:

```rust
    m.is_datacenter = majority_bool(&ok, |d| d.is_datacenter);
```

并在 `aggregate.rs` 顶部(`merge` 前)加 helper:

```rust
/// 多数决:多源布尔取多数;平票或无值返回 None。少数派由渲染层另行展示。
fn majority_bool(ok: &[SourceData], f: impl Fn(&SourceData) -> Option<bool>) -> Option<bool> {
    let (mut t, mut fa) = (0u32, 0u32);
    for d in ok { match f(d) { Some(true) => t += 1, Some(false) => fa += 1, None => {} } }
    if t == 0 && fa == 0 { None } else if t > fa { Some(true) } else if fa > t { Some(false) } else { Some(false) }
}
```

- [ ] **Step 5: merge 多数决测试** — `src/aggregate.rs` 的 `mod tests` 加:

```rust
#[test]
fn merge_datacenter_majority() {
    let ip = "1.1.1.1".parse().unwrap();
    let mk = |id: &str, dc: bool| { let mut d = SourceData::new(id); d.is_datacenter = Some(dc); d };
    let m = merge(ip, vec![
        ("bdc".into(), Ok(mk("bdc", true))),
        ("ipapiis".into(), Ok(mk("ipapiis", true))),
        ("ip2loc".into(), Ok(mk("ip2loc", false))),
    ]);
    assert_eq!(m.is_datacenter, Some(true)); // 2:1 多数
}
```

- [ ] **Step 6: 跑测试** — Run: `cargo test model:: aggregate:: 2>&1 | tail -6` → Expected: PASS

- [ ] **Step 7: Commit** — `git add src/model.rs src/aggregate.rs && git commit -m "feat(model): 多源质量字段(使用/公司类型/ASN-公司滥用分/数据中心)+ 多数决合并"`

---

## Task 2: ipwhois.io 源(免key)

**Files:** Create `src/sources/ipwhois.rs`;Modify `src/sources/mod.rs`(Task 8 统一注册)

端点:`http://ipwho.is/<ip>`(免 key)。先 `curl http://ipwho.is/1.1.1.1` 确认字段。

- [ ] **Step 1: 写源 + 测试** — 新建 `src/sources/ipwhois.rs`:

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    success: bool,
    message: Option<String>,
    country: Option<String>,
    region: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    connection: Option<Conn>,
    timezone: Option<Tz>,
}
#[derive(Deserialize)]
struct Conn { asn: Option<u32>, isp: Option<String>, org: Option<String> }
#[derive(Deserialize)]
struct Tz { id: Option<String> }

pub struct IpWhois { pub base: String }
impl Default for IpWhois { fn default() -> Self { IpWhois { base: "http://ipwho.is".into() } } }

#[async_trait]
impl Source for IpWhois {
    fn id(&self) -> &'static str { "ipwhois" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/{}", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if !r.success { return Err(SourceError::Unavailable(r.message.unwrap_or_default())); }
    let mut d = SourceData::new("ipwhois");
    d.country = r.country; d.region = r.region; d.city = r.city;
    d.lat = r.latitude; d.lon = r.longitude;
    d.timezone = r.timezone.and_then(|t| t.id);
    if let Some(c) = r.connection { d.asn = c.asn; d.isp = c.isp; d.org = c.org; }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"success":true,"country":"United States","region":"California","city":"Los Angeles","latitude":34.05,"longitude":-118.24,"connection":{"asn":13335,"isp":"Cloudflare","org":"Cloudflare Inc"},"timezone":{"id":"America/Los_Angeles"}}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipwhois");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.city.as_deref(), Some("Los Angeles"));
        assert_eq!(d.timezone.as_deref(), Some("America/Los_Angeles"));
    }
    #[test]
    fn parse_fail() {
        assert!(parse(r#"{"success":false,"message":"reserved"}"#).is_err());
    }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.path("/1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = IpWhois { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.asn, Some(13335));
    }
}
```

- [ ] **Step 2: 加模块声明** — `src/sources/mod.rs` 顶部加 `pub mod ipwhois;`(与现有 `pub mod ipapi;` 同处)

- [ ] **Step 3: curl 核实 + 跑测试** — Run: `curl -s http://ipwho.is/1.1.1.1 | head -c 400; echo; cargo test sources::ipwhois 2>&1 | tail -6` → Expected: 真实 JSON 字段与 SAMPLE 一致(不一致则改 `Resp`),测试 PASS

- [ ] **Step 4: Commit** — `git add src/sources/ipwhois.rs src/sources/mod.rs && git commit -m "feat(sources): 接入 ipwhois.io(免key)"`

---

## Task 3: db-ip 免费 API 源

**Files:** Create `src/sources/dbip.rs`;Modify `src/sources/mod.rs`

端点:`https://api.db-ip.com/v2/free/<ip>`(免 key)。先 `curl https://api.db-ip.com/v2/free/1.1.1.1`。

- [ ] **Step 1: 写源 + 测试** — 新建 `src/sources/dbip.rs`(仿 ipwhois 结构):

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    #[serde(rename = "countryName")] country_name: Option<String>,
    #[serde(rename = "stateProv")] state_prov: Option<String>,
    city: Option<String>,
    error: Option<String>,
}

pub struct DbIp { pub base: String }
impl Default for DbIp { fn default() -> Self { DbIp { base: "https://api.db-ip.com/v2/free".into() } } }

#[async_trait]
impl Source for DbIp {
    fn id(&self) -> &'static str { "dbip" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/{}", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if let Some(e) = r.error { return Err(SourceError::Unavailable(e)); }
    let mut d = SourceData::new("dbip");
    d.country = r.country_name; d.region = r.state_prov; d.city = r.city;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"ipAddress":"1.1.1.1","continentCode":"OC","countryName":"Australia","stateProv":"Queensland","city":"Brisbane"}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "dbip");
        assert_eq!(d.country.as_deref(), Some("Australia"));
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
    }
    #[test]
    fn parse_error() { assert!(parse(r#"{"error":"quota exceeded"}"#).is_err()); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.path("/1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = DbIp { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
    }
}
```

- [ ] **Step 2: 模块声明** — `src/sources/mod.rs` 加 `pub mod dbip;`

- [ ] **Step 3: curl 核实 + 测试** — Run: `curl -s https://api.db-ip.com/v2/free/1.1.1.1 | head -c 400; echo; cargo test sources::dbip 2>&1 | tail -6` → Expected: PASS(字段不符则改 `Resp`)

- [ ] **Step 4: Commit** — `git add src/sources/dbip.rs src/sources/mod.rs && git commit -m "feat(sources): 接入 db-ip 免费 API"`

---

## Task 4: bigdatacloud 源

**Files:** Create `src/sources/bigdatacloud.rs`;Modify `src/sources/mod.rs`

端点:`https://api.bigdatacloud.net/data/ip-geolocation-full?ip=<ip>&localityLanguage=en`(免 key tier;部分字段需 key,免key部分含国家/网络)。先 curl 核实免key可得字段(若免key不返回 `hazardReport`/`isDataCenter`,则该源只取地理,`is_datacenter` 留空)。

- [ ] **Step 1: 写源 + 测试** — 新建 `src/sources/bigdatacloud.rs`:

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    country: Option<Country>,
    location: Option<Loc>,
    network: Option<Net>,
}
#[derive(Deserialize)]
struct Country { name: Option<String> }
#[derive(Deserialize)]
struct Loc { city: Option<String>, #[serde(rename = "principalSubdivision")] sub: Option<String> }
#[derive(Deserialize)]
struct Net { #[serde(rename = "isDataCenter")] is_data_center: Option<bool> }

pub struct BigDataCloud { pub base: String }
impl Default for BigDataCloud { fn default() -> Self { BigDataCloud { base: "https://api.bigdatacloud.net/data/ip-geolocation-full".into() } } }

#[async_trait]
impl Source for BigDataCloud {
    fn id(&self) -> &'static str { "bdc" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}?ip={}&localityLanguage=en", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("bdc");
    if let Some(c) = r.country { d.country = c.name; }
    if let Some(l) = r.location { d.city = l.city; d.region = l.sub; }
    if let Some(n) = r.network { d.is_datacenter = n.is_data_center; }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"country":{"name":"Australia"},"location":{"city":"Brisbane","principalSubdivision":"Queensland"},"network":{"isDataCenter":true}}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "bdc");
        assert_eq!(d.country.as_deref(), Some("Australia"));
        assert_eq!(d.is_datacenter, Some(true));
    }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.query_param("ip", "1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = BigDataCloud { base: s.url("/data/ip-geolocation-full") }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.is_datacenter, Some(true));
    }
}
```

- [ ] **Step 2: 模块声明** — `src/sources/mod.rs` 加 `pub mod bigdatacloud;`

- [ ] **Step 3: curl 核实 + 测试** — Run: `curl -s "https://api.bigdatacloud.net/data/ip-geolocation-full?ip=1.1.1.1&localityLanguage=en" | head -c 600; echo; cargo test sources::bigdatacloud 2>&1 | tail -6` → Expected: PASS(若免key不含 network.isDataCenter,删该字段映射,`is_datacenter` 留空)

- [ ] **Step 4: Commit** — `git add src/sources/bigdatacloud.rs src/sources/mod.rs && git commit -m "feat(sources): 接入 bigdatacloud(免key)"`

---

## Task 5: ipapi.is 源(ASN/公司滥用分,高价值)

**Files:** Create `src/sources/ipapiis.rs`;Modify `src/sources/mod.rs`

端点:`https://api.ipapi.is/?q=<ip>`(免 key 限额)。先 `curl "https://api.ipapi.is/?q=1.1.1.1"`。

- [ ] **Step 1: 写源 + 测试** — 新建 `src/sources/ipapiis.rs`:

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    is_datacenter: Option<bool>,
    is_vpn: Option<bool>,
    is_proxy: Option<bool>,
    is_abuser: Option<bool>,
    asn: Option<Asn>,
    company: Option<Company>,
}
#[derive(Deserialize)]
struct Asn { asn: Option<u32>, abuser_score: Option<String>, org: Option<String> }
#[derive(Deserialize)]
struct Company { #[serde(rename = "type")] ctype: Option<String>, abuser_score: Option<String> }

/// "0.0131 (Elevated)" → 0.0131
fn lead_f64(s: &str) -> Option<f64> { s.trim().split_whitespace().next()?.parse().ok() }

pub struct IpApiIs { pub base: String }
impl Default for IpApiIs { fn default() -> Self { IpApiIs { base: "https://api.ipapi.is".into() } } }

#[async_trait]
impl Source for IpApiIs {
    fn id(&self) -> &'static str { "ipapiis" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/?q={}", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipapiis");
    d.is_datacenter = r.is_datacenter; d.is_vpn = r.is_vpn; d.is_proxy = r.is_proxy; d.is_abuser = r.is_abuser;
    if let Some(a) = r.asn { d.asn = a.asn; d.as_org = a.org; d.asn_abuse_score = a.abuser_score.as_deref().and_then(lead_f64); }
    if let Some(c) = r.company { d.company_type = c.ctype; d.company_abuse_score = c.abuser_score.as_deref().and_then(lead_f64); }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"is_datacenter":true,"is_vpn":false,"is_proxy":false,"is_abuser":false,"asn":{"asn":13335,"abuser_score":"0.0131 (Elevated)","org":"Cloudflare"},"company":{"type":"hosting","abuser_score":"0.015 (Elevated)"}}"#;
    #[test]
    fn parse_extracts_abuse_scores() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipapiis");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.asn_abuse_score, Some(0.0131));
        assert_eq!(d.company_abuse_score, Some(0.015));
        assert_eq!(d.company_type.as_deref(), Some("hosting"));
        assert_eq!(d.is_datacenter, Some(true));
    }
    #[test]
    fn lead_f64_parses() { assert_eq!(lead_f64("0.0131 (Elevated)"), Some(0.0131)); assert_eq!(lead_f64("x"), None); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.query_param("q", "1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = IpApiIs { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.asn_abuse_score, Some(0.0131));
    }
}
```

- [ ] **Step 2: 模块声明** — `src/sources/mod.rs` 加 `pub mod ipapiis;`

- [ ] **Step 3: curl 核实 + 测试** — Run: `curl -s "https://api.ipapi.is/?q=1.1.1.1" | head -c 800; echo; cargo test sources::ipapiis 2>&1 | tail -6` → Expected: PASS(确认 `asn.abuser_score`/`company.abuser_score` 字段名与样本一致)

- [ ] **Step 4: Commit** — `git add src/sources/ipapiis.rs src/sources/mod.rs && git commit -m "feat(sources): 接入 ipapi.is(ASN/公司滥用分,免key限额)"`

---

## Task 6: ipapi.co 源

**Files:** Create `src/sources/ipapicom.rs`;Modify `src/sources/mod.rs`

端点:`https://ipapi.co/<ip>/json/`(免 key 限额)。先 `curl https://ipapi.co/1.1.1.1/json/`。

- [ ] **Step 1: 写源 + 测试** — 新建 `src/sources/ipapicom.rs`:

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    error: Option<bool>,
    reason: Option<String>,
    country_name: Option<String>,
    region: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
    asn: Option<String>,   // "AS13335"
    org: Option<String>,
}

pub struct IpApiCom { pub base: String }
impl Default for IpApiCom { fn default() -> Self { IpApiCom { base: "https://ipapi.co".into() } } }

#[async_trait]
impl Source for IpApiCom {
    fn id(&self) -> &'static str { "ipapicom" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/{}/json/", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if r.error == Some(true) { return Err(SourceError::Unavailable(r.reason.unwrap_or_default())); }
    let mut d = SourceData::new("ipapicom");
    d.country = r.country_name; d.region = r.region; d.city = r.city;
    d.lat = r.latitude; d.lon = r.longitude; d.timezone = r.timezone; d.org = r.org;
    if let Some(a) = r.asn { d.asn = a.trim_start_matches("AS").parse().ok(); }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"country_name":"Australia","region":"Queensland","city":"Brisbane","latitude":-27.46,"longitude":153.02,"timezone":"Australia/Brisbane","asn":"AS13335","org":"CLOUDFLARENET"}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ipapicom");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.city.as_deref(), Some("Brisbane"));
    }
    #[test]
    fn parse_error() { assert!(parse(r#"{"error":true,"reason":"RateLimited"}"#).is_err()); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.path("/1.1.1.1/json/"); then.status(200).body(SAMPLE); });
        let d = IpApiCom { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.asn, Some(13335));
    }
}
```

- [ ] **Step 2: 模块声明** — `src/sources/mod.rs` 加 `pub mod ipapicom;`

- [ ] **Step 3: curl 核实 + 测试** — Run: `curl -s https://ipapi.co/1.1.1.1/json/ | head -c 500; echo; cargo test sources::ipapicom 2>&1 | tail -6` → Expected: PASS

- [ ] **Step 4: Commit** — `git add src/sources/ipapicom.rs src/sources/mod.rs && git commit -m "feat(sources): 接入 ipapi.co(免key限额)"`

---

## Task 7: ip2location.io 源

**Files:** Create `src/sources/ip2location.rs`;Modify `src/sources/mod.rs`

端点:`https://api.ip2location.io/?ip=<ip>`(免 key 限额,无 key 返回基础字段)。先 `curl "https://api.ip2location.io/?ip=1.1.1.1"`。

- [ ] **Step 1: 写源 + 测试** — 新建 `src/sources/ip2location.rs`:

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::model::{SourceData, SourceError, SourceResult};
use crate::sources::Source;

#[derive(Deserialize)]
struct Resp {
    error: Option<serde_json::Value>,
    country_name: Option<String>,
    region_name: Option<String>,
    city_name: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    time_zone: Option<String>,
    #[serde(rename = "as")] as_name: Option<String>,
    asn: Option<String>,
    is_proxy: Option<bool>,
    usage_type: Option<String>,
}

pub struct Ip2Location { pub base: String }
impl Default for Ip2Location { fn default() -> Self { Ip2Location { base: "https://api.ip2location.io".into() } } }

#[async_trait]
impl Source for Ip2Location {
    fn id(&self) -> &'static str { "ip2loc" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/?ip={}", self.base, ip);
        let body = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
            .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    if r.error.is_some() { return Err(SourceError::Unavailable("ip2location error".into())); }
    let mut d = SourceData::new("ip2loc");
    d.country = r.country_name; d.region = r.region_name; d.city = r.city_name;
    d.lat = r.latitude; d.lon = r.longitude; d.timezone = r.time_zone;
    d.is_proxy = r.is_proxy; d.usage_type = r.usage_type;
    d.asn = r.asn.and_then(|s| s.parse().ok());
    if let Some(a) = r.as_name { d.as_org = Some(a); }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = r#"{"country_name":"Australia","region_name":"Queensland","city_name":"Brisbane","latitude":-27.46,"longitude":153.02,"time_zone":"+10:00","asn":"13335","as":"Cloudflare Inc","is_proxy":false,"usage_type":"DCH"}"#;
    #[test]
    fn parse_extracts() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "ip2loc");
        assert_eq!(d.usage_type.as_deref(), Some("DCH"));
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.is_proxy, Some(false));
    }
    #[test]
    fn parse_error() { assert!(parse(r#"{"error":{"error_code":10001}}"#).is_err()); }
    #[tokio::test]
    async fn fetch_works() {
        let s = httpmock::MockServer::start();
        s.mock(|when, then| { when.query_param("ip", "1.1.1.1"); then.status(200).body(SAMPLE); });
        let d = Ip2Location { base: s.base_url() }.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.usage_type.as_deref(), Some("DCH"));
    }
}
```

- [ ] **Step 2: 模块声明** — `src/sources/mod.rs` 加 `pub mod ip2location;`

- [ ] **Step 3: curl 核实 + 测试** — Run: `curl -s "https://api.ip2location.io/?ip=1.1.1.1" | head -c 600; echo; cargo test sources::ip2location 2>&1 | tail -6` → Expected: PASS

- [ ] **Step 4: Commit** — `git add src/sources/ip2location.rs src/sources/mod.rs && git commit -m "feat(sources): 接入 ip2location.io(免key限额)"`

---

## Task 8: 注册 6 新源进 all_sources

**Files:** Modify `src/sources/mod.rs`

- [ ] **Step 1: 写失败测试** — `src/sources/mod.rs` 的 `mod tests` 加(仿现有 `all_sources_includes_netcoffee`):

```rust
#[test]
fn all_sources_includes_phase1() {
    let s = all_sources(None);
    let ids: Vec<&str> = s.iter().map(|x| x.id()).collect();
    for id in ["ipwhois", "dbip", "bdc", "ipapiis", "ipapicom", "ip2loc"] {
        assert!(ids.contains(&id), "缺少源 {id}");
    }
}
```

- [ ] **Step 2: 跑测试确认失败** — Run: `cargo test sources::tests::all_sources_includes_phase1 2>&1 | tail -6` → Expected: FAIL(源未注册)

- [ ] **Step 3: 注册** — 在 `src/sources/mod.rs` 的 `all_sources()` 的 `Box::new(ipqs::Ipqs::default()),` 后加:

```rust
        Box::new(ipwhois::IpWhois::default()),
        Box::new(dbip::DbIp::default()),
        Box::new(bigdatacloud::BigDataCloud::default()),
        Box::new(ipapiis::IpApiIs::default()),
        Box::new(ipapicom::IpApiCom::default()),
        Box::new(ip2location::Ip2Location::default()),
```

- [ ] **Step 4: 跑测试** — Run: `cargo test sources:: 2>&1 | tail -6` → Expected: PASS

- [ ] **Step 5: Commit** — `git add src/sources/mod.rs && git commit -m "feat(sources): all_sources 注册 6 个免key源"`

---

## Task 9: `--raw` 逐源详表渲染

**Files:** Create `src/render/raw.rs`;Modify `src/render/mod.rs`、`src/cli.rs`、`src/main.rs`

- [ ] **Step 1: 写失败测试** — 新建 `src/render/raw.rs`:

```rust
use crate::aggregate::MergedReport;

/// securityCheck 同款逐字段逐源 [源缩写] 详表(纯文本)
pub fn render(report: &MergedReport) -> String {
    let mut out = String::from("═══ IP 质量检测(逐源) ═══\n");
    // 每个字段:遍历各源,列出有值的 (值 [源])
    macro_rules! line {
        ($label:expr, $field:ident, $fmt:expr) => {{
            let parts: Vec<String> = report.raw.iter()
                .filter_map(|d| d.$field.as_ref().map(|v| format!("{} [{}]", $fmt(v), d.source_id)))
                .collect();
            if !parts.is_empty() { out.push_str(&format!("{}: {}\n", $label, parts.join("  "))); }
        }};
    }
    line!("国家", country, |v: &String| v.clone());
    line!("使用类型", usage_type, |v: &String| v.clone());
    line!("公司类型", company_type, |v: &String| v.clone());
    line!("ASN滥用分", asn_abuse_score, |v: &f64| format!("{v}"));
    line!("公司滥用分", company_abuse_score, |v: &f64| format!("{v}"));
    line!("是否代理", is_proxy, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否VPN", is_vpn, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否数据中心", is_datacenter, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SourceData;
    #[test]
    fn raw_lists_per_source() {
        let mut a = SourceData::new("ipapiis"); a.is_proxy = Some(true); a.asn_abuse_score = Some(0.0131);
        let mut b = SourceData::new("ip2loc"); b.is_proxy = Some(false); b.usage_type = Some("DCH".into());
        let report = MergedReport { raw: vec![a, b], ..Default::default() };
        let s = render(&report);
        assert!(s.contains("是否代理"));
        assert!(s.contains("Yes [ipapiis]"));
        assert!(s.contains("No [ip2loc]"));
        assert!(s.contains("0.0131 [ipapiis]"));
        assert!(s.contains("DCH [ip2loc]"));
    }
}
```

- [ ] **Step 2: 跑测试确认失败** — Run: `cargo test render::raw 2>&1 | tail -8` → Expected: FAIL(模块未声明)

- [ ] **Step 3: 声明模块 + CLI 标志 + 调度** —
  - `src/render/mod.rs` 加 `pub mod raw;`
  - `src/cli.rs` 加字段:`/// 逐源原始详表(securityCheck 同款,每字段标来源)\n#[arg(long)]\n pub raw: bool,`
  - `src/main.rs`:在 JSON 分支判断后、终端渲染处,`if args.raw { print!("{}", render::raw::render(&report)); }`(放在终端主报告之后、各 probe 之前;`--raw` 与 `--json`/`--markdown` 互斥时以 raw 优先打印质量逐源块——具体:在 `else`(非 json)块内,主报告 `print!` 之后加 `if args.raw { print!("\n{}", render::raw::render(&report)); }`)

- [ ] **Step 4: 跑测试 + 冒烟** — Run: `cargo build 2>&1 | tail -3 && cargo test render:: 2>&1 | tail -6 && ./target/debug/ipano --raw 1.1.1.1 2>&1 | sed -n '/逐源/,$p' | head -15` → Expected: 编译通过,测试 PASS,冒烟出逐源块

- [ ] **Step 5: Commit** — `git add src/render/raw.rs src/render/mod.rs src/cli.rs src/main.rs && git commit -m "feat(render): --raw 逐源 [源缩写] 详表"`

---

## Task 10: DNSBL 列表 12 → ~300

**Files:** Modify `src/probe/dnsbl.rs`

- [ ] **Step 1: 看现状** — Run: `grep -n "const \|&\[\|zen.spamhaus\|fn list\|LISTS\|DNSBLS" src/probe/dnsbl.rs | head` → 确认黑名单列表常量名与结构

- [ ] **Step 2: 写失败测试** — `src/probe/dnsbl.rs` 的 `mod tests` 加:

```rust
#[test]
fn dnsbl_list_is_large_and_unique() {
    let l = dnsbl_zones(); // 现有列表函数名(Step 1 确认,若不同则改此处)
    assert!(l.len() >= 200, "DNSBL 列表应 >= 200, got {}", l.len());
    let mut v: Vec<&str> = l.to_vec();
    let n = v.len(); v.sort_unstable(); v.dedup();
    assert_eq!(v.len(), n, "DNSBL 列表不应有重复");
}
```

> 注:`dnsbl_zones()` 用 Step 1 实测的真实函数/常量名替换。

- [ ] **Step 3: 扩充列表** — 把 `src/probe/dnsbl.rs` 的列表常量替换为 ~300 条 DNSBL 域名(取自 multirbl.valli.org 公布集合;按 Step 1 的现有结构 `&[&str]` 扩充,保留现有 12 条 + 新增至 ~300)。完整列表见 multirbl.valli.org/list/ 的 active 集合,内置为静态数组。**实现期**:从 `https://multirbl.valli.org/list/` 抓取 active DNSBL 域名,去重后填入数组(或用社区维护的列表如 `https://raw.githubusercontent.com/oneclickvirt/securityCheck` 若公开)。

> 实现说明:这一步需联网取列表一次,人工整理进静态数组。若无法获取完整 300,以能确认有效的子集(目标 ≥200)填入,并在注释标注来源与日期。每条 4s 超时、全局并发(现有 `check_all` 逻辑不变,只是列表变长)。

- [ ] **Step 4: 跑测试** — Run: `cargo test dnsbl 2>&1 | tail -6` → Expected: PASS(列表 ≥200 且无重复)

- [ ] **Step 5: Commit** — `git add src/probe/dnsbl.rs && git commit -m "feat(dnsbl): 黑名单列表 12 扩到 ~300(multirbl 集)"`

---

## Task 11: 文档 + 版本 0.17.0

**Files:** Modify `README.md`、`CHANGELOG.md`、`Cargo.toml`

- [ ] **Step 1: 版本** — `Cargo.toml` `version = "0.16.2"` → `version = "0.17.0"`

- [ ] **Step 2: CHANGELOG** — 顶部加:

```markdown
## [0.17.0] - 2026-06-13

### 新增

- **IP 质量多源扩充 阶段一(免key源)**:默认报告新接入 6 个免key源 —— ipwhois.io / db-ip / bigdatacloud / **ipapi.is(ASN/公司滥用分)** / ipapi.co / ip2location.io。新字段:使用类型、公司类型、ASN/公司滥用分、是否数据中心。
- **`--raw` 逐源详表**:securityCheck 同款,每字段列出各源取值 + `[源缩写]` 标注,直观看源间分歧。
- **DNSBL 扩到 ~300**:`--dnsbl` 黑名单从 12 条扩为 ~300 条(multirbl 集),并发查询。
- 多源布尔字段(是否数据中心等)合并改为多数决。

> 阶段二(virustotal/ipdata/scamalytics/ipregistry/cloudflare 等 keyed 源)见后续 v0.18.0。

[0.17.0]: https://github.com/Furinelle/ipano/releases/tag/v0.17.0
```

- [ ] **Step 3: README** — `## 功能` 段加一条「多源 IP 质量(免key 6 源 + ASN/公司滥用分,`--raw` 看逐源)」;`## 用法` 加 `ipano --raw 1.1.1.1   # 逐源原始详表`;源列表(架构图)补 6 源。

- [ ] **Step 4: 全量验证** — Run: `cargo build --release 2>&1 | tail -3 && cargo test 2>&1 | tail -4` → Expected: release 编译通过,全测试绿

- [ ] **Step 5: Commit** — `git add README.md CHANGELOG.md Cargo.toml Cargo.lock && git commit -m "docs(sources): IP 质量多源阶段一文档 + v0.17.0"`

---

## Task 12: 联网冒烟(人工)

- [ ] **Step 1: 默认报告含新源** — Run: `cargo run -- 1.1.1.1 2>&1 | tail -30` → Expected: 源状态行含 ipwhois/dbip/bdc/ipapiis/ipapicom/ip2loc;报告含使用类型/滥用分等(限额触发的源标失败属正常)
- [ ] **Step 2: --raw** — Run: `cargo run -- --raw 1.1.1.1 2>&1 | sed -n '/逐源/,$p'` → Expected: 逐字段 `值 [源缩写]` 详表
- [ ] **Step 3: JSON** — Run: `cargo run -- --json 1.1.1.1 2>&1 | python3 -m json.tool | grep -E 'usage_type|asn_abuse|company_abuse|is_datacenter'` → Expected: 新字段出现

---

## Self-Review(对照 spec)

- **Spec 覆盖(阶段一)**:6 免key源(Task 2-7)✓ / 新字段(Task 1)✓ / 并进默认报告(Task 8)✓ / 默认合并 + 多数决(Task 1)✓ / `--raw` 逐源(Task 9)✓ / DNSBL ~300(Task 10)✓ / 源名缩写标注(Task 9 用 source_id)✓ / 无key跳过(沿用现有 run_all,免key源无此问题)✓ / JSON 新字段(Task 1 加在 SourceData/MergedReport,json.rs 自动序列化 raw[] + 顶层)✓。阶段二 keyed 源不在本计划(spec 已划归 v0.18.0)。
- **占位符**:各源 `Resp` 基于公开文档,每个源 Task 的 Step 3 用 `curl` 实测校正字段名(非占位,是必要的实现校验步骤)。DNSBL 列表 Task 10 Step 3 需联网取 ~300 条整理进静态数组(已给来源与降级阈值 ≥200)。
- **类型一致**:新字段 `usage_type/company_type/asn_abuse_score/company_abuse_score/is_datacenter` 在 model(Task1)/各源(Task2-7)/merge(Task1)/raw(Task9) 命名一致;源 id 缩写 `ipwhois/dbip/bdc/ipapiis/ipapicom/ip2loc` 在源定义(Task2-7)/注册(Task8)/测试 一致。`majority_bool` 定义于 Task1 用于 Task1。
