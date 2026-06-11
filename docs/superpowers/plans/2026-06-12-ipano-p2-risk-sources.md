# ipano P2 风险/纯净度源 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 接入 ip.net.coffee 的 `/api/iprisk/{ip}` JSON 接口作为差异化"风控/纯净度/风险"主源,并实现 ping0.cc 的 cookie 复用降级源(用户自带 token,被 Turnstile 封锁时优雅降级)。

**Architecture:** 沿用现有 `Source` trait + 每源一文件 + 纯解析层/抓取层分离的模式。先扩展 `SourceData`/`MergedReport` 数据模型加入风险字段;net.coffee 走干净 JSON 解析;ping0 走 cookie 注入 + 验证码检测 + 降级,认证后 HTML 解析为 best-effort(选择器标注待真实样本校正);最后扩展终端/JSON 渲染呈现风险区,并更新文档。

**Tech Stack:** Rust, reqwest(rustls), serde/serde_json, async-trait, comfy-table, owo-colors, httpmock(dev)。

## 背景:为何放弃 ping0 程序化抓取

侦察(2026-06-12)确认:ping0.cc 整站(含 `/ip/{ip}`)已被 **Cloudflare Turnstile 验证码**接管,首页只返回 ~1357 字节验证码页;其 token 在浏览器解完验证码后写入 cookie 且 **60 秒过期**(`date.setTime(date.getTime()+60*1000)`)。程序化复刻 token = 绕过验证码,既触红线也技术不可行。

**决策(用户拍板):net.coffee 为主源 + ping0 cookie 复用作可选降级。** net.coffee 的 `/api/iprisk/{ip}` 恰好提供 ping0 那一派的差异化数据(`trust_score` 纯净度、`abuser_score`、`rep_threat`、`ai_verdict`、各 `is_*` 标记),且干净 JSON、无验证码。ping0 仅在用户手动提供有效 token cookie 时复用;无 token → `NeedsKey` 降级;命中验证码 → `ChallengeFailed` 降级。绝不自行求解验证码。

## net.coffee `/api/iprisk/{ip}` 真实响应样本(1.1.1.1)

```json
{"ip": "1.1.1.2", "cidr": "1.1.1.0/24", "is_bogon": false, "is_datacenter": true,
 "isResidential": false, "is_vpn": false, "is_proxy": false, "is_tor": false,
 "is_crawler": false, "is_abuser": true, "is_mobile": false, "company_type": "hosting",
 "company_name": "APNIC Research and Development", "abuser_score": "0.0234 (Elevated)",
 "datacenter_name": "", "asn": 13335, "asOrganization": "Cloudflare, Inc.",
 "country": "Australia", "countryCode": "au", "region": "Queensland", "city": "South Brisbane",
 "src": "g1", "trust_score": 41, "rdns": "security.cloudflare-dns.com",
 "asn_kind": "hosting", "ai_verdict": {"label": "Suspicious", "confidence": 60,
 "reasoning": "Mid-low trust score - residential front possible"}, "rep_threat": 29,
 "intelligence": {"threats": [{"label": "历史滥用记录", "severity": "warn"}]}}
```

## 文件结构

| 文件 | 职责 | 动作 |
|---|---|---|
| `src/model.rs` | 数据模型:`SourceData` 加风险字段 + 新增 `AiVerdict` | 修改 |
| `src/sources/netcoffee.rs` | net.coffee iprisk 解析层 + 抓取层 | 新建 |
| `src/sources/ping0.rs` | ping0 cookie 复用:transport + 验证码降级 + best-effort 解析 | 新建 |
| `src/sources/mod.rs` | 注册两个新源到 `all_sources()` | 修改 |
| `src/aggregate.rs` | `MergedReport` 加风险字段 + `merge` pick + `PRIORITY` | 修改 |
| `src/render/terminal.rs` | 终端报告增加"风险/纯净度"区 | 修改 |
| `src/render/json.rs` | JSON 输出补齐新字段 | 修改 |
| `README.md` / `CHANGELOG.md` | 路线图 P2 状态 + ping0 诚实说明 | 修改 |

---

### Task 1: 扩展数据模型(风险字段 + AiVerdict)

**Files:**
- Modify: `src/model.rs`

- [ ] **Step 1: 写失败测试**

在 `src/model.rs` 的 `mod tests` 内追加:

