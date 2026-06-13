# IP 质量多源扩充 阶段二(keyed 源)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在阶段一(免key 6 源)基础上接入需要 API key 的高价值源,补齐 VirusTotal 黑名单统计、Cloudflare Radar 人机流量/设备分布、ipdata/ipregistry 的云/中继/匿名判定等独有字段,使 ipano 默认报告达到 securityCheck 级别完整度。

**Architecture:** 沿用 ipano 既有 `trait Source` + `all_sources()` 注册 + `aggregate::merge` 合并流。每个 keyed 源 = 一个实现 `Source` 的文件,无 key 时返回 `SourceError::NeedsKey` 被 `run_all` 收集为「跳过」状态(与现有 AbuseIPDB/IPQS 完全一致,绝不伪造数据)。新字段先加进 `SourceData` 与 `MergedReport`,再由各源填充、`merge` 合并、三个 render 后端展示。

**Tech Stack:** Rust 2021 · `reqwest`(异步 HTTP)· `serde`/`serde_json`(解析)· `async-trait` · `tokio` · `httpmock`(源单测)· `comfy-table`(终端表)。

---

## 端点核实结论(2026-06-13 计划期实测)

实现前已对 spec 阶段二清单逐个核实(curl / 官方文档 / GitHub),结论如下。**本计划据此确定 9 接 1 弃。**

| 缩写 | 源 | 端点 / 鉴权 | schema 置信度 | 处置 |
|---|---|---|---|---|
| `ipreg` | ipregistry | `GET https://api.ipregistry.co/<ip>?key=<KEY>` | **已实测**(tryout key 取得完整真实响应) | 接,代码完整 |
| `vt` | virustotal | `GET https://www.virustotal.com/api/v3/ip_addresses/<ip>` header `x-apikey` | **高**(v3 稳定 schema,无 key 实测返 401) | 接,代码完整 |
| `ipintel` | getipintel | `GET http://check.getipintel.net/check.php?ip=<ip>&contact=<EMAIL>&flags=f&format=json` | **已实测**(占位 contact 取得真实响应) | 接,代码完整 |
| `ipdata` | ipdata.co | `GET https://api.ipdata.co/<ip>?api-key=<KEY>` | **中高**(端点实测 401;`threat` 对象布尔字段稳定,`scores` 子对象需 key 核实) | 接,含 Step 0 核实 |
| `cf` | cloudflare radar | `GET https://api.cloudflare.com/client/v4/radar/...` header `Authorization: Bearer` | **中**(多端点:IP→ASN 再查 summary;summary JSON 形状需 token 核实) | 接,含 Step 0 核实 |
| `bdc` | bigdatacloud | `GET https://api.bigdatacloud.net/data/ip-geolocation-full?ip=<ip>&localityLanguage=en&key=<KEY>` | **中**(阶段一实测免key端点返 403;full 端点 `hazardReport` 字段需 key 核实) | 接,含 Step 0 核实 |
| `scam` | scamalytics | `GET https://<account-host>/<user>/?key=<KEY>&ip=<ip>` | **中**(host/user 因账号而异,需用户提供) | 接,含 Step 0 核实 |
| `fraudlogix` | fraudlogix | base `https://api.fraudlogix.com`(确切 path/params 在注册后文档) | **低**(公开仓库无 schema,PDF/注册后可得) | 接,含 Step 0 核实 |
| `dkly` | dkly | `https://ipinfo.dkly.net`(有公开文档) | **低-中**(文档存在,字段需核实) | 接,含 Step 0 核实 |
| `ipfighter` | ipfighter | 无公开 API(仅网页查分工具) | — | **放弃**,spec/README 注明 |

> **「Step 0 核实」约定**:置信度非「已实测/高」的源,其 Task 第一步为「用你的真实 key `curl` 一次,把响应存成测试用 `SAMPLE` 常量,再据此校正 `Resp` 结构体字段名」。这样既不伪造字段、又保证 parse 与真实响应对齐。无 key 的实现者可跳过该源的运行验证(`NeedsKey` 降级路径已被单测覆盖)。

## 合并规则决策(沿用阶段一,不改既有行为)

spec §3 描述了「数值取中位数 / 分类取众数」。**阶段一实际落地的是 `aggregate.rs` 的 `pick!`(按 `PRIORITY` 取首个非空)+ `majority_bool`(布尔多数决),并未实现中位/众数。** 本阶段**沿用阶段一既成模式**:

- **理由**:① 改成中位/众数会改变 `trust_score`/`fraud_score`/`abuseipdb_score`/`ipqs_score` 等**已发布字段**的合并结果,超出「阶段二只加 keyed 源」范围,有回归风险;② 阶段二多数新字段(VT 黑名单计数、CF 流量占比/分布)是**单源独有**,无合并冲突,`pick!` 即足够;③ 新增布尔(is_cloud/is_relay/is_anonymous/is_bogon)与 `is_datacenter` 同性质,直接复用 `majority_bool`。
- **否决项**:在本阶段重写 merge 为中位/众数 —— 范围蔓延 + 回归风险,留作独立后续(若需要,另开 spec)。

## File Structure

| 文件 | 改动 | 责任 |
|---|---|---|
| `src/model.rs` | 改 | `SourceData` 增 8 个 keyed 字段(threat_level / human_traffic_pct / bot_traffic_pct / browser_dist / device_dist / os_dist / is_cloud / is_relay / is_anonymous / is_bogon / blacklist_*) |
| `src/aggregate.rs` | 改 | `MergedReport` 镜像同字段 + `pick!`/`majority_bool` 合并 |
| `src/sources/ipregistry.rs` | 新 | ipregistry 源 |
| `src/sources/virustotal.rs` | 新 | virustotal 源(黑名单统计) |
| `src/sources/getipintel.rs` | 新 | getipintel 源(代理概率) |
| `src/sources/ipdata.rs` | 新 | ipdata 源 |
| `src/sources/cloudflare.rs` | 新 | cloudflare radar 源(多端点) |
| `src/sources/bigdatacloud.rs` | 新 | bigdatacloud 源 |
| `src/sources/scamalytics.rs` | 新 | scamalytics 源 |
| `src/sources/fraudlogix.rs` | 新 | fraudlogix 源 |
| `src/sources/dkly.rs` | 新 | dkly 源 |
| `src/sources/mod.rs` | 改 | `pub mod` + `all_sources()` 注册 9 源 |
| `src/render/raw.rs` | 改 | `--raw` 增新字段逐源行 |
| `src/render/terminal.rs` | 改 | 默认报告风险区增 VT 黑名单 / CF 流量占比 / threat_level |
| `src/render/json.rs` | 改 | 顶层增新字段 |
| `src/render/markdown.rs` | 改 | 与 terminal 对齐(若该后端展示风险区) |
| `Cargo.toml` | 改 | 版本 0.17.0 → 0.18.0 |
| `CHANGELOG.md` / `README.md` | 改 | 0.18.0 条目 + keyed 源 env 表 + ipfighter 放弃说明 |
| `docs/superpowers/specs/...-multisource-design.md` | 改 | 状态更新:ipfighter 标放弃 |

**实现顺序(执行时按此顺序,前 4 个 schema 已实测/高置信,先落地建立信心)**:
model 字段 → merge → ipregistry → virustotal → getipintel → ipdata → cloudflare → bigdatacloud → scamalytics → fraudlogix → dkly → 渲染 → 文档。

---

## keyed 源标准模板(读一次,后续每源任务只给「差异块」)

所有 keyed 源共享同一骨架(与 `src/sources/abuseipdb.rs` / `ipqs.rs` 同构)。下面是**完整可复制的模板**;后续每个源任务只给出 4 个差异块——`Resp` 结构体、`parse()` 体、`SAMPLE` 常量、配置三元组(`id` / env 名 / url 构造)——其余 boilerplate 一律照本模板填。

```rust
use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

// ===== 差异块 A:Resp 结构体(每源不同) =====
#[derive(Deserialize)]
struct Resp { /* ... */ }

// ===== 差异块 B:parse(每源不同) =====
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("<ID>");
    /* 字段映射 */
    Ok(d)
}

pub struct <Type> {
    pub base: String,
    pub key: Option<String>,   // 部分源还有 user/email,见各任务
}

impl Default for <Type> {
    fn default() -> Self {
        <Type> {
            base: "<BASE_URL>".to_string(),
            key: std::env::var("<ENV>").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for <Type> {
    fn id(&self) -> &'static str { "<ID>" }
    fn needs_key(&self) -> Option<&'static str> { Some("<ENV>") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(|| SourceError::NeedsKey("<ENV>".to_string()))?;
        // ===== 差异块 C:url + header(每源不同) =====
        let url = format!(/* ... */);
        let resp = client.get(&url)/* .header(...) */.send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // ===== 差异块 D:SAMPLE 常量(每源不同) =====
    const SAMPLE: &str = r#"{ ... }"#;

    #[test]
    fn parse_extracts_fields() { /* 见各任务 */ }

    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = <Type> { base: "<BASE_URL>".into(), key: None };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn fetch_with_key_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| { when.path("<PATH>"); then.status(200).body(SAMPLE); });
        let src = <Type> { base: server.base_url(), key: Some("secret".into()) };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        /* 断言关键字段 */
    }
}
```

每个源任务的步骤统一为:
1. (仅核实型源)Step 0:用真实 key `curl` 一次,把响应粘成 `SAMPLE`,据此校正 `Resp` 字段名。
2. 写失败的 `parse` 单测(用 `SAMPLE`)。
3. 跑测试确认 FAIL(函数未定义)。
4. 套模板写源文件(填 A/B/C/D 差异块)。
5. 跑该源单测确认 PASS。
6. `mod.rs` 注册(`pub mod` + `all_sources()` 加 `Box::new(...)`)+ 扩 `all_sources_includes_phase2` 测试断言。
7. 跑 `cargo test` 全绿。
8. Commit。