```rust
    #[test]
    fn sourcedata_has_risk_fields() {
        let mut d = SourceData::new("netcoffee");
        d.trust_score = Some(41);
        d.risk_score = Some(80);
        d.rep_threat = Some(29);
        d.abuser_score = Some("0.0234 (Elevated)".into());
        d.is_abuser = Some(true);
        d.ai_verdict = Some(AiVerdict {
            label: "Suspicious".into(), confidence: 60,
            reasoning: "Mid-low trust score".into(),
        });
        assert_eq!(d.trust_score, Some(41));
        assert_eq!(d.ai_verdict.as_ref().unwrap().confidence, 60);
    }

    #[test]
    fn ai_verdict_roundtrips_json() {
        let v = AiVerdict { label: "Clean".into(), confidence: 90, reasoning: "ok".into() };
        let s = serde_json::to_string(&v).unwrap();
        let back: AiVerdict = serde_json::from_str(&s).unwrap();
        assert_eq!(back.label, "Clean");
        assert_eq!(back.confidence, 90);
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib model:: 2>&1 | tail -15`
Expected: 编译失败 —— `AiVerdict` 未定义、`SourceData` 无 `trust_score` 等字段。

- [ ] **Step 3: 实现 — 新增 AiVerdict 与字段**

在 `src/model.rs` 中,`IpType` enum 之后、`SourceData` 之前插入:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiVerdict {
    pub label: String,
    pub confidence: i64,
    pub reasoning: String,
}
```

在 `SourceData` 结构体里,`pub is_hosting: Option<bool>,` 之后追加:

```rust
    // —— P2 风险/纯净度字段 ——
    pub trust_score: Option<i64>,   // 可信/纯净分 0-100,越高越干净(net.coffee)
    pub risk_score: Option<i64>,    // 风控值 0-100,越高越危险(ping0)
    pub abuser_score: Option<String>,
    pub rep_threat: Option<i64>,    // 信誉威胁值(net.coffee)
    pub ai_verdict: Option<AiVerdict>,
    pub is_abuser: Option<bool>,
    pub is_crawler: Option<bool>,
    pub is_mobile: Option<bool>,
    pub is_residential: Option<bool>,
```

`SourceData` 已派生 `Default`,新增 `Option` 字段默认 `None`,`SourceData::new` 无需改动。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib model:: 2>&1 | tail -15`
Expected: `model::tests` 全部 PASS(含新增 2 个)。

- [ ] **Step 5: 提交**

```bash
git add src/model.rs
git commit -m "feat(model): 新增风险/纯净度字段与 AiVerdict"
```

---

### Task 2: net.coffee 解析层

**Files:**
- Create: `src/sources/netcoffee.rs`

- [ ] **Step 1: 写失败测试 — 纯解析函数**

新建 `src/sources/netcoffee.rs`,先只写测试与 `use`(实现下一步补):

```rust
use std::net::IpAddr;
use serde::Deserialize;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult, IpType, AiVerdict};

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"ip":"1.1.1.2","is_datacenter":true,"isResidential":false,
        "is_vpn":false,"is_proxy":false,"is_tor":false,"is_crawler":false,"is_abuser":true,
        "is_mobile":false,"company_type":"hosting","company_name":"APNIC Research and Development",
        "abuser_score":"0.0234 (Elevated)","asn":13335,"asOrganization":"Cloudflare, Inc.",
        "country":"Australia","region":"Queensland","city":"South Brisbane","trust_score":41,
        "rdns":"security.cloudflare-dns.com","rep_threat":29,
        "ai_verdict":{"label":"Suspicious","confidence":60,"reasoning":"Mid-low trust score"}}"#;

    #[test]
    fn parse_extracts_base_and_risk() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(d.source_id, "netcoffee");
        assert_eq!(d.asn, Some(13335));
        assert_eq!(d.as_org.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(d.city.as_deref(), Some("South Brisbane"));
        assert_eq!(d.rdns.as_deref(), Some("security.cloudflare-dns.com"));
        assert_eq!(d.trust_score, Some(41));
        assert_eq!(d.rep_threat, Some(29));
        assert_eq!(d.abuser_score.as_deref(), Some("0.0234 (Elevated)"));
        assert_eq!(d.is_abuser, Some(true));
        assert_eq!(d.is_hosting, Some(true));      // is_datacenter → is_hosting
        assert_eq!(d.ip_type, Some(IpType::Hosting));
        let v = d.ai_verdict.unwrap();
        assert_eq!(v.label, "Suspicious");
        assert_eq!(v.confidence, 60);
    }

    #[test]
    fn parse_derives_mobile_type() {
        let body = r#"{"is_mobile":true,"is_datacenter":false,"company_type":"isp"}"#;
        let d = parse(body).unwrap();
        assert_eq!(d.ip_type, Some(IpType::Mobile));
        assert_eq!(d.is_mobile, Some(true));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse("not json").is_err());
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib netcoffee 2>&1 | tail -15`
Expected: 编译失败 —— `parse` 未定义。