---

## Task 1: model.rs — 新增 keyed 字段

**Files:**
- Modify: `src/model.rs`(`SourceData` 结构体,在「阶段一 多源质量字段」段后追加)
- Test: `src/model.rs`(`#[cfg(test)] mod tests`)

- [ ] **Step 1: 写失败的字段单测**

在 `src/model.rs` 的 `mod tests` 内追加:

```rust
#[test]
fn sourcedata_has_phase2_fields() {
    let mut d = SourceData::new("vt");
    d.threat_level = Some("high".into());
    d.human_traffic_pct = Some(78.5);
    d.bot_traffic_pct = Some(21.5);
    d.browser_dist = Some("Chrome 64% 其他 36%".into());
    d.device_dist = Some("desktop 70% mobile 30%".into());
    d.os_dist = Some("Windows 55% Android 25%".into());
    d.is_cloud = Some(true);
    d.is_relay = Some(false);
    d.is_anonymous = Some(false);
    d.is_bogon = Some(false);
    d.blacklist_harmless = Some(80);
    d.blacklist_malicious = Some(2);
    d.blacklist_suspicious = Some(1);
    d.blacklist_undetected = Some(11);
    assert_eq!(d.threat_level.as_deref(), Some("high"));
    assert_eq!(d.human_traffic_pct, Some(78.5));
    assert_eq!(d.is_cloud, Some(true));
    assert_eq!(d.blacklist_malicious, Some(2));
}
```

- [ ] **Step 2: 跑测试确认 FAIL**

Run: `cargo test --lib model:: 2>&1 | tail -20`
Expected: 编译失败 —— `no field 'threat_level' on type 'SourceData'`。

- [ ] **Step 3: 加字段**

在 `src/model.rs` 的 `SourceData` 中,`pub is_datacenter: Option<bool>,` 这一行**之后**追加:

```rust
    // —— 阶段二 keyed 源字段 ——
    pub threat_level: Option<String>,        // low/medium/high(ipdata/scamalytics/fraudlogix)
    pub human_traffic_pct: Option<f64>,      // cloudflare radar 人类流量占比
    pub bot_traffic_pct: Option<f64>,        // cloudflare radar 机器人流量占比
    pub browser_dist: Option<String>,        // cloudflare radar 浏览器分布摘要
    pub device_dist: Option<String>,         // cloudflare radar 设备类型分布摘要
    pub os_dist: Option<String>,             // cloudflare radar 操作系统分布摘要
    pub is_cloud: Option<bool>,              // 云服务商(ipregistry/ipdata)
    pub is_relay: Option<bool>,              // 中继(ipregistry,如 iCloud Relay)
    pub is_anonymous: Option<bool>,          // 匿名网络(ipregistry/ipdata)
    pub is_bogon: Option<bool>,              // bogon/保留地址(ipregistry/ipdata)
    pub blacklist_harmless: Option<u32>,     // virustotal 无害引擎数
    pub blacklist_malicious: Option<u32>,    // virustotal 恶意引擎数
    pub blacklist_suspicious: Option<u32>,   // virustotal 可疑引擎数
    pub blacklist_undetected: Option<u32>,   // virustotal 未检出引擎数
```

> `SourceData` 已 `#[derive(Default, Serialize, Deserialize, Clone, Debug)]`,新增 `Option` 字段自动 `None`,无需改 `new()`。

- [ ] **Step 4: 跑测试确认 PASS**

Run: `cargo test --lib model:: 2>&1 | tail -20`
Expected: `sourcedata_has_phase2_fields ... ok`,model 模块全绿。

- [ ] **Step 5: Commit**

```bash
git add src/model.rs
git commit -m "feat(model): 阶段二 keyed 源字段(威胁等级/CF流量/云中继匿名/VT黑名单计数)"
```

---

## Task 2: aggregate.rs — MergedReport 镜像字段 + 合并

**Files:**
- Modify: `src/aggregate.rs`(`MergedReport` 结构体 + `merge()` 函数体)
- Test: `src/aggregate.rs`(`mod tests`)

- [ ] **Step 1: 写失败的合并单测**

在 `src/aggregate.rs` 的 `mod tests` 内追加:

```rust
#[test]
fn merge_carries_phase2_fields() {
    let ip = "1.1.1.1".parse().unwrap();
    let mut vt = SourceData::new("vt");
    vt.blacklist_malicious = Some(2);
    vt.blacklist_harmless = Some(80);
    let mut ipreg = SourceData::new("ipreg");
    ipreg.is_cloud = Some(true);
    ipreg.is_relay = Some(false);
    ipreg.threat_level = Some("high".into());
    let m = merge(ip, vec![
        ("vt".into(), Ok(vt)),
        ("ipreg".into(), Ok(ipreg)),
    ]);
    assert_eq!(m.blacklist_malicious, Some(2));     // 单源 pick
    assert_eq!(m.is_cloud, Some(true));             // 多数决(1:0)
    assert_eq!(m.threat_level.as_deref(), Some("high"));
}

#[test]
fn merge_is_cloud_majority() {
    let ip = "1.1.1.1".parse().unwrap();
    let mk = |id: &str, c: bool| { let mut d = SourceData::new(id); d.is_cloud = Some(c); d };
    let m = merge(ip, vec![
        ("ipreg".into(), Ok(mk("ipreg", true))),
        ("ipdata".into(), Ok(mk("ipdata", true))),
        ("bdc".into(), Ok(mk("bdc", false))),
    ]);
    assert_eq!(m.is_cloud, Some(true)); // 2:1
}
```

- [ ] **Step 2: 跑测试确认 FAIL**

Run: `cargo test --lib aggregate:: 2>&1 | tail -20`
Expected: 编译失败 —— `no field 'blacklist_malicious' on type 'MergedReport'`。

- [ ] **Step 3: MergedReport 加字段**

在 `src/aggregate.rs` 的 `MergedReport` 中,`pub is_datacenter: Option<bool>,` 之后追加(与 Task 1 同名同类型):

```rust
    // —— 阶段二 keyed 源字段 ——
    pub threat_level: Option<String>,
    pub human_traffic_pct: Option<f64>,
    pub bot_traffic_pct: Option<f64>,
    pub browser_dist: Option<String>,
    pub device_dist: Option<String>,
    pub os_dist: Option<String>,
    pub is_cloud: Option<bool>,
    pub is_relay: Option<bool>,
    pub is_anonymous: Option<bool>,
    pub is_bogon: Option<bool>,
    pub blacklist_harmless: Option<u32>,
    pub blacklist_malicious: Option<u32>,
    pub blacklist_suspicious: Option<u32>,
    pub blacklist_undetected: Option<u32>,
```

- [ ] **Step 4: merge() 填充合并逻辑**

在 `merge()` 中,`m.is_datacenter = majority_bool(&ok, |d| d.is_datacenter);` 这一行**之前**(即所有 `pick!(...)` 调用之后)追加标量/字符串字段的 `pick!`:

```rust
    pick!(threat_level);
    pick!(human_traffic_pct); pick!(bot_traffic_pct);
    pick!(browser_dist); pick!(device_dist); pick!(os_dist);
    pick!(blacklist_harmless); pick!(blacklist_malicious);
    pick!(blacklist_suspicious); pick!(blacklist_undetected);
```

再在 `m.is_datacenter = majority_bool(...)` 之后追加新布尔的多数决:

```rust
    m.is_cloud = majority_bool(&ok, |d| d.is_cloud);
    m.is_relay = majority_bool(&ok, |d| d.is_relay);
    m.is_anonymous = majority_bool(&ok, |d| d.is_anonymous);
    m.is_bogon = majority_bool(&ok, |d| d.is_bogon);
```

> `pick!` 宏对 `Copy`(如 `Option<u32>`/`Option<f64>`)与 `Clone`(如 `Option<String>`)字段均适用——它内部用 `.clone()`,`u32`/`f64` 的 `Option` 也实现 `Clone`,无需改宏。

- [ ] **Step 5: 跑测试确认 PASS**

Run: `cargo test --lib aggregate:: 2>&1 | tail -20`
Expected: `merge_carries_phase2_fields ... ok`、`merge_is_cloud_majority ... ok`,aggregate 模块全绿。

- [ ] **Step 6: Commit**

```bash
git add src/aggregate.rs
git commit -m "feat(aggregate): MergedReport 合并阶段二 keyed 字段(pick + 多数决)"
```

---

## Task 3: ipregistry 源(schema 已实测)

**Files:**
- Create: `src/sources/ipregistry.rs`
- Modify: `src/sources/mod.rs`

**配置三元组**:`id = "ipreg"` · env `IPANO_IPREGISTRY_KEY` · `url = format!("{}/{}?key={}", base, ip, key)` · base `https://api.ipregistry.co`

- [ ] **Step 1: 写失败的 parse 单测**(`SAMPLE` 为 2026-06-13 tryout key 实测真实响应,已裁剪)

```rust
const SAMPLE: &str = r#"{"company":{"domain":"apnic.net","name":"Apnic R&D","type":"hosting"},
"connection":{"asn":13335,"domain":"cloudflare.com","organization":"Cloudflare, Inc.","route":"1.1.1.0/24","type":"hosting"},
"ip":"1.1.1.1","location":{"country":{"code":"AU","name":"Australia"},"region":{"code":"AU-QLD","name":"Queensland"},"city":"Brisbane","latitude":-27.46798,"longitude":153.02809},
"security":{"is_abuser":false,"is_attacker":false,"is_bogon":false,"is_cloud_provider":true,"is_proxy":false,"is_relay":false,"is_tor":false,"is_tor_exit":false,"is_vpn":false,"is_anonymous":false,"is_threat":false},
"time_zone":{"id":"Australia/Brisbane"},"type":"IPv4"}"#;

#[test]
fn parse_extracts_security_flags() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "ipreg");
    assert_eq!(d.asn, Some(13335));
    assert_eq!(d.country.as_deref(), Some("AU"));
    assert_eq!(d.city.as_deref(), Some("Brisbane"));
    assert_eq!(d.company_type.as_deref(), Some("hosting"));
    assert_eq!(d.is_cloud, Some(true));
    assert_eq!(d.is_relay, Some(false));
    assert_eq!(d.is_anonymous, Some(false));
    assert_eq!(d.is_bogon, Some(false));
    assert_eq!(d.is_proxy, Some(false));
    assert_eq!(d.is_abuser, Some(false));
}
```