- [ ] **Step 3: 实现解析层**

在 `src/sources/netcoffee.rs` 顶部(测试模块之前、`use` 之后)插入:

```rust
#[derive(Deserialize)]
struct AiVerdictRaw {
    label: Option<String>,
    confidence: Option<i64>,
    reasoning: Option<String>,
}

#[derive(Deserialize)]
struct Resp {
    asn: Option<u32>,
    #[serde(rename = "asOrganization")]
    as_organization: Option<String>,
    company_name: Option<String>,
    company_type: Option<String>,
    country: Option<String>,
    region: Option<String>,
    city: Option<String>,
    rdns: Option<String>,
    is_proxy: Option<bool>,
    is_vpn: Option<bool>,
    is_tor: Option<bool>,
    is_datacenter: Option<bool>,
    #[serde(rename = "isResidential")]
    is_residential: Option<bool>,
    is_mobile: Option<bool>,
    is_abuser: Option<bool>,
    is_crawler: Option<bool>,
    trust_score: Option<i64>,
    abuser_score: Option<String>,
    rep_threat: Option<i64>,
    ai_verdict: Option<AiVerdictRaw>,
}

/// net.coffee 的 company_type/is_* 字段映射到统一 IpType
fn derive_ip_type(r: &Resp) -> Option<IpType> {
    if r.is_mobile == Some(true) { return Some(IpType::Mobile); }
    if r.is_datacenter == Some(true) || r.company_type.as_deref() == Some("hosting") {
        return Some(IpType::Hosting);
    }
    if r.is_residential == Some(true) { return Some(IpType::Residential); }
    match r.company_type.as_deref() {
        Some("business") => Some(IpType::Business),
        _ => None,
    }
}

pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let r: Resp = serde_json::from_str(body).map_err(|e| SourceError::Parse(e.to_string()))?;
    let mut d = SourceData::new("netcoffee");
    d.asn = r.asn;
    d.as_org = r.as_organization.clone();
    d.isp = r.company_name.clone();
    d.org = r.company_name;
    d.country = r.country;
    d.region = r.region;
    d.city = r.city;
    d.rdns = r.rdns;
    d.is_proxy = r.is_proxy;
    d.is_vpn = r.is_vpn;
    d.is_tor = r.is_tor;
    d.is_hosting = r.is_datacenter;
    d.is_abuser = r.is_abuser;
    d.is_crawler = r.is_crawler;
    d.is_mobile = r.is_mobile;
    d.is_residential = r.is_residential;
    d.trust_score = r.trust_score;
    d.rep_threat = r.rep_threat;
    d.abuser_score = r.abuser_score;
    d.ip_type = derive_ip_type(&r);
    d.ai_verdict = r.ai_verdict.and_then(|v| match (v.label, v.confidence, v.reasoning) {
        (Some(label), Some(confidence), reasoning) =>
            Some(AiVerdict { label, confidence, reasoning: reasoning.unwrap_or_default() }),
        _ => None,
    });
    Ok(d)
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib netcoffee 2>&1 | tail -15`
Expected: `netcoffee::tests` 三个解析测试 PASS。

注:此时 `src/sources/netcoffee.rs` 尚未在 `mod.rs` 声明,需在 Task 4 注册;为让本任务可独立编译测试,本任务末尾先临时声明 —— 见 Step 5。

- [ ] **Step 5: 临时声明模块并提交**

在 `src/sources/mod.rs` 顶部的 `pub mod ipsb;` 之后加一行:

```rust
pub mod netcoffee;
```

Run: `cargo test --lib netcoffee 2>&1 | tail -5`
Expected: PASS(模块已可见)。

```bash
git add src/sources/netcoffee.rs src/sources/mod.rs
git commit -m "feat(netcoffee): iprisk JSON 解析层"
```

---

### Task 3: net.coffee 抓取层

**Files:**
- Modify: `src/sources/netcoffee.rs`

- [ ] **Step 1: 写失败测试 — httpmock 抓取**

在 `src/sources/netcoffee.rs` 的 `mod tests` 内追加:

```rust
    #[tokio::test]
    async fn fetch_hits_iprisk_endpoint() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/api/iprisk/1.1.1.1");
            then.status(200).body(SAMPLE);
        });
        let src = NetCoffee { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.trust_score, Some(41));
        assert_eq!(d.asn, Some(13335));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib netcoffee::tests::fetch 2>&1 | tail -15`