- [ ] **Step 2: 跑测试确认 FAIL**

Run: `cargo test --lib sources::ipregistry 2>&1 | tail -15`
Expected: `cannot find function 'parse'` / module 不存在。

- [ ] **Step 3: 套模板写 `src/sources/ipregistry.rs`**

差异块 A(`Resp`):

```rust
#[derive(Deserialize)]
struct Resp {
    company: Option<Company>,
    connection: Option<Connection>,
    location: Option<Location>,
    security: Option<Security>,
}
#[derive(Deserialize)]
struct Company { #[serde(rename = "type")] ctype: Option<String> }
#[derive(Deserialize)]
struct Connection { asn: Option<u32>, organization: Option<String> }
#[derive(Deserialize)]
struct Location { country: Option<CodeName>, region: Option<CodeName>, city: Option<String> }
#[derive(Deserialize)]
struct CodeName { code: Option<String>, name: Option<String> }
#[derive(Deserialize)]
struct Security {
    is_proxy: Option<bool>, is_vpn: Option<bool>, is_tor: Option<bool>,
    is_abuser: Option<bool>, is_bogon: Option<bool>, is_relay: Option<bool>,
    is_anonymous: Option<bool>, is_cloud_provider: Option<bool>,
}
```

差异块 B(`parse`):

```rust
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipreg");
    if let Some(c) = r.company { d.company_type = c.ctype; }
    if let Some(c) = r.connection { d.asn = c.asn; d.as_org = c.organization; }
    if let Some(l) = r.location {
        d.country = l.country.and_then(|x| x.code);
        d.region = l.region.and_then(|x| x.name);
        d.city = l.city;
    }
    if let Some(s) = r.security {
        d.is_proxy = s.is_proxy; d.is_vpn = s.is_vpn; d.is_tor = s.is_tor;
        d.is_abuser = s.is_abuser; d.is_bogon = s.is_bogon; d.is_relay = s.is_relay;
        d.is_anonymous = s.is_anonymous; d.is_cloud = s.is_cloud_provider;
    }
    Ok(d)
}
```

差异块 C(fetch url,无特殊 header):

```rust
        let url = format!("{}/{}?key={}", self.base, ip, key);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?;
```

差异块 D 已在 Step 1 给出。`fetch_with_key_parses` 测试的 mock `when.path` 用 `"/1.1.1.1"`,断言 `assert_eq!(d.is_cloud, Some(true));`。

- [ ] **Step 4: 跑该源单测确认 PASS**

Run: `cargo test --lib sources::ipregistry 2>&1 | tail -15`
Expected: 3 测试全 ok。

- [ ] **Step 5: 注册到 mod.rs**

在 `src/sources/mod.rs` 顶部 `pub mod` 区加 `pub mod ipregistry;`;在 `all_sources()` 的 `vec![...]` 末尾(`ip2location` 之后)加 `Box::new(ipregistry::IpRegistry::default()),`。

> 注意:结构体名用 `IpRegistry`(与文件名 `ipregistry` 对应)。模板里 `<Type>` = `IpRegistry`。

- [ ] **Step 6: 扩注册断言并跑全测**

在 `mod.rs` 的 `all_sources_includes_phase2` 测试(若不存在则新建,见下)断言含 `"ipreg"`。新建测试:

```rust
#[test]
fn all_sources_includes_phase2_keyed() {
    let s = all_sources(None);
    let ids: Vec<&str> = s.iter().map(|x| x.id()).collect();
    for id in ["ipreg", "vt", "ipintel", "ipdata", "cf", "bdc", "scam", "fraudlogix", "dkly"] {
        assert!(ids.contains(&id), "缺少源 {id}");
    }
}
```

> 该测试在 Task 3 会因其余源未注册而失败——**先注释掉尚未实现的 id**,每接一个源解开一个;或在 Task 11 末尾一次性启用全列表。推荐后者:Task 3–10 各自只断言自己的 id 已注册,Task 11 末尾启用完整列表断言。

Run: `cargo test --lib 2>&1 | tail -15`
Expected: 全绿。

- [ ] **Step 7: Commit**

```bash
git add src/sources/ipregistry.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 ipregistry(ipreg,云/中继/匿名/公司类型)"
```

---

## Task 4: virustotal 源(黑名单统计 — 头牌字段)

**Files:**
- Create: `src/sources/virustotal.rs`
- Modify: `src/sources/mod.rs`

**配置三元组**:`id = "vt"` · env `IPANO_VIRUSTOTAL_KEY` · `url = format!("{}/api/v3/ip_addresses/{}", base, ip)` · header `x-apikey: <key>` · base `https://www.virustotal.com`

- [ ] **Step 1: 写失败的 parse 单测**(`SAMPLE` 为 VT v3 标准响应裁剪)

```rust
const SAMPLE: &str = r#"{"data":{"id":"1.1.1.1","type":"ip_address","attributes":{
"as_owner":"Cloudflare, Inc.","asn":13335,"country":"AU",
"last_analysis_stats":{"harmless":80,"malicious":2,"suspicious":1,"undetected":11,"timeout":0}}}}"#;

#[test]
fn parse_extracts_blacklist_stats() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "vt");
    assert_eq!(d.asn, Some(13335));
    assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
    assert_eq!(d.country.as_deref(), Some("AU"));
    assert_eq!(d.blacklist_harmless, Some(80));
    assert_eq!(d.blacklist_malicious, Some(2));
    assert_eq!(d.blacklist_suspicious, Some(1));
    assert_eq!(d.blacklist_undetected, Some(11));
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::virustotal 2>&1 | tail -15`;Expected: 模块/函数不存在。

- [ ] **Step 3: 套模板写 `src/sources/virustotal.rs`**

差异块 A:

```rust
#[derive(Deserialize)]
struct Resp { data: Option<Data> }
#[derive(Deserialize)]
struct Data { attributes: Option<Attr> }
#[derive(Deserialize)]
struct Attr {
    as_owner: Option<String>,
    asn: Option<u32>,
    country: Option<String>,
    last_analysis_stats: Option<Stats>,
}
#[derive(Deserialize)]
struct Stats { harmless: Option<u32>, malicious: Option<u32>, suspicious: Option<u32>, undetected: Option<u32> }
```

差异块 B:

```rust
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let attr = r.data.and_then(|d| d.attributes)
        .ok_or_else(|| SourceError::Parse("VirusTotal 响应缺 data.attributes".into()))?;
    let mut d = SourceData::new("vt");
    d.as_org = attr.as_owner;
    d.asn = attr.asn;
    d.country = attr.country;
    if let Some(s) = attr.last_analysis_stats {
        d.blacklist_harmless = s.harmless;
        d.blacklist_malicious = s.malicious;
        d.blacklist_suspicious = s.suspicious;
        d.blacklist_undetected = s.undetected;
        // 恶意/可疑命中即视为有滥用史
        d.is_abuser = Some(s.malicious.unwrap_or(0) + s.suspicious.unwrap_or(0) > 0);
    }
    Ok(d)
}
```

差异块 C(带 `x-apikey` header):

```rust
        let url = format!("{}/api/v3/ip_addresses/{}", self.base, ip);
        let resp = client.get(&url)
            .header("x-apikey", key)
            .send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
```

`fetch_with_key_parses` mock:`when.path("/api/v3/ip_addresses/1.1.1.1").header("x-apikey", "secret");`,断言 `assert_eq!(d.blacklist_malicious, Some(2));`。`<Type>` = `VirusTotal`。

- [ ] **Step 4: 跑该源单测确认 PASS** — Run: `cargo test --lib sources::virustotal 2>&1 | tail -15`;Expected: 全 ok。

- [ ] **Step 5: 注册** — `mod.rs` 加 `pub mod virustotal;` + `Box::new(virustotal::VirusTotal::default()),`。本任务断言 `"vt"` 已注册(临时单测或并入 Task 11)。

- [ ] **Step 6: 跑全测** — Run: `cargo test --lib 2>&1 | tail -15`;Expected: 全绿。

- [ ] **Step 7: Commit**

```bash
git add src/sources/virustotal.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 virustotal(vt,黑名单无害/恶意/可疑/未检出计数)"
```

---

## Task 5: getipintel 源(代理概率 — schema 已实测)

**Files:**
- Create: `src/sources/getipintel.rs`
- Modify: `src/sources/mod.rs`

**配置**:`id = "ipintel"` · env `IPANO_IPINTEL_EMAIL`(注意:这里 key 字段实为 contact email,必填参数) · `url = format!("{}/check.php?ip={}&contact={}&flags=f&format=json", base, ip, email)` · base `http://check.getipintel.net`

> getipintel 的鉴权是「联系邮箱」而非 key;沿用 `key: Option<String>` 字段承载 email,`needs_key()` 返回 `IPANO_IPINTEL_EMAIL`,语义为「需配置邮箱」。

- [ ] **Step 1: 写失败的 parse 单测**(`SAMPLE` 为 2026-06-13 实测真实响应)

```rust
const SAMPLE: &str = r#"{"status":"success","result":"0.97","queryIP":"1.1.1.1","queryFlags":"f","queryFormat":"json","contact":"x@example.com"}"#;

#[test]
fn parse_maps_probability_to_risk() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "ipintel");
    // result 0.97 → risk_score 97;>=0.95 判定为代理
    assert_eq!(d.risk_score, Some(97));
    assert_eq!(d.is_proxy, Some(true));
}

#[test]
fn parse_low_probability_not_proxy() {
    let body = r#"{"status":"success","result":"0.10"}"#;
    let d = parse(body).unwrap();
    assert_eq!(d.risk_score, Some(10));
    assert_eq!(d.is_proxy, Some(false));
}

#[test]
fn parse_error_status_is_err() {
    // result 为负数字符串表示错误(如 -3 无效邮箱)
    let body = r#"{"status":"error","result":"-3"}"#;
    assert!(parse(body).is_err());
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::getipintel 2>&1 | tail -15`。

- [ ] **Step 3: 套模板写 `src/sources/getipintel.rs`**

差异块 A:

```rust
#[derive(Deserialize)]
struct Resp { status: Option<String>, result: Option<String> }
```

差异块 B:

```rust
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let prob: f64 = r.result.as_deref().and_then(|s| s.parse().ok())
        .ok_or_else(|| SourceError::Parse("getipintel result 非数值".into()))?;
    if r.status.as_deref() == Some("error") || prob < 0.0 {
        return Err(SourceError::Unavailable(format!("getipintel 错误码 {prob}")));
    }
    let mut d = SourceData::new("ipintel");
    d.risk_score = Some((prob * 100.0).round() as i64);
    d.is_proxy = Some(prob >= 0.95);   // 官方建议 0.95 阈值
    Ok(d)
}
```

差异块 C(注意 url 拼 email,且 getipintel 用 HTTP;`self.key` 承载 email):

```rust
        let url = format!("{}/check.php?ip={}&contact={}&flags=f&format=json", self.base, ip, key);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
```

`Default` 的 base 为 `"http://check.getipintel.net"`,env 为 `IPANO_IPINTEL_EMAIL`。`<Type>` = `GetIpIntel`。`fetch_with_key_parses` mock:`when.path("/check.php").query_param("contact", "secret");`,断言 `d.is_proxy == Some(true)`(SAMPLE result 0.97)。

- [ ] **Step 4: 跑该源单测确认 PASS** — Run: `cargo test --lib sources::getipintel 2>&1 | tail -15`。

- [ ] **Step 5: 注册** — `pub mod getipintel;` + `Box::new(getipintel::GetIpIntel::default()),`。

- [ ] **Step 6: 跑全测** — `cargo test --lib 2>&1 | tail -15`。

- [ ] **Step 7: Commit**

```bash
git add src/sources/getipintel.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 getipintel(ipintel,代理/VPN 概率→风控值)"
```

---

## Task 6: ipdata 源(信任/威胁 — 含 Step 0 核实)

**Files:**
- Create: `src/sources/ipdata.rs`
- Modify: `src/sources/mod.rs`

**配置**:`id = "ipdata"` · env `IPANO_IPDATA_KEY` · `url = format!("{}/{}?api-key={}", base, ip, key)` · base `https://api.ipdata.co`

- [ ] **Step 0(核实):用真实 key 抓一次,校正字段**

```bash
curl -sS "https://api.ipdata.co/1.1.1.1?api-key=$IPANO_IPDATA_KEY" | tee /tmp/ipdata.json | python3 -m json.tool | head -60
```

确认 `threat` 子对象字段名(`is_tor` / `is_proxy` / `is_datacenter` / `is_anonymous` / `is_known_attacker` / `is_known_abuser` / `is_threat` / `is_bogon` / `blocklists`),以及是否存在 `threat.scores`。把 `/tmp/ipdata.json` 内容裁剪后粘成下方 `SAMPLE`,如字段名与下文不符则同步改 `Resp`。

- [ ] **Step 1: 写失败的 parse 单测**(`SAMPLE` 为 ipdata 文档化 threat 结构)

```rust
const SAMPLE: &str = r#"{"ip":"1.1.1.1","country_code":"AU","region":"Queensland","city":"Brisbane",
"asn":{"asn":"AS13335","name":"Cloudflare, Inc."},
"threat":{"is_tor":false,"is_icloud_relay":false,"is_proxy":false,"is_datacenter":true,
"is_anonymous":false,"is_known_attacker":false,"is_known_abuser":false,"is_threat":false,"is_bogon":false,
"blocklists":[]}}"#;

#[test]
fn parse_extracts_threat_flags() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "ipdata");
    assert_eq!(d.country.as_deref(), Some("AU"));
    assert_eq!(d.asn, Some(13335));            // "AS13335" → 13335
    assert_eq!(d.is_datacenter, Some(true));
    assert_eq!(d.is_tor, Some(false));
    assert_eq!(d.is_relay, Some(false));       // is_icloud_relay
    assert_eq!(d.is_anonymous, Some(false));
    assert_eq!(d.is_bogon, Some(false));
    assert_eq!(d.is_abuser, Some(false));      // is_known_abuser || is_known_attacker
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::ipdata 2>&1 | tail -15`。

- [ ] **Step 3: 套模板写 `src/sources/ipdata.rs`**

差异块 A:

```rust
#[derive(Deserialize)]
struct Resp {
    country_code: Option<String>, region: Option<String>, city: Option<String>,
    asn: Option<Asn>, threat: Option<Threat>,
}
#[derive(Deserialize)]
struct Asn { asn: Option<String>, name: Option<String> }
#[derive(Deserialize)]
struct Threat {
    is_tor: Option<bool>, is_icloud_relay: Option<bool>, is_proxy: Option<bool>,
    is_datacenter: Option<bool>, is_anonymous: Option<bool>, is_bogon: Option<bool>,
    is_known_attacker: Option<bool>, is_known_abuser: Option<bool>, is_threat: Option<bool>,
}
```

差异块 B:

```rust
/// "AS13335" → 13335
fn asn_num(s: &str) -> Option<u32> { s.trim_start_matches("AS").parse().ok() }

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("ipdata");
    d.country = r.country_code; d.region = r.region; d.city = r.city;
    if let Some(a) = r.asn { d.asn = a.asn.as_deref().and_then(asn_num); d.as_org = a.name; }
    if let Some(t) = r.threat {
        d.is_tor = t.is_tor; d.is_proxy = t.is_proxy; d.is_datacenter = t.is_datacenter;
        d.is_anonymous = t.is_anonymous; d.is_bogon = t.is_bogon; d.is_relay = t.is_icloud_relay;
        d.is_abuser = Some(t.is_known_abuser.unwrap_or(false) || t.is_known_attacker.unwrap_or(false));
        if t.is_threat == Some(true) { d.threat_level = Some("high".into()); }
    }
    Ok(d)
}
```

差异块 C:

```rust
        let url = format!("{}/{}?api-key={}", self.base, ip, key);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
```

`<Type>` = `IpData`。`fetch_with_key_parses` mock:`when.path("/1.1.1.1").query_param("api-key", "secret");`,断言 `d.is_datacenter == Some(true)`。

- [ ] **Step 4: 跑该源单测确认 PASS** — Run: `cargo test --lib sources::ipdata 2>&1 | tail -15`。

- [ ] **Step 5: 注册** — `pub mod ipdata;` + `Box::new(ipdata::IpData::default()),`。

- [ ] **Step 6: 跑全测** — `cargo test --lib 2>&1 | tail -15`。

- [ ] **Step 7: Commit**

```bash
git add src/sources/ipdata.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 ipdata(数据中心/Tor/中继/匿名/滥用威胁)"
```

---

## Task 7: cloudflare radar 源(人机流量 + 设备分布 — 头牌字段,多端点,含 Step 0 核实)

**Files:**
- Create: `src/sources/cloudflare.rs`
- Modify: `src/sources/mod.rs`

**配置**:`id = "cf"` · env `IPANO_CF_TOKEN` · header `Authorization: Bearer <token>` · base `https://api.cloudflare.com/client/v4`

**特殊性**:Cloudflare Radar 不提供「单 IP」画像,而是「IP→ASN→该 ASN 的聚合流量」。因此该源 `fetch` 需 **2+ 次请求**:① IP 解析 ASN;② 按 ASN 查 bot_class / device_type 摘要。spec §诚实标注已写明「Radar 为 ASN/地区聚合,非该 IP 精确画像,仅供参考」。

- [ ] **Step 0(核实):用真实 token 抓三个端点,确认路径与 JSON 形状**

```bash
TOK="Authorization: Bearer $IPANO_CF_TOKEN"
# ① IP→ASN
curl -sS -H "$TOK" "https://api.cloudflare.com/client/v4/radar/entities/asns/ip?ip=1.1.1.1" | python3 -m json.tool | head -30
# ② 该 ASN 的 bot/human 流量占比(用 ① 返回的 asn,如 13335)
curl -sS -H "$TOK" "https://api.cloudflare.com/client/v4/radar/http/summary/bot_class?asn=13335&dateRange=7d" | python3 -m json.tool
# ③ 设备类型分布
curl -sS -H "$TOK" "https://api.cloudflare.com/client/v4/radar/http/summary/device_type?asn=13335&dateRange=7d" | python3 -m json.tool
```

记录:① 响应里 ASN 数值的 JSON 路径(预期 `result.asn.asn` 或 `result.asns[0].asn`);② bot_class 的 `result.summary_0.{bot,human}`(值为百分比字符串);③ device_type 的 `result.summary_0.{desktop,mobile,other}`。**把实际形状填进下方 `Resp*` 与 `SAMPLE_*`**;若路径不符,以实测为准改结构体。

- [ ] **Step 1: 写失败的 parse 单测**(两个纯函数:`parse_asn` 与 `parse_bot` / `parse_device`)