Expected: 编译失败 —— `NetCoffee` 未定义。

- [ ] **Step 3: 实现抓取层**

在 `src/sources/netcoffee.rs` 的 `parse` 函数之后、`mod tests` 之前插入:

```rust
pub struct NetCoffee {
    pub base: String,
}

impl Default for NetCoffee {
    fn default() -> Self {
        NetCoffee { base: "https://ip.net.coffee".to_string() }
    }
}

#[async_trait]
impl Source for NetCoffee {
    fn id(&self) -> &'static str { "netcoffee" }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let url = format!("{}/api/iprisk/{}", self.base, ip);
        let resp = client.get(&url).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        if resp.status().as_u16() == 429 { return Err(SourceError::RateLimited); }
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        parse(&body)
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib netcoffee 2>&1 | tail -10`
Expected: `netcoffee::tests` 全部 PASS(解析 3 + 抓取 1)。

- [ ] **Step 5: 提交**

```bash
git add src/sources/netcoffee.rs
git commit -m "feat(netcoffee): iprisk 抓取层 + 429 限流识别"
```

---

### Task 4: 聚合合并风险字段 + 注册 net.coffee

**Files:**
- Modify: `src/aggregate.rs`
- Modify: `src/sources/mod.rs`

- [ ] **Step 1: 写失败测试 — merge 合并风险字段**

在 `src/aggregate.rs` 的 `mod tests` 内追加:

```rust
    #[test]
    fn merge_carries_risk_fields() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut nc = SourceData::new("netcoffee");
        nc.trust_score = Some(41);
        nc.rep_threat = Some(29);
        nc.is_abuser = Some(true);
        nc.ai_verdict = Some(crate::model::AiVerdict {
            label: "Suspicious".into(), confidence: 60, reasoning: "x".into(),
        });
        let m = merge(ip, vec![("netcoffee".to_string(), Ok(nc))]);
        assert_eq!(m.trust_score, Some(41));
        assert_eq!(m.rep_threat, Some(29));
        assert_eq!(m.is_abuser, Some(true));
        assert_eq!(m.ai_verdict.as_ref().unwrap().confidence, 60);
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib aggregate::tests::merge_carries 2>&1 | tail -15`
Expected: 编译失败 —— `MergedReport` 无 `trust_score` 等字段。

- [ ] **Step 3: 实现 — 扩展 MergedReport、PRIORITY、pick**

`src/aggregate.rs` 改三处。

(a) `PRIORITY` 常量加入 netcoffee(放在 ipsb 之后):

```rust
const PRIORITY: [&str; 4] = ["ipinfo", "ipsb", "netcoffee", "ipapi"];
```

(b) `MergedReport` 结构体里 `pub is_hosting: Option<bool>,` 之后追加(与 `SourceData` 同名同类型):

```rust
    pub trust_score: Option<i64>,
    pub risk_score: Option<i64>,
    pub abuser_score: Option<String>,
    pub rep_threat: Option<i64>,
    pub ai_verdict: Option<crate::model::AiVerdict>,
    pub is_abuser: Option<bool>,
    pub is_crawler: Option<bool>,
    pub is_mobile: Option<bool>,
    pub is_residential: Option<bool>,
```

(c) `merge` 函数里现有 `pick!(ip_type); ...` 那一行之后追加:

```rust
    pick!(trust_score); pick!(risk_score); pick!(abuser_score); pick!(rep_threat);
    pick!(ai_verdict); pick!(is_abuser); pick!(is_crawler); pick!(is_mobile); pick!(is_residential);
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib aggregate 2>&1 | tail -10`
Expected: `aggregate::tests` 全 PASS(原 merge 测试 + 新 risk 测试)。

- [ ] **Step 5: 注册 net.coffee 到 all_sources**

`src/sources/mod.rs` 改两处。

(a) `all_sources()` 的 `vec![...]` 内,`Box::new(ipsb::IpSb::default()),` 之后追加:

```rust
        Box::new(netcoffee::NetCoffee::default()),
```

(b) `mod tests` 内 `all_sources_has_three` 测试改为四源:

```rust
    #[test]
    fn all_sources_includes_netcoffee() {
        let s = all_sources();
        let ids: Vec<&str> = s.iter().map(|x| x.id()).collect();
        assert!(ids.contains(&"ipapi"));
        assert!(ids.contains(&"ipinfo"));
        assert!(ids.contains(&"ipsb"));
        assert!(ids.contains(&"netcoffee"));
    }
```

(删除旧的 `all_sources_has_three` 函数,替换为上面这个。)