```rust
const SAMPLE_ASN: &str = r#"{"result":{"asn":{"asn":13335,"name":"CLOUDFLARENET"}},"success":true}"#;
const SAMPLE_BOT: &str = r#"{"result":{"summary_0":{"bot":"21.5","human":"78.5"}},"success":true}"#;
const SAMPLE_DEV: &str = r#"{"result":{"summary_0":{"desktop":"70.0","mobile":"28.0","other":"2.0"}},"success":true}"#;

#[test]
fn parse_asn_extracts_number() {
    assert_eq!(parse_asn(SAMPLE_ASN).unwrap(), 13335);
}

#[test]
fn build_data_merges_summaries() {
    let d = build_data(Some((78.5, 21.5)), Some("desktop 70.0% mobile 28.0% other 2.0%".into()));
    assert_eq!(d.source_id, "cf");
    assert_eq!(d.human_traffic_pct, Some(78.5));
    assert_eq!(d.bot_traffic_pct, Some(21.5));
    assert_eq!(d.device_dist.as_deref(), Some("desktop 70.0% mobile 28.0% other 2.0%"));
}

#[test]
fn parse_bot_pcts() {
    let (h, b) = parse_bot(SAMPLE_BOT).unwrap();
    assert_eq!(h, 78.5);
    assert_eq!(b, 21.5);
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::cloudflare 2>&1 | tail -15`。

- [ ] **Step 3: 写 `src/sources/cloudflare.rs`**(不完全套模板:fetch 多步)

```rust
use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult};

#[derive(Deserialize)]
struct AsnResp { result: Option<AsnResult> }
#[derive(Deserialize)]
struct AsnResult { asn: Option<AsnInner> }
#[derive(Deserialize)]
struct AsnInner { asn: Option<u32> }

#[derive(Deserialize)]
struct BotResp { result: Option<BotResult> }
#[derive(Deserialize)]
struct BotResult { summary_0: Option<BotSummary> }
#[derive(Deserialize)]
struct BotSummary { bot: Option<String>, human: Option<String> }

#[derive(Deserialize)]
struct DevResp { result: Option<DevResult> }
#[derive(Deserialize)]
struct DevResult { summary_0: Option<std::collections::BTreeMap<String, String>> }

pub fn parse_asn(body: &str) -> Result<u32, SourceError> {
    let r: AsnResp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    r.result.and_then(|x| x.asn).and_then(|x| x.asn)
        .ok_or_else(|| SourceError::Parse("CF: 无法从响应解析 ASN".into()))
}

/// 返回 (human_pct, bot_pct)
pub fn parse_bot(body: &str) -> Result<(f64, f64), SourceError> {
    let r: BotResp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let s = r.result.and_then(|x| x.summary_0)
        .ok_or_else(|| SourceError::Parse("CF: bot_class 缺 summary_0".into()))?;
    let h = s.human.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let b = s.bot.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    Ok((h, b))
}

/// "desktop 70.0% mobile 28.0% other 2.0%"
pub fn parse_device(body: &str) -> Option<String> {
    let r: DevResp = serde_json::from_str(body).ok()?;
    let m = r.result?.summary_0?;
    let parts: Vec<String> = m.iter().map(|(k, v)| format!("{k} {v}%")).collect();
    if parts.is_empty() { None } else { Some(parts.join(" ")) }
}

pub fn build_data(traffic: Option<(f64, f64)>, device_dist: Option<String>) -> SourceData {
    let mut d = SourceData::new("cf");
    if let Some((h, b)) = traffic { d.human_traffic_pct = Some(h); d.bot_traffic_pct = Some(b); }
    d.device_dist = device_dist;
    d
}

pub struct Cloudflare { pub base: String, pub key: Option<String> }
impl Default for Cloudflare {
    fn default() -> Self {
        Cloudflare {
            base: "https://api.cloudflare.com/client/v4".to_string(),
            key: std::env::var("IPANO_CF_TOKEN").ok().filter(|s| !s.is_empty()),
        }
    }
}

#[async_trait]
impl Source for Cloudflare {
    fn id(&self) -> &'static str { "cf" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_CF_TOKEN") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(|| SourceError::NeedsKey("IPANO_CF_TOKEN".to_string()))?;
        let bearer = format!("Bearer {key}");
        let get = |url: String| {
            let c = client.clone(); let b = bearer.clone();
            async move {
                c.get(&url).header(reqwest::header::AUTHORIZATION, b).send().await
                    .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?
                    .text().await.map_err(|e| SourceError::Unavailable(e.to_string()))
            }
        };
        // ① IP → ASN
        let asn_body = get(format!("{}/radar/entities/asns/ip?ip={}", self.base, ip)).await?;
        let asn = parse_asn(&asn_body)?;
        // ② 流量 + ③ 设备(失败则该子项留空,不拖垮整源)
        let traffic = get(format!("{}/radar/http/summary/bot_class?asn={}&dateRange=7d", self.base, asn))
            .await.ok().and_then(|b| parse_bot(&b).ok());
        let device = get(format!("{}/radar/http/summary/device_type?asn={}&dateRange=7d", self.base, asn))
            .await.ok().and_then(|b| parse_device(&b));
        if traffic.is_none() && device.is_none() {
            return Err(SourceError::Unavailable("CF: 无可用 Radar 聚合数据".into()));
        }
        Ok(build_data(traffic, device))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // SAMPLE_* + 上述 Step 1 单测
    // no_key 测试:
    #[tokio::test]
    async fn no_key_yields_needs_key() {
        let src = Cloudflare { base: "https://api.cloudflare.com/client/v4".into(), key: None };
        let err = src.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }
    // 端到端 mock:三个 path 分别返回 SAMPLE_ASN / SAMPLE_BOT / SAMPLE_DEV
    #[tokio::test]
    async fn fetch_chains_asn_then_summaries() {
        let server = httpmock::MockServer::start();
        server.mock(|w, t| { w.path("/radar/entities/asns/ip"); t.status(200).body(SAMPLE_ASN); });
        server.mock(|w, t| { w.path("/radar/http/summary/bot_class"); t.status(200).body(SAMPLE_BOT); });
        server.mock(|w, t| { w.path("/radar/http/summary/device_type"); t.status(200).body(SAMPLE_DEV); });
        let src = Cloudflare { base: server.base_url(), key: Some("secret".into()) };
        let d = src.fetch(&crate::fetch::build_client(5), "1.1.1.1".parse().unwrap()).await.unwrap();
        assert_eq!(d.human_traffic_pct, Some(78.5));
        assert!(d.device_dist.is_some());
    }
}
```

> 注:`reqwest::Client` 是 `Arc` 包裹,`.clone()` 廉价。若 `httpmock` 对同 server 多 mock 的 path 匹配有歧义,给每个 mock 加 `when.method(GET)` 与精确 path 即可。

- [ ] **Step 4: 跑该源单测确认 PASS** — Run: `cargo test --lib sources::cloudflare 2>&1 | tail -15`。

- [ ] **Step 5: 注册** — `pub mod cloudflare;` + `Box::new(cloudflare::Cloudflare::default()),`。

- [ ] **Step 6: 跑全测** — `cargo test --lib 2>&1 | tail -15`。

- [ ] **Step 7: Commit**

```bash
git add src/sources/cloudflare.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 cloudflare radar(cf,ASN 聚合人机流量/设备分布)"
```

---

## Task 8: bigdatacloud 源(含 Step 0 核实)

**Files:**
- Create: `src/sources/bigdatacloud.rs`
- Modify: `src/sources/mod.rs`

**配置**:`id = "bdc"` · env `IPANO_BDC_KEY` · `url = format!("{}/data/ip-geolocation-full?ip={}&localityLanguage=en&key={}", base, ip, key)` · base `https://api.bigdatacloud.net`

- [ ] **Step 0(核实):**

```bash
curl -sS "https://api.bigdatacloud.net/data/ip-geolocation-full?ip=1.1.1.1&localityLanguage=en&key=$IPANO_BDC_KEY" | tee /tmp/bdc.json | python3 -m json.tool | head -80
```

确认 `hazardReport` 字段名(`isKnownAsTorServer` / `isKnownAsVpn` / `isKnownAsProxy` / `hazardScore`)与 `country.isoAlpha2` / `location.city` / `network.organisation`。据实测裁剪填 `SAMPLE`,校正 `Resp`。

- [ ] **Step 1: 写失败的 parse 单测**

```rust
const SAMPLE: &str = r#"{"country":{"isoAlpha2":"AU","name":"Australia"},
"location":{"city":"Brisbane"},"network":{"organisation":"Cloudflare, Inc.","registeredCountry":{}},
"hazardReport":{"isKnownAsVpn":false,"isKnownAsTorServer":false,"isKnownAsProxy":false,"hazardScore":12}}"#;

#[test]
fn parse_extracts_hazard() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "bdc");
    assert_eq!(d.country.as_deref(), Some("AU"));
    assert_eq!(d.city.as_deref(), Some("Brisbane"));
    assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
    assert_eq!(d.is_vpn, Some(false));
    assert_eq!(d.is_tor, Some(false));
    assert_eq!(d.is_proxy, Some(false));
    assert_eq!(d.risk_score, Some(12));
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::bigdatacloud 2>&1 | tail -15`。

- [ ] **Step 3: 套模板写 `src/sources/bigdatacloud.rs`**

差异块 A:

```rust
#[derive(Deserialize)]
struct Resp { country: Option<Country>, location: Option<Loc>, network: Option<Net>, #[serde(rename = "hazardReport")] hazard: Option<Hazard> }
#[derive(Deserialize)]
struct Country { #[serde(rename = "isoAlpha2")] iso: Option<String> }
#[derive(Deserialize)]
struct Loc { city: Option<String> }
#[derive(Deserialize)]
struct Net { organisation: Option<String> }
#[derive(Deserialize)]
struct Hazard {
    #[serde(rename = "isKnownAsVpn")] is_vpn: Option<bool>,
    #[serde(rename = "isKnownAsTorServer")] is_tor: Option<bool>,
    #[serde(rename = "isKnownAsProxy")] is_proxy: Option<bool>,
    #[serde(rename = "hazardScore")] hazard_score: Option<i64>,
}
```

差异块 B:

```rust
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("bdc");
    d.country = r.country.and_then(|c| c.iso);
    d.city = r.location.and_then(|l| l.city);
    d.as_org = r.network.and_then(|n| n.organisation);
    if let Some(h) = r.hazard {
        d.is_vpn = h.is_vpn; d.is_tor = h.is_tor; d.is_proxy = h.is_proxy;
        d.risk_score = h.hazard_score;
    }
    Ok(d)
}
```

差异块 C:`let url = format!("{}/data/ip-geolocation-full?ip={}&localityLanguage=en&key={}", self.base, ip, key);` + 标准 send/429。`<Type>` = `BigDataCloud`。`fetch_with_key_parses` mock:`when.path("/data/ip-geolocation-full").query_param("key", "secret");`,断言 `d.risk_score == Some(12)`。

- [ ] **Step 4–7**:跑源测 PASS → 注册 `pub mod bigdatacloud;` + `Box::new(bigdatacloud::BigDataCloud::default()),` → 全测 → commit:

```bash
git add src/sources/bigdatacloud.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 bigdatacloud(bdc,hazardReport VPN/Tor/代理 + 危险分)"
```

---

## Task 9: scamalytics 源(含 Step 0 核实;host/user 因账号而异)

**Files:**
- Create: `src/sources/scamalytics.rs`
- Modify: `src/sources/mod.rs`

**配置**:`id = "scam"` · key env `IPANO_SCAMALYTICS_KEY` · 额外 env `IPANO_SCAMALYTICS_USER`(账号名)+ `IPANO_SCAMALYTICS_BASE`(账号专属 host,默认 `https://api12.scamalytics.com`) · `url = format!("{}/{}/?key={}&ip={}", base, user, key, ip)`

> scamalytics 的请求 host(如 `api11`/`api12`)与 username 因账号而异,**必须**由用户从其 dashboard 取得。结构体多一个 `user: Option<String>` 字段;`fetch` 中 user 缺失也返回 `NeedsKey`。

- [ ] **Step 0(核实):**

```bash
curl -sS "$IPANO_SCAMALYTICS_BASE/$IPANO_SCAMALYTICS_USER/?key=$IPANO_SCAMALYTICS_KEY&ip=1.1.1.1" | python3 -m json.tool | head -50
```

确认外层是否 `scamalytics` 包裹,以及 `scamalytics_score`(数值)/`scamalytics_risk`(low/medium/high/very high)/`scamalytics_proxy.{is_vpn,is_tor,is_datacenter,...}`。据实测校正 `Resp` 与 `SAMPLE`。

- [ ] **Step 1: 写失败的 parse 单测**

```rust
const SAMPLE: &str = r#"{"scamalytics":{"status":"ok","scamalytics_score":18,"scamalytics_risk":"low",
"scamalytics_proxy":{"is_vpn":false,"is_tor":false,"is_datacenter":true,"is_anonymous":false}}}"#;

#[test]
fn parse_extracts_score_and_risk() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "scam");
    assert_eq!(d.fraud_score, Some(18));
    assert_eq!(d.threat_level.as_deref(), Some("low"));
    assert_eq!(d.is_datacenter, Some(true));
    assert_eq!(d.is_vpn, Some(false));
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::scamalytics 2>&1 | tail -15`。

- [ ] **Step 3: 写 `src/sources/scamalytics.rs`**(结构体含 `user`)

差异块 A:

```rust
#[derive(Deserialize)]
struct Resp { scamalytics: Option<Inner> }
#[derive(Deserialize)]
struct Inner {
    scamalytics_score: Option<i64>,
    scamalytics_risk: Option<String>,
    scamalytics_proxy: Option<Proxy>,
}
#[derive(Deserialize)]
struct Proxy { is_vpn: Option<bool>, is_tor: Option<bool>, is_datacenter: Option<bool>, is_anonymous: Option<bool> }
```

差异块 B:

```rust
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let s = r.scamalytics.ok_or_else(|| SourceError::Parse("scamalytics 响应缺 scamalytics".into()))?;
    let mut d = SourceData::new("scam");
    d.fraud_score = s.scamalytics_score;
    d.threat_level = s.scamalytics_risk;
    if let Some(p) = s.scamalytics_proxy {
        d.is_vpn = p.is_vpn; d.is_tor = p.is_tor; d.is_datacenter = p.is_datacenter; d.is_anonymous = p.is_anonymous;
    }
    Ok(d)
}
```

差异块 C(结构体与 fetch):

```rust
pub struct Scamalytics { pub base: String, pub user: Option<String>, pub key: Option<String> }
impl Default for Scamalytics {
    fn default() -> Self {
        Scamalytics {
            base: std::env::var("IPANO_SCAMALYTICS_BASE").ok().filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api12.scamalytics.com".to_string()),
            user: std::env::var("IPANO_SCAMALYTICS_USER").ok().filter(|s| !s.is_empty()),
            key: std::env::var("IPANO_SCAMALYTICS_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}
#[async_trait]
impl Source for Scamalytics {
    fn id(&self) -> &'static str { "scam" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_SCAMALYTICS_KEY") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let key = self.key.as_ref().ok_or_else(|| SourceError::NeedsKey("IPANO_SCAMALYTICS_KEY".to_string()))?;
        let user = self.user.as_ref().ok_or_else(|| SourceError::NeedsKey("IPANO_SCAMALYTICS_USER".to_string()))?;
        let url = format!("{}/{}/?key={}&ip={}", self.base, user, key, ip);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout } else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}
```

`no_key` 测试构造 `Scamalytics { base, user: Some("u".into()), key: None }`;`fetch_with_key_parses` 构造 `user: Some("u".into()), key: Some("secret".into())`,mock `when.path("/u/").query_param("ip", "1.1.1.1");`,断言 `d.fraud_score == Some(18)`。

- [ ] **Step 4–7**:源测 PASS → 注册 `pub mod scamalytics;` + `Box::new(scamalytics::Scamalytics::default()),` → 全测 → commit:

```bash
git add src/sources/scamalytics.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 scamalytics(scam,欺诈分 + 风险等级 + 代理判定)"
```

---

## Task 10: fraudlogix 源(含 Step 0 核实;schema 置信度最低)

**Files:**
- Create: `src/sources/fraudlogix.rs`
- Modify: `src/sources/mod.rs`

**配置**:`id = "fraudlogix"` · env `IPANO_FRAUDLOGIX_KEY` · base `https://api.fraudlogix.com`

> 公开仓库无完整 schema;确切 path/参数/字段名在注册后文档(或其 Bot&Fraud API PDF)。**Step 0 必做**,否则字段名几乎肯定不符。

- [ ] **Step 0(核实):** 登录 fraudlogix dashboard 取 key 与 API 文档,确认:① 端点 path 与查询参数(IP、key 如何传);② 响应字段名(风险等级 Low/Medium/High/Extreme、proxy/vpn/datacenter/bot/blacklist)。把真实响应粘成 `SAMPLE`,据此写 `Resp` 与 `url`。

```bash
# 占位,确切 path 以文档为准:
curl -sS "https://api.fraudlogix.com/<path>?key=$IPANO_FRAUDLOGIX_KEY&ip=1.1.1.1" | python3 -m json.tool | head -50
```

- [ ] **Step 1: 写失败的 parse 单测**(`SAMPLE` 为「文档化字段假设」,Step 0 后据实修正)

```rust
const SAMPLE: &str = r#"{"risk":"Low","proxy":false,"vpn":false,"datacenter":true,"bot":false,"blacklisted":false}"#;

#[test]
fn parse_extracts_risk_and_flags() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "fraudlogix");
    assert_eq!(d.threat_level.as_deref(), Some("low"));   // 归一化小写
    assert_eq!(d.is_proxy, Some(false));
    assert_eq!(d.is_datacenter, Some(true));
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::fraudlogix 2>&1 | tail -15`。

- [ ] **Step 3: 套模板写 `src/sources/fraudlogix.rs`**

差异块 A:

```rust
#[derive(Deserialize)]
struct Resp {
    risk: Option<String>,
    proxy: Option<bool>, vpn: Option<bool>, datacenter: Option<bool>,
    bot: Option<bool>, blacklisted: Option<bool>,
}
```

差异块 B:

```rust
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("fraudlogix");
    d.threat_level = r.risk.map(|s| s.to_lowercase());
    d.is_proxy = r.proxy; d.is_vpn = r.vpn; d.is_datacenter = r.datacenter;
    d.is_crawler = r.bot;
    d.is_abuser = r.blacklisted;
    Ok(d)
}
```

差异块 C:`let url = format!("{}/<path>?key={}&ip={}", self.base, key, ip);`(**path 待 Step 0 填**)+ 标准 send/429。`<Type>` = `FraudLogix`。mock path 用 Step 0 实际 path。

- [ ] **Step 4–7**:源测 PASS → 注册 `pub mod fraudlogix;` + `Box::new(fraudlogix::FraudLogix::default()),` → 全测 → commit:

```bash
git add src/sources/fraudlogix.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 fraudlogix(风险等级 + 代理/VPN/机房/黑名单)"
```

> 若 Step 0 发现 fraudlogix 无可用公开 API(纯付费/无 self-serve key),则**放弃该源**:删除本任务产物,在 spec 与 README「放弃源」一节同 ipfighter 一并注明,不强接。

---

## Task 11: dkly 源(含 Step 0 核实)