- [ ] **Step 6: 跑全量测试确认通过**

Run: `cargo test 2>&1 | tail -10`
Expected: 全部 PASS(测试数比 P1 的 19 个增加)。

- [ ] **Step 7: 提交**

```bash
git add src/aggregate.rs src/sources/mod.rs
git commit -m "feat(aggregate): 合并风险字段并注册 net.coffee 源"
```

---

### Task 5: ping0 cookie 复用源(transport + 验证码降级)

**Files:**
- Create: `src/sources/ping0.rs`

- [ ] **Step 1: 写失败测试 — 降级与挑战检测**

新建 `src/sources/ping0.rs`:

```rust
use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult, IpType};

#[cfg(test)]
mod tests {
    use super::*;

    // ping0 验证码页特征(实测 2026-06-12:含 cf-turnstile 与 captcha-element)
    const CHALLENGE: &str = r#"<html><head>
        <script>window.AliyunCaptchaConfig={region:"cn"};</script></head>
        <body><div id="captcha-element" class="cf-turnstile"
        data-sitekey="0x4AAAAAAB01fdNepRQppzkd"></div></body></html>"#;

    // 认证后 ping0 页面片段。注:选择器为 best-effort,基于"风控值 + 数字"
    // 与"原生 IP"文本标记;待真实认证样本校正(见 parse 注释)。
    const PING0_HTML: &str = r#"<html><body>
        <div class="line"><span class="name">IP 风控值</span>
        <span class="value">41</span></div>
        <div class="line"><span class="name">IP 类型</span>
        <span class="value">原生 IP</span></div>
        </body></html>"#;

    #[test]
    fn detects_turnstile_challenge() {
        assert!(is_challenge(CHALLENGE));
        assert!(!is_challenge("<html><body>风控值 41</body></html>"));
    }

    #[tokio::test]
    async fn no_token_yields_needs_key() {
        let src = Ping0 { base: "https://ping0.cc".into(), token: None, tokentype: "cf".into() };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn challenge_page_yields_challenge_failed() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/ip/1.1.1.1");
            then.status(200).body(CHALLENGE);
        });
        let src = Ping0 { base: server.base_url(), token: Some("abc".into()), tokentype: "cf".into() };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        m.assert();
        assert!(matches!(err, SourceError::ChallengeFailed));
    }

    #[tokio::test]
    async fn sends_token_cookie_and_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/ip/1.1.1.1").header("cookie", "token=abc; tokentype=cf");
            then.status(200).body(PING0_HTML);
        });
        let src = Ping0 { base: server.base_url(), token: Some("abc".into()), tokentype: "cf".into() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert(); // 断言带正确 cookie 的请求确实命中
        assert_eq!(d.source_id, "ping0");
        assert_eq!(d.risk_score, Some(41));
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib ping0 2>&1 | tail -15`
Expected: 编译失败 —— `is_challenge`/`Ping0`/`parse` 未定义。

- [ ] **Step 3: 实现 transport + is_challenge + parse**

在 `src/sources/ping0.rs` 的 `use` 之后、`mod tests` 之前插入:

```rust
/// 判定响应是否为 Cloudflare Turnstile / Aliyun 验证码页(实测特征)
pub fn is_challenge(body: &str) -> bool {
    body.contains("cf-turnstile")
        || body.contains("captcha-element")
        || body.contains("AliyunCaptchaConfig")
}

/// 从 ping0 认证后 HTML 解析风控值/原生 IP。
/// 选择器为 best-effort:基于"风控值"标签后首个 0-100 整数、"原生 IP"文本标记。
/// 待真实认证样本校正(ping0 改版/A-B 测试可能改变结构)。
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let mut d = SourceData::new("ping0");
    if let Some(v) = risk_after_label(body, "风控值") {
        d.risk_score = Some(v);
    }
    if body.contains("原生 IP") {
        d.ip_type = Some(IpType::Native);
    }
    if d.risk_score.is_none() && d.ip_type.is_none() {
        return Err(SourceError::Parse("ping0 页面结构无法识别(可能改版)".to_string()));
    }
    Ok(d)
}

/// 在 label 之后提取首个 0-100 的整数(跳过非数字字符)
fn risk_after_label(body: &str, label: &str) -> Option<i64> {
    let idx = body.find(label)? + label.len();
    let tail = &body[idx..];
    let digits: String = tail.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<i64>().ok().filter(|v| (0..=100).contains(v))
}

pub struct Ping0 {
    pub base: String,
    pub token: Option<String>,
    pub tokentype: String,
}

impl Default for Ping0 {
    fn default() -> Self {
        Ping0 {
            base: "https://ping0.cc".to_string(),
            token: std::env::var("IPANO_PING0_TOKEN").ok().filter(|s| !s.is_empty()),
            tokentype: std::env::var("IPANO_PING0_TOKENTYPE").unwrap_or_else(|_| "cf".to_string()),
        }
    }
}

#[async_trait]
impl Source for Ping0 {
    fn id(&self) -> &'static str { "ping0" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_PING0_TOKEN") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let token = self.token.as_ref().ok_or_else(|| SourceError::NeedsKey(
            "IPANO_PING0_TOKEN(浏览器解 Turnstile 后从 cookie 复制,60 秒内有效)".to_string()))?;
        let url = format!("{}/ip/{}", self.base, ip);
        let cookie = format!("token={}; tokentype={}", token, self.tokentype);
        let resp = client.get(&url).header(reqwest::header::COOKIE, cookie).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        if is_challenge(&body) { return Err(SourceError::ChallengeFailed); }
        parse(&body)
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib ping0 2>&1 | tail -15`
Expected: `ping0::tests` 四个测试全 PASS(挑战检测、无 token 降级、验证码降级、带 cookie 抓取并解析出 risk_score=41)。