**Files:**
- Create: `src/sources/dkly.rs`
- Modify: `src/sources/mod.rs`

**配置**:`id = "dkly"` · env `IPANO_DKLY_KEY` · base `https://ipinfo.dkly.net`(确切 path 见 `https://ipinfo.dkly.net/documentation/`)

- [ ] **Step 0(核实):** 看 `https://ipinfo.dkly.net/documentation/` 确认端点 path、key 传法、`security` 字段名(vpn/proxy/tor/threat、residential/datacenter)。注册取 key(文档称无需邮箱验证)。粘真实响应成 `SAMPLE`。

```bash
curl -sS "https://ipinfo.dkly.net/<path>?key=$IPANO_DKLY_KEY&ip=1.1.1.1" | python3 -m json.tool | head -50
```

- [ ] **Step 1: 写失败的 parse 单测**(`SAMPLE` 为文档化假设结构,Step 0 后修正)

```rust
const SAMPLE: &str = r#"{"country":"AU","city":"Brisbane","asn":13335,
"security":{"vpn":false,"proxy":false,"tor":false,"threat":false},
"connection":{"type":"hosting"}}"#;

#[test]
fn parse_extracts_geo_and_security() {
    let d = parse(SAMPLE).unwrap();
    assert_eq!(d.source_id, "dkly");
    assert_eq!(d.country.as_deref(), Some("AU"));
    assert_eq!(d.asn, Some(13335));
    assert_eq!(d.is_vpn, Some(false));
    assert_eq!(d.is_tor, Some(false));
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib sources::dkly 2>&1 | tail -15`。

- [ ] **Step 3: 套模板写 `src/sources/dkly.rs`**

差异块 A:

```rust
#[derive(Deserialize)]
struct Resp { country: Option<String>, city: Option<String>, asn: Option<u32>, security: Option<Sec>, connection: Option<Conn> }
#[derive(Deserialize)]
struct Sec { vpn: Option<bool>, proxy: Option<bool>, tor: Option<bool>, threat: Option<bool> }
#[derive(Deserialize)]
struct Conn { #[serde(rename = "type")] ctype: Option<String> }
```

差异块 B:

```rust
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("dkly");
    d.country = r.country; d.city = r.city; d.asn = r.asn;
    if let Some(s) = r.security {
        d.is_vpn = s.vpn; d.is_proxy = s.proxy; d.is_tor = s.tor;
        if s.threat == Some(true) { d.is_abuser = Some(true); }
    }
    if let Some(c) = r.connection { d.company_type = c.ctype; }
    Ok(d)
}
```

差异块 C:`let url = format!("{}/<path>?key={}&ip={}", self.base, key, ip);`(**path 待 Step 0**)。`<Type>` = `Dkly`。

- [ ] **Step 4–6**:源测 PASS → 注册 `pub mod dkly;` + `Box::new(dkly::Dkly::default()),`。

- [ ] **Step 7: 启用完整注册断言并跑全测**

此时 9 源全部注册,启用 Task 3 Step 6 的 `all_sources_includes_phase2_keyed` 完整列表断言(若 fraudlogix 在 Task 10 被放弃,则从列表移除 `"fraudlogix"`)。

Run: `cargo test --lib 2>&1 | tail -20`
Expected: 全绿,源数 = 14(阶段一)+ 9(本阶段,或 8 若弃 fraudlogix)。

- [ ] **Step 8: Commit**

```bash
git add src/sources/dkly.rs src/sources/mod.rs
git commit -m "feat(sources): 接入 dkly + 启用阶段二完整源注册断言"
```

---

## Task 12: render/raw.rs — `--raw` 增新字段逐源行

**Files:**
- Modify: `src/render/raw.rs`
- Test: `src/render/raw.rs`(`mod tests`)

- [ ] **Step 1: 写失败的渲染单测**

在 `raw.rs` 的 `mod tests::raw_lists_per_source` 之外追加:

```rust
#[test]
fn raw_lists_phase2_fields() {
    let mut vt = SourceData::new("vt");
    vt.blacklist_malicious = Some(2);
    let mut cf = SourceData::new("cf");
    cf.human_traffic_pct = Some(78.5);
    cf.bot_traffic_pct = Some(21.5);
    let mut ipreg = SourceData::new("ipreg");
    ipreg.is_cloud = Some(true);
    ipreg.threat_level = Some("high".into());
    let report = MergedReport { raw: vec![vt, cf, ipreg], ..Default::default() };
    let s = render(&report);
    assert!(s.contains("VT恶意"));
    assert!(s.contains("2 [vt]"));
    assert!(s.contains("人类流量"));
    assert!(s.contains("78.5 [cf]"));
    assert!(s.contains("是否云"));
    assert!(s.contains("Yes [ipreg]"));
    assert!(s.contains("威胁等级"));
    assert!(s.contains("high [ipreg]"));
}
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib render::raw 2>&1 | tail -15`;Expected: 断言失败(缺 `VT恶意` 等行)。

- [ ] **Step 3: 加渲染行**

在 `raw.rs` 的 `render()` 中,`line!("是否数据中心", is_datacenter, ...)` 之后追加:

```rust
    line!("威胁等级", threat_level, |v: &String| v.clone());
    line!("是否云", is_cloud, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否中继", is_relay, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否匿名", is_anonymous, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("人类流量", human_traffic_pct, |v: &f64| format!("{v}"));
    line!("机器人流量", bot_traffic_pct, |v: &f64| format!("{v}"));
    line!("设备分布", device_dist, |v: &String| v.clone());
    line!("VT无害", blacklist_harmless, |v: &u32| format!("{v}"));
    line!("VT恶意", blacklist_malicious, |v: &u32| format!("{v}"));
    line!("VT可疑", blacklist_suspicious, |v: &u32| format!("{v}"));
```

> `line!` 宏对 `Option<u32>`/`Option<f64>` 适用(用 `.as_ref()` + 闭包格式化),与现有 `asn_abuse_score`(f64)同法。

- [ ] **Step 4: 跑测试确认 PASS** — Run: `cargo test --lib render::raw 2>&1 | tail -15`;Expected: `raw_lists_phase2_fields ... ok`。

- [ ] **Step 5: Commit**

```bash
git add src/render/raw.rs
git commit -m "feat(render): --raw 增阶段二字段逐源行(VT黑名单/CF流量/云中继匿名/威胁等级)"
```

---

## Task 13: render/terminal.rs + json.rs — 默认报告与 JSON 暴露新字段

**Files:**
- Modify: `src/render/terminal.rs`(风险区表 + `has_risk`)
- Modify: `src/render/json.rs`(顶层字段)
- Modify: `src/render/markdown.rs`(若该后端有风险区,镜像 terminal;否则跳过并说明)
- Test: 各文件 `mod tests`

- [ ] **Step 1: 写失败的 json 单测**

在 `json.rs` 的 `mod tests` 追加:

```rust
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
```

- [ ] **Step 2: 跑测试确认 FAIL** — Run: `cargo test --lib render::json 2>&1 | tail -15`;Expected: 断言失败(JSON 不含 `blacklist_malicious`)。

- [ ] **Step 3: json.rs 加顶层字段**

在 `json.rs` 的 `json!({...})` 中,`"is_datacenter": r.is_datacenter,` 之后追加:

```rust
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
```

- [ ] **Step 4: 跑 json 测试确认 PASS** — Run: `cargo test --lib render::json 2>&1 | tail -15`。

- [ ] **Step 5: 写失败的 terminal 单测**

在 `terminal.rs` 的 `mod tests` 追加:

```rust
#[test]
fn render_shows_blacklist_and_traffic() {
    let ip = "1.1.1.1".parse().unwrap();
    let mut vt = SourceData::new("vt");
    vt.blacklist_malicious = Some(2);
    vt.blacklist_harmless = Some(80);
    let mut cf = SourceData::new("cf");
    cf.human_traffic_pct = Some(78.5);
    cf.bot_traffic_pct = Some(21.5);
    let report = merge(ip, vec![("vt".into(), Ok(vt)), ("cf".into(), Ok(cf))]);
    let s = render(&report, true, crate::i18n::Lang::Zh);
    assert!(s.contains("VT 黑名单"));
    assert!(s.contains("2 恶意"));
    assert!(s.contains("人机流量"));
}
```

> 确认 `Lang` 变体名(读 `src/i18n.rs`;若为 `Lang::Zh` 以外名称,用实际变体)。

- [ ] **Step 6: 跑测试确认 FAIL** — Run: `cargo test --lib render::terminal 2>&1 | tail -15`。

- [ ] **Step 7: terminal.rs 加风险区行 + 扩 has_risk**

在 `terminal.rs` 的 `has_risk()` 的布尔表达式末尾追加:

```rust
        || r.blacklist_malicious.is_some() || r.human_traffic_pct.is_some()
        || r.threat_level.is_some() || r.is_cloud == Some(true)
```

在风险区表 `rt`,`if let Some(s) = &r.abuser_score { ... }` 之后追加:

```rust
        if let Some(t) = &r.threat_level { rt.add_row(vec!["威胁等级".to_string(), t.clone()]); }
        if r.blacklist_malicious.is_some() || r.blacklist_harmless.is_some() {
            rt.add_row(vec!["VT 黑名单".to_string(), format!(
                "{} 恶意 / {} 可疑 / {} 无害",
                r.blacklist_malicious.unwrap_or(0),
                r.blacklist_suspicious.unwrap_or(0),
                r.blacklist_harmless.unwrap_or(0))]);
        }
        if let (Some(h), Some(b)) = (r.human_traffic_pct, r.bot_traffic_pct) {
            rt.add_row(vec!["人机流量(CF Radar)".to_string(), format!("人类 {h}% / 机器人 {b}%")]);
        }
```