- [ ] **Step 5: 临时声明模块并提交**

在 `src/sources/mod.rs` 的 `pub mod netcoffee;` 之后加一行:

```rust
pub mod ping0;
```

Run: `cargo test --lib ping0 2>&1 | tail -5`
Expected: PASS。

```bash
git add src/sources/ping0.rs src/sources/mod.rs
git commit -m "feat(ping0): cookie 复用 transport + Turnstile 验证码降级"
```

---

### Task 6: ping0 解析边界测试 + 注册到 all_sources

**Files:**
- Modify: `src/sources/ping0.rs`
- Modify: `src/sources/mod.rs`

- [ ] **Step 1: 写测试 — 解析边界**

在 `src/sources/ping0.rs` 的 `mod tests` 内追加:

```rust
    #[test]
    fn parse_extracts_risk_and_native() {
        let d = parse(PING0_HTML).unwrap();
        assert_eq!(d.risk_score, Some(41));
        assert_eq!(d.ip_type, Some(IpType::Native));
    }

    #[test]
    fn parse_unrecognized_page_errors() {
        let err = parse("<html><body>欢迎</body></html>").unwrap_err();
        assert!(matches!(err, SourceError::Parse(_)));
    }

    #[test]
    fn risk_label_rejects_out_of_range() {
        // 标签后数字 >100 视为非风控值
        assert_eq!(risk_after_label("风控值 250 分", "风控值"), None);
        assert_eq!(risk_after_label("风控值 88 分", "风控值"), Some(88));
    }
```

- [ ] **Step 2: 跑测试确认通过**

Run: `cargo test --lib ping0 2>&1 | tail -15`
Expected: 三个新测试均 PASS(Task 5 的 `parse`/`risk_after_label` 已满足这些断言)。

> 本任务以测试固化 Task 5 实现的契约边界,无需改动 `parse`;若上一步全 PASS,直接进 Step 3。

- [ ] **Step 3: 注册 ping0 到 all_sources**

`src/sources/mod.rs` 的 `all_sources()` 内,`Box::new(netcoffee::NetCoffee::default()),` 之后追加:

```rust
        Box::new(ping0::Ping0::default()),
```

- [ ] **Step 4: 跑全量测试确认通过**

Run: `cargo test 2>&1 | tail -10`
Expected: 全部 PASS。`all_sources` 现含 5 源;ping0 在无 `IPANO_PING0_TOKEN` 时运行期返回 `NeedsKey`(降级),不影响其它源。

注:`all_sources_includes_netcoffee` 测试不校验源数量,故新增 ping0 不会破坏它。

- [ ] **Step 5: 提交**

```bash
git add src/sources/ping0.rs src/sources/mod.rs
git commit -m "feat(ping0): 解析边界测试 + 注册到 all_sources"
```

---

### Task 7: 渲染风险/纯净度区(terminal + json)

**Files:**
- Modify: `src/render/terminal.rs`
- Modify: `src/render/json.rs`

- [ ] **Step 1: 写失败测试 — 终端渲染风险区**

在 `src/render/terminal.rs` 的 `mod tests` 内追加:

```rust
    #[test]
    fn render_shows_risk_section() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut d = SourceData::new("netcoffee");
        d.trust_score = Some(41);
        d.rep_threat = Some(29);
        d.is_abuser = Some(true);
        d.ai_verdict = Some(crate::model::AiVerdict {
            label: "Suspicious".into(), confidence: 60, reasoning: "front possible".into(),
        });
        let report = merge(ip, vec![("netcoffee".to_string(), Ok(d))]);
        let out = render(&report, true);
        assert!(out.contains("纯净度") || out.contains("可信"));
        assert!(out.contains("41"));
        assert!(out.contains("Suspicious"));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib render::terminal 2>&1 | tail -15`
Expected: FAIL —— 输出不含 "纯净度"/"Suspicious"。

- [ ] **Step 3: 实现 — 终端追加风险区**

完整替换 `src/render/terminal.rs` 的 `render` 函数为下面版本,并在其后追加 `has_risk`、`risk_flags` 两个辅助函数:

```rust
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

    // —— 风险/纯净度区(仅当至少一项有值时展示)——
    if has_risk(r) {
        let mut rt = Table::new();
        rt.load_preset(UTF8_FULL);
        rt.set_header(vec!["风险判定", "值"]);
        if let Some(v) = r.trust_score { rt.add_row(vec!["纯净度(越高越干净)".to_string(), v.to_string()]); }
        if let Some(v) = r.risk_score { rt.add_row(vec!["风控值(越高越危险)".to_string(), v.to_string()]); }
        if let Some(v) = r.rep_threat { rt.add_row(vec!["信誉威胁值".to_string(), v.to_string()]); }
        if let Some(s) = &r.abuser_score { rt.add_row(vec!["滥用评分".to_string(), s.clone()]); }
        rt.add_row(vec!["标记".to_string(), risk_flags(r)]);
        if let Some(v) = &r.ai_verdict {
            rt.add_row(vec!["AI 判定".to_string(),
                format!("{}（{}%）{}", v.label, v.confidence, v.reasoning)]);
        }
        out.push_str(&rt.to_string());
        out.push('\n');
    }

    let status: Vec<String> = r.sources.iter().map(|s| {
        let mark = if s.ok { "✓" } else { "✗" };
        format!("{}{}", mark, s.id)
    }).collect();
    out.push_str(&format!("源状态  {}\n", status.join(" ")));
    out
}

fn has_risk(r: &MergedReport) -> bool {
    r.trust_score.is_some() || r.risk_score.is_some() || r.rep_threat.is_some()
        || r.abuser_score.is_some() || r.ai_verdict.is_some()
        || r.is_proxy == Some(true) || r.is_vpn == Some(true) || r.is_tor == Some(true)
        || r.is_abuser == Some(true) || r.ip_type.is_some()
}

fn risk_flags(r: &MergedReport) -> String {
    let mut f = Vec::new();
    if r.ip_type == Some(crate::model::IpType::Hosting) { f.push("机房"); }
    if r.ip_type == Some(crate::model::IpType::Native) { f.push("原生"); }
    if r.ip_type == Some(crate::model::IpType::Residential) { f.push("家宽"); }
    if r.ip_type == Some(crate::model::IpType::Mobile) { f.push("移动"); }
    if r.is_proxy == Some(true) { f.push("代理"); }
    if r.is_vpn == Some(true) { f.push("VPN"); }
    if r.is_tor == Some(true) { f.push("Tor"); }
    if r.is_abuser == Some(true) { f.push("滥用史"); }
    if r.is_crawler == Some(true) { f.push("爬虫"); }
    if f.is_empty() { "—".to_string() } else { f.join(" ") }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib render::terminal 2>&1 | tail -10`
Expected: `render_shows_risk_section` 与原 `render_contains_header_and_source_status` 均 PASS。

- [ ] **Step 5: 写失败测试 — JSON 含风险字段**

在 `src/render/json.rs` 的 `mod tests` 内追加:

```rust
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
```

- [ ] **Step 6: 跑测试确认失败**

Run: `cargo test --lib render::json 2>&1 | tail -15`
Expected: FAIL —— JSON 不含 trust_score/ai_verdict。

- [ ] **Step 7: 实现 — JSON 补齐字段**

`src/render/json.rs` 的 `to_json` 中,`json!({...})` 内 `"is_hosting": r.is_hosting,` 之后追加(补 is_vpn/is_tor 及全部风险字段):

```rust
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
```

(`AiVerdict` 已派生 `Serialize`,`serde_json::json!` 可直接序列化 `Option<AiVerdict>`。)

- [ ] **Step 8: 跑全量测试确认通过**

Run: `cargo test 2>&1 | tail -10`
Expected: 全部 PASS。

- [ ] **Step 9: 提交**

```bash
git add src/render/terminal.rs src/render/json.rs
git commit -m "feat(render): 终端风险/纯净度区 + JSON 补齐风险字段"
```