并在 `risk_flags()` 的 `f` 收集中追加(可选,使 flags 行体现云/匿名):

```rust
    if r.is_cloud == Some(true) { f.push("云"); }
    if r.is_anonymous == Some(true) { f.push("匿名"); }
```

- [ ] **Step 8: 跑测试确认 PASS** — Run: `cargo test --lib render::terminal 2>&1 | tail -15`;Expected: `render_shows_blacklist_and_traffic ... ok`。

- [ ] **Step 9: markdown.rs 对齐**

读 `src/render/markdown.rs`:若它独立渲染风险区(不复用 terminal),镜像 Step 7 的三行;若它已基于同一 `MergedReport` 字段循环,确认新字段被覆盖即可。跑 `cargo test --lib render::markdown 2>&1 | tail -15` 确认无回归。

- [ ] **Step 10: Commit**

```bash
git add src/render/terminal.rs src/render/json.rs src/render/markdown.rs
git commit -m "feat(render): 默认报告与 JSON 暴露阶段二字段(VT黑名单/CF人机流量/威胁等级)"
```

---

## Task 14: 文档 + 版本 + spec 状态

**Files:**
- Modify: `Cargo.toml`(version)
- Modify: `CHANGELOG.md`
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-06-13-ipano-ipquality-multisource-design.md`

- [ ] **Step 1: 版本号 0.17.0 → 0.18.0**

`Cargo.toml` 第 3 行 `version = "0.17.0"` 改为 `version = "0.18.0"`。

- [ ] **Step 2: CHANGELOG 0.18.0 条目**

在 `CHANGELOG.md` 的 `## [0.17.0]` 之前插入:

```markdown
## [0.18.0] - 2026-06-13

### 新增

- **IP 质量多源扩充 阶段二(keyed 源)**:接入需 API key 的高价值源,无 key 自动跳过并标注(沿用 AbuseIPDB/IPQS 降级,绝不伪造):
  - **[virustotal](https://www.virustotal.com)(`vt`)**:黑名单引擎统计(无害/恶意/可疑/未检出),默认报告新增「VT 黑名单」行。
  - **[cloudflare radar](https://radar.cloudflare.com)(`cf`)**:基于 IP→ASN 的人机流量占比与设备类型分布(Radar 聚合数据,非该 IP 精确画像,仅供参考)。
  - **[ipregistry](https://ipregistry.co)(`ipreg`)**:云服务商/中继/匿名/公司类型判定。
  - **[ipdata.co](https://ipdata.co)(`ipdata`)**:数据中心/Tor/iCloud中继/匿名/已知滥用威胁。
  - **[getipintel](https://getipintel.net)(`ipintel`)**:代理/VPN 概率(→风控值,需配置联系邮箱 `IPANO_IPINTEL_EMAIL`)。
  - **[bigdatacloud](https://www.bigdatacloud.com)(`bdc`)**:hazardReport VPN/Tor/代理 + 危险分。
  - **[scamalytics](https://scamalytics.com)(`scam`)**:欺诈分 + 风险等级 + 代理判定(需 host/user/key)。
  - **[fraudlogix](https://www.fraudlogix.com)(`fraudlogix`)**:风险等级 + 代理/VPN/机房/黑名单。
  - **[dkly](https://ipinfo.dkly.net)(`dkly`)**:地理 + VPN/代理/Tor/威胁。
- 新字段:威胁等级、人类/机器人流量占比、设备/OS/浏览器分布、是否云/中继/匿名/bogon、VT 黑名单四项计数,`--json` 顶层与 `--raw` 逐源详表一并暴露。

### 移除 / 放弃

- **ipfighter**:经核实无公开 API(仅网页查分工具),按 spec「不可得即放弃、不爬网页」原则放弃接入。

[0.18.0]: https://github.com/Furinelle/ipano/releases/tag/v0.18.0
```

- [ ] **Step 3: README keyed 源 env 表**

在 README 的「源 / 环境变量」相关章节,补 9 个新 env(沿用现有 `IPANO_ABUSEIPDB_KEY`/`IPANO_IPQS_KEY` 的表格格式):

```markdown
| 源 | 环境变量 | 获取 |
|---|---|---|
| virustotal | `IPANO_VIRUSTOTAL_KEY` | virustotal.com 免费账号 API key |
| cloudflare radar | `IPANO_CF_TOKEN` | dash.cloudflare.com → API Tokens(Radar 读权限) |
| ipregistry | `IPANO_IPREGISTRY_KEY` | ipregistry.co 免费额度 |
| ipdata.co | `IPANO_IPDATA_KEY` | ipdata.co 免费额度 |
| getipintel | `IPANO_IPINTEL_EMAIL` | 你的联系邮箱(免费,作必填参数) |
| bigdatacloud | `IPANO_BDC_KEY` | bigdatacloud.com 免费 ~10k/月 |
| scamalytics | `IPANO_SCAMALYTICS_KEY` + `IPANO_SCAMALYTICS_USER` + `IPANO_SCAMALYTICS_BASE` | scamalytics.com dashboard |
| fraudlogix | `IPANO_FRAUDLOGIX_KEY` | fraudlogix.com(1000 次免费) |
| dkly | `IPANO_DKLY_KEY` | ipinfo.dkly.net 注册(无需邮箱验证) |
```

并在「诚实标注」段补:cloudflare 流量/设备为 Radar 按 ASN 聚合,非该 IP 精确画像;ipfighter 因无公开 API 未接入。

- [ ] **Step 4: spec 状态更新**

在 spec `### 1. 源清单` 的阶段二表内,把 `ipfighter` 行标注 `~~放弃~~(无公开 API,2026-06-13 核实)`;在文末「分阶段交付」标注阶段二已实现版本 v0.18.0。

- [ ] **Step 5: 全量构建 + 测试 + clippy**

```bash
cargo build --release 2>&1 | tail -5
cargo test 2>&1 | tail -20
cargo clippy --all-targets 2>&1 | tail -20
```

Expected: build 成功;所有测试通过;clippy 无 error(warning 视现有基线)。

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml CHANGELOG.md README.md docs/superpowers/specs/2026-06-13-ipano-ipquality-multisource-design.md
git commit -m "docs(release): v0.18.0 阶段二 keyed 源文档 + ipfighter 放弃说明"
```

---

## 收尾(实现完成后,非本计划任务)

阶段二完成、`cargo test` 全绿后,按 `superpowers:finishing-a-development-branch` 决定合并/发布。**发布沿用阶段一流程**:`git push origin main` → 打 annotated tag `v0.18.0`(消息 `v0.18.0 — IP 质量多源阶段二(keyed 源)`)→ push tag 触发 `.github/workflows/release.yml` 自动建 Release + musl 二进制。

---

## Self-Review(对照 spec 核查)

**1. spec 覆盖**:
- spec §1 阶段二 10 源 → Task 3–11 接入 9 源(ipreg/vt/ipintel/ipdata/cf/bdc/scam/fraudlogix/dkly),ipfighter 经核实放弃(Task 14 注明)。✅
- spec §2 新字段 → Task 1/2 全部加入 model + MergedReport(threat_level/流量占比/分布/is_cloud/relay/anonymous/bogon/blacklist_*);usage_type/company_type/abuse_score/is_datacenter 阶段一已有。✅
- spec §3 合并规则 → 决策记录:沿用阶段一 `pick!`+`majority_bool`,理由已述(避免改既有字段行为)。⚠ 与 spec 字面「中位/众数」有出入,已在「合并规则决策」明确记录并说明理由。
- spec §4 输出(默认 + --raw + JSON)→ Task 12(raw)/13(terminal+json)。✅
- spec §5 CLI `--raw` → 阶段一已加,本阶段无新 flag。✅
- spec §6 DNSBL → 阶段一已扩到 211,本阶段不动。✅(不在阶段二范围)
- spec §7 错误处理降级 → 全源用 `NeedsKey`/`RateLimited`/`Timeout`,与现有一致。✅
- spec §8 测试 → 每源 parse 纯函数单测 + httpmock fetch 测 + no_key 测;merge 合并测;raw/json/terminal 渲染测。✅
- spec 诚实标注 → Task 14 README 补 CF 聚合声明 + ipfighter 放弃。✅

**2. 占位扫描**:核实型源(ipdata/cf/bdc/scam/fraudlogix/dkly)的 `SAMPLE` 标注为「文档化假设,Step 0 后据实修正」——这是 keyed 源无法离线验证的诚实处理,非空泛占位(每个都给了完整可编译的 `Resp`/`parse`/`SAMPLE`,Step 0 仅校正字段名)。已实测源(ipreg/vt/ipintel)代码为最终版。fraudlogix/dkly/scam/cf/bdc 的 url path 中 `<path>` 待 Step 0 填——已显式标注且给了 curl 命令。

**3. 类型一致性**:Task 1 定义的字段名(`threat_level`/`human_traffic_pct`/`bot_traffic_pct`/`browser_dist`/`device_dist`/`os_dist`/`is_cloud`/`is_relay`/`is_anonymous`/`is_bogon`/`blacklist_harmless/malicious/suspicious/undetected`)在 Task 2(MergedReport + merge)、Task 12(raw)、Task 13(json+terminal)中逐一对应,无命名漂移。各源结构体名(`IpRegistry`/`VirusTotal`/`GetIpIntel`/`IpData`/`Cloudflare`/`BigDataCloud`/`Scamalytics`/`FraudLogix`/`Dkly`)与文件名、注册行一致。

> **注**:`os_dist`/`browser_dist` 字段已加入 model/merge/json,但 cloudflare 源 Task 7 当前只填了 `device_dist`(MVP)。`os_dist`/`browser_dist` 留作 CF 源的可选扩展(再加两次 `get(.../summary/os)`、`.../summary/browser` 即可,与 device 同构),不阻塞主线。已在字段注释体现。