---

### Task 8: 文档更新(README + CHANGELOG)

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: 更新 README 路线图与功能**

`README.md` 的路线图表格中,P2、P3 两行替换为:

```markdown
| P2 | **ip.net.coffee 风控/纯净度源**(trust_score/abuser/rep_threat/AI 判定)+ ping0 cookie 复用降级 | ✅ |
| P3 | ippure + 西方欺诈库交叉确认 | 计划中 |
```

"功能(当前版本)" 小节末尾追加一条:

```markdown
- **风险/纯净度**:接入 ip.net.coffee `iprisk` 接口,呈现纯净度、滥用评分、信誉威胁值、AI 判定及代理/VPN/Tor/机房等标记
```

在 "能力边界" 小节末尾追加一段 ping0 诚实说明:

```markdown
**关于 ping0.cc**:ping0 现已被 Cloudflare Turnstile 验证码全站接管,且其 token 60 秒过期,无法程序化抓取(强行绕过验证码不在本工具范围)。ipano 仅支持 **cookie 复用**:在浏览器中解开 ping0 验证码后,把 `token` cookie 值通过环境变量 `IPANO_PING0_TOKEN` 提供(60 秒内有效),ipano 会在该窗口内复用;未提供或已失效时,ping0 源自动标注降级,不影响其它源。
```

- [ ] **Step 2: 更新 CHANGELOG**

`CHANGELOG.md` 在 `## [0.1.0]` 之前插入新版本块:

```markdown
## [0.2.0] - 2026-06-12

P2:差异化风险/纯净度源。

### 新增

- **ip.net.coffee 源**:接入 `/api/iprisk/{ip}` JSON 接口,提供纯净度(trust_score)、滥用评分、信誉威胁值、AI 判定(label/confidence/reasoning)及 is_abuser/is_crawler/is_mobile/is_residential 等标记
- **ping0 cookie 复用源**:支持经 `IPANO_PING0_TOKEN` 环境变量提供浏览器 token 复用;命中 Turnstile 验证码或无 token 时优雅降级(ChallengeFailed/NeedsKey),不阻塞整体
- **数据模型**:`SourceData`/`MergedReport` 新增 trust_score/risk_score/abuser_score/rep_threat/ai_verdict 及四个 is_* 字段;新增 `AiVerdict` 结构
- **渲染**:终端报告新增"风险/纯净度"区(纯净度/风控值/信誉威胁/滥用评分/标记/AI 判定);JSON 补齐 is_vpn/is_tor 及全部风险字段

### 说明

- ping0.cc 已被 Cloudflare Turnstile 全站接管且 token 60 秒过期,本工具不绕过验证码,仅在用户自带有效 cookie 时复用
- 风险分按源独立保留(net.coffee 纯净度越高越干净;ping0 风控值越高越危险),不强行折算成单一数字

[0.2.0]: https://github.com/Furinelle/ipano/releases/tag/v0.2.0
```

- [ ] **Step 3: 校验文档 + 全量测试**

Run: `cargo test 2>&1 | tail -5 && grep -n "P2" README.md`
Expected: 测试全 PASS;README P2 行显示 ✅。

- [ ] **Step 4: 提交**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: P2 风险源路线图 + ping0 诚实说明 + v0.2.0 CHANGELOG"
```

---

## 完成后

全部任务完成后,由 subagent-driven-development 派发最终整体代码评审,再用 superpowers:finishing-a-development-branch 收尾(测试验证 → 合并 main → 打 v0.2.0 → 推送)。

## 自检清单(已核对)

- **Spec 覆盖**:P2 = ping0 差异化数据 → 因 Turnstile 封锁,经用户确认改由 net.coffee 主源交付同类数据(纯净度/风险/AI 判定),ping0 保留 cookie 复用降级路径。✅ 覆盖。
- **占位符扫描**:无 TBD/TODO;ping0 解析选择器为 best-effort 已显式标注理由(无法获取认证样本),非占位。✅
- **类型一致性**:`AiVerdict{label:String,confidence:i64,reasoning:String}` 跨 model/netcoffee/ping0/aggregate/render 一致;新增字段名 trust_score/risk_score/abuser_score/rep_threat/ai_verdict/is_abuser/is_crawler/is_mobile/is_residential 在 `SourceData` 与 `MergedReport` 同名同类型;`merge` 的 `pick!` 宏对每个新字段调用一次。✅
- **风险字段语义**:trust_score(越高越干净,net.coffee)与 risk_score(越高越危险,ping0)分立,不混用同一字段。✅
```