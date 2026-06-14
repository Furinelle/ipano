# UnlockTests 简单探针扩充(阶段 B)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 ipano `--probe` 新增 13 个单请求型解锁探针(Claude/Gemini · Bing/GoogleSearch/Reddit/Wikipedia/OneTrust/Apple/Steam · IQiYi/KOCOWA/Viu/TikTok),并为 `ProbeResult` 增 `info` 备注字段,对标 oneclickvirt/UnlockTests。

**Architecture:** 复用现有 `trait Probe` + `classify_*` 纯函数 + httpmock 测试模式(见 `src/probe/ai.rs::ChatGpt`)。新增 `probe/web.rs`(搜索/工具/商店类)、`probe/unlock_util.rs`(region 三→二码 / cookie 提取 / 正则提取纯函数);AI 类进 `probe/ai.rs`,亚洲媒体 + TikTok 进 `probe/streaming.rs`。全部在 `all_probes()` 注册,渲染自动复用解锁表(新增「备注」列)。MetaAI/SonyLiv/InstagramMusic/GooglePlay/CDN 属复杂多步,**不在本阶段**(留阶段 C)。

**Tech Stack:** Rust,reqwest(异步 HTTP),async-trait,regex(已在依赖?实现期确认,无则用 `str` 查找替代),comfy-table(渲染),httpmock(测试)。

**前置事实(实现者必读):**
- `ProbeResult` 定义在 `src/probe/mod.rs:32-38`,字段 `name/status/region/unlock_type`。唯一字面量构造在 `mod.rs:42` 的 `ProbeResult::new`;`unknown()` 调 `new()`。85 处调用点均经 `::new`/`::unknown`,加字段只改这一处构造 + 渲染表头。
- `ProbeStatus`:`Unlocked/Restricted/Blocked/Unknown`(`mod.rs:16`)。
- 探针模板见 `src/probe/ai.rs`:`struct{base}` + `Default`(默认真实域名 base)+ `impl Probe{ name(); check() }` + 纯函数 `classify_*(status)->ProbeStatus` + httpmock 测试用 `server.base_url()` 覆盖 base。
- 渲染表头在 `mod.rs:140-145`(终端 `set_header` 4 列)与 `mod.rs:179-185`(markdown pipe 表),行循环在 `mod.rs:146-157` 与 `186-190`。
- 默认 HTTP client 由 `crate::fetch::build_client(secs)` 建(见 ai.rs 测试)。
- **regex 依赖**:实现 Task 2 前先 `grep '^regex' Cargo.toml` 确认;若无,本计划所有正则改用 `str::find`/`split` 手写提取(纯函数封装在 unlock_util,接口不变)。

---

### Task 1: `ProbeResult` 增 `info` 字段 + 渲染「备注」列

**Files:**
- Modify: `src/probe/mod.rs`（struct、`new`、`with_info`、终端 + markdown 渲染表）
- Test: `src/probe/mod.rs`（`#[cfg(test)] mod tests`）

- [ ] **Step 1: 写失败测试**

在 `src/probe/mod.rs` 的 `mod tests` 末尾追加:

```rust
    #[test]
    fn probe_result_with_info_and_default_none() {
        let r = ProbeResult::new("Steam", ProbeStatus::Unlocked, Some("us".into()));
        assert_eq!(r.info, None);
        let r2 = r.with_info("Community Available");
        assert_eq!(r2.info.as_deref(), Some("Community Available"));
    }

    #[test]
    fn render_terminal_has_note_column() {
        let mut r = ProbeResult::new("Steam", ProbeStatus::Unlocked, Some("us".into()));
        r = r.with_info("Community Available");
        let s = render_terminal(&[r], crate::i18n::Lang::Zh, true);
        assert!(s.contains("备注"));
        assert!(s.contains("Community Available"));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test probe_result_with_info_and_default_none render_terminal_has_note_column`
Expected: FAIL —— 编译错误（`info` 字段/`with_info` 不存在）。

- [ ] **Step 3: 加字段 + 构造 + 渲染列**

(3a) `src/probe/mod.rs` struct（`mod.rs:32-38`）加字段:

```rust
pub struct ProbeResult {
    pub name: String,
    pub status: ProbeStatus,
    pub region: Option<String>,
    pub unlock_type: UnlockType,
    pub info: Option<String>,
}
```

(3b) `impl ProbeResult`（`mod.rs:40-47`）改 `new` 并加 `with_info`:

```rust
impl ProbeResult {
    pub fn new(name: &str, status: ProbeStatus, region: Option<String>) -> Self {
        ProbeResult { name: name.to_string(), status, region, unlock_type: UnlockType::Unknown, info: None }
    }
    pub fn unknown(name: &str) -> Self {
        ProbeResult::new(name, ProbeStatus::Unknown, None)
    }
    pub fn with_info(mut self, info: impl Into<String>) -> Self {
        self.info = Some(info.into());
        self
    }
}
```

(3c) 终端表头（`mod.rs:140-145`)加列:

```rust
    t.set_header(vec![
        lang.pick("服务", "Service"),
        lang.pick("状态", "Status"),
        lang.pick("地区", "Region"),
        lang.pick("类型", "Type"),
        lang.pick("备注", "Note"),
    ]);
```

(3d) 终端行（`mod.rs:146-157` 的 `for r in results` 末尾 `t.add_row(vec![...])`)加备注单元格:

```rust
        let note = r.info.clone().unwrap_or_else(|| "—".to_string());
        t.add_row(vec![Cell::new(&r.name), status, Cell::new(region), utype, Cell::new(note)]);
```

(3e) markdown 表头（`mod.rs:179-185`)与分隔/行（`mod.rs:185-190`):

```rust
    writeln!(out, "| {} | {} | {} | {} | {} |",
        lang.pick("服务", "Service"),
        lang.pick("状态", "Status"),
        lang.pick("地区", "Region"),
        lang.pick("类型", "Type"),
        lang.pick("备注", "Note"),
    ).ok();
    writeln!(out, "|---|---|---|---|---|").ok();
    for r in results {
        let region = r.region.clone().unwrap_or_else(|| "—".to_string());
        let note = r.info.clone().unwrap_or_else(|| "—".to_string());
        writeln!(out, "| {} | {} | {} | {} | {} |",
            r.name, r.status.label(lang), region, r.unlock_type.label(lang), note).ok();
    }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test probe::`
Expected: PASS（现有 probe 测试不回归;新 2 测试通过）。

- [ ] **Step 5: 提交**

```bash
git add src/probe/mod.rs
git commit -m "feat(probe): ProbeResult 增 info 备注字段 + 解锁表加备注列"
```

---

### Task 2: `probe/unlock_util.rs` 纯函数助手

**Files:**
- Create: `src/probe/unlock_util.rs`
- Modify: `src/probe/mod.rs`（加 `pub mod unlock_util;`）
- Test: `src/probe/unlock_util.rs`（模块内测试）

- [ ] **Step 1: 写文件 + 失败测试**

创建 `src/probe/unlock_util.rs`:

```rust
//! 解锁探针共用纯函数:region 码转换、cookie 提取、正则/子串提取。

/// ISO 3166-1 alpha-3 → alpha-2(覆盖常见解锁地区;未知返回大写原值)。
pub fn three_to_two(code: &str) -> String {
    let c = code.to_uppercase();
    let m = match c.as_str() {
        "USA" => "US", "JPN" => "JP", "GBR" => "GB", "DEU" => "DE", "FRA" => "FR",
        "HKG" => "HK", "TWN" => "TW", "KOR" => "KR", "SGP" => "SG", "CHN" => "CN",
        "CAN" => "CA", "AUS" => "AU", "NLD" => "NL", "IND" => "IN", "BRA" => "BR",
        "RUS" => "RU", "ITA" => "IT", "ESP" => "ES", "THA" => "TH", "MYS" => "MY",
        "IDN" => "ID", "PHL" => "PH", "VNM" => "VN", "TUR" => "TR", "MEX" => "MX",
        _ => return c,
    };
    m.to_string()
}

/// 从 Set-Cookie 串里取某 cookie 的值(到分号或末尾)。无则 None。
pub fn extract_cookie(set_cookie: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let start = set_cookie.find(&needle)? + needle.len();
    let rest = &set_cookie[start..];
    let end = rest.find(';').unwrap_or(rest.len());
    let val = rest[..end].trim();
    if val.is_empty() { None } else { Some(val.to_string()) }
}

/// 从 body 中取 `prefix` 与 `suffix` 之间首个子串(简单无正则提取)。
pub fn between<'a>(body: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let s = body.find(prefix)? + prefix.len();
    let rest = &body[s..];
    let e = rest.find(suffix)?;
    Some(&rest[..e])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_to_two_maps_common() {
        assert_eq!(three_to_two("USA"), "US");
        assert_eq!(three_to_two("jpn"), "JP");
        assert_eq!(three_to_two("ZZZ"), "ZZZ"); // 未知透传
    }

    #[test]
    fn extract_cookie_picks_value() {
        let c = "foo=bar; steamCountry=US%7Cabc; path=/";
        assert_eq!(extract_cookie(c, "steamCountry").as_deref(), Some("US%7Cabc"));
        assert_eq!(extract_cookie(c, "missing"), None);
    }

    #[test]
    fn between_extracts() {
        assert_eq!(between(r#","region":"jp","#, r#""region":""#, r#"""#), Some("jp"));
        assert_eq!(between("abc", "x", "y"), None);
    }
}
```

在 `src/probe/mod.rs` 的模块声明区（`mod.rs:6-11` 一带）加:

```rust
pub mod unlock_util;
```

- [ ] **Step 2: 运行确认通过(本任务测试与实现同写,直接验证)**

Run: `cargo test probe::unlock_util`
Expected: PASS（3 测试）。

- [ ] **Step 3: 提交**

```bash
git add src/probe/unlock_util.rs src/probe/mod.rs
git commit -m "feat(probe): unlock_util 纯函数(三→二码 / cookie 提取 / 子串提取)"
```

---

### Task 3: Claude 探针(ai.rs)— 完整模板示范

**Files:**
- Modify: `src/probe/ai.rs`（加 `Claude` struct + `classify_claude`）
- Modify: `src/probe/mod.rs`（`all_probes()` 注册）
- Test: `src/probe/ai.rs`

- [ ] **Step 1: 写失败测试**

在 `src/probe/ai.rs` 的 `mod tests` 末尾追加:

```rust
    #[test]
    fn claude_status_mapping() {
        assert_eq!(classify_claude(200), ProbeStatus::Unlocked);
        assert_eq!(classify_claude(403), ProbeStatus::Blocked);
        assert_eq!(classify_claude(500), ProbeStatus::Unknown);
    }

    #[tokio::test]
    async fn claude_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/");
            then.status(200).body("ok");
        });
        let p = Claude { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.name, "Claude");
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test claude_status_mapping`
Expected: FAIL —— `classify_claude`/`Claude` 未定义。

- [ ] **Step 3: 实现**

在 `src/probe/ai.rs`（ChatGpt 之后)加:

```rust
// ===== Claude (Anthropic) =====
// 方法:GET claude.ai/ → 200=可用;403/451=地区封锁。区域(可选)从 /cdn-cgi/trace 取。
pub fn classify_claude(status: u16) -> ProbeStatus {
    match status {
        200 => ProbeStatus::Unlocked,
        403 | 451 => ProbeStatus::Blocked,
        _ => ProbeStatus::Unknown,
    }
}

pub struct Claude {
    pub base: String,
}
impl Default for Claude {
    fn default() -> Self { Claude { base: "https://claude.ai".to_string() } }
}

#[async_trait]
impl Probe for Claude {
    fn name(&self) -> &'static str { "Claude" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/", self.base);
        match client.get(&url).send().await {
            Ok(resp) => ProbeResult::new(self.name(), classify_claude(resp.status().as_u16()), None),
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}
```

在 `src/probe/mod.rs` 的 `all_probes()`（`mod.rs:104` `Box::new(ai::ChatGpt::default()),` 一带)加:

```rust
        Box::new(ai::Claude::default()),
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test probe::ai`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/probe/ai.rs src/probe/mod.rs
git commit -m "feat(probe): Claude 解锁探针"
```

---

### Task 4: Gemini 探针(ai.rs)— region 解析模板示范

**Files:**
- Modify: `src/probe/ai.rs`、`src/probe/mod.rs`
- Test: `src/probe/ai.rs`

- [ ] **Step 1: 写失败测试**

在 `mod tests` 追加:

```rust
    #[tokio::test]
    async fn gemini_blocked_403() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| { when.path("/"); then.status(403).body("no"); });
        let p = Gemini { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[tokio::test]
    async fn gemini_unlocked_with_region() {
        let server = httpmock::MockServer::start();
        let body = r#"window.WIZ=[null,2,1,200,"USA"];"#;
        let m = server.mock(|when, then| { when.path("/"); then.status(200).body(body); });
        let p = Gemini { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test gemini_blocked_403 gemini_unlocked_with_region`
Expected: FAIL —— `Gemini` 未定义。

- [ ] **Step 3: 实现**

在 `src/probe/ai.rs` 加(顶部已 `use crate::probe::unlock_util;` 或用全路径):

```rust
// ===== Gemini (Google) =====
// 方法:GET gemini.google.com → 200 解析地区(三码转两码);403/451=封锁。
pub fn parse_gemini(status: u16, body: &str) -> ProbeResult {
    use crate::probe::unlock_util::{between, three_to_two};
    if status == 403 || status == 451 {
        return ProbeResult::new("Gemini", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        // 形如 ...,2,1,200,"USA"...
        let region = between(body, ",2,1,200,\"", "\"")
            .map(|c| three_to_two(c));
        return ProbeResult::new("Gemini", ProbeStatus::Unlocked, region);
    }
    ProbeResult::unknown("Gemini")
}

pub struct Gemini {
    pub base: String,
}
impl Default for Gemini {
    fn default() -> Self { Gemini { base: "https://gemini.google.com".to_string() } }
}

#[async_trait]
impl Probe for Gemini {
    fn name(&self) -> &'static str { "Gemini" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_gemini(status, &body)
            }
            Err(_) => ProbeResult::unknown("Gemini"),
        }
    }
}
```

在 `all_probes()` 加 `Box::new(ai::Gemini::default()),`。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test probe::ai`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/probe/ai.rs src/probe/mod.rs
git commit -m "feat(probe): Gemini 解锁探针(含地区解析)"
```

---

### Task 5: 新建 `probe/web.rs` + Bing 探针

**Files:**
- Create: `src/probe/web.rs`
- Modify: `src/probe/mod.rs`（`pub mod web;` + 注册）
- Test: `src/probe/web.rs`

- [ ] **Step 1: 写文件骨架 + Bing + 失败测试**

创建 `src/probe/web.rs`:

```rust
use async_trait::async_trait;
use reqwest::Client;
use crate::probe::{Probe, ProbeResult, ProbeStatus};
use crate::probe::unlock_util::{between, three_to_two, extract_cookie};

// ===== Bing =====
// GET bing.com → 200 解析 Region:"XX";cn.bing.com → cn;403/451=封锁。
pub fn parse_bing(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Bing", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        if body.contains("cn.bing.com") {
            return ProbeResult::new("Bing", ProbeStatus::Unlocked, Some("cn".into()));
        }
        let region = between(body, "Region:\"", "\"").map(|r| r.to_lowercase());
        return ProbeResult::new("Bing", ProbeStatus::Unlocked, region);
    }
    ProbeResult::unknown("Bing")
}

pub struct Bing { pub base: String }
impl Default for Bing {
    fn default() -> Self { Bing { base: "https://www.bing.com".to_string() } }
}
#[async_trait]
impl Probe for Bing {
    fn name(&self) -> &'static str { "Bing" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_bing(st, &body)
            }
            Err(_) => ProbeResult::unknown("Bing"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bing_region_parse() {
        let r = parse_bing(200, r#"x Region:"US" y"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        assert_eq!(parse_bing(403, "").status, ProbeStatus::Blocked);
    }
}
```

> `use ... {between, three_to_two, extract_cookie}` 中暂未用到的 `three_to_two`/`extract_cookie` 会触发 unused 警告——后续 Task(Apple/Steam)会用。为避免警告,本 Task 先只 `use {between}`,在引入 Apple/Steam 时再补 import。

在 `src/probe/mod.rs` 加 `pub mod web;` 与 `all_probes()` 加 `Box::new(web::Bing::default()),`。

- [ ] **Step 2: 运行确认通过**

Run: `cargo test probe::web`
Expected: PASS（1 测试)。

- [ ] **Step 3: 提交**

```bash
git add src/probe/web.rs src/probe/mod.rs
git commit -m "feat(probe): 新建 web.rs + Bing 探针"
```

---

### Task 6-11: web.rs 其余 6 探针(逐个 TDD,沿用 Task 5 模板)

对以下每个探针,重复:写 `parse_*` 纯函数测试 → 失败 → 实现 struct+parse+Probe → 注册 `all_probes()` → 通过 → 提交。每个独立一个提交。**端点与判定如下:**

- [ ] **Task 6: GoogleSearch** — `https://www.google.com/search?q=ipano-probe-check`。逻辑:429 → `Unknown` + `.with_info("Rate Limited")`;body 含 "unusual traffic from" 或 403/451 → `Blocked`;200 → `Unlocked`。无 region。测试覆盖三分支。

  ```rust
  pub fn parse_google_search(status: u16, body: &str) -> ProbeResult {
      if status == 429 { return ProbeResult::new("GoogleSearch", ProbeStatus::Unknown, None).with_info("Rate Limited"); }
      if status == 403 || status == 451 || body.contains("unusual traffic from") {
          return ProbeResult::new("GoogleSearch", ProbeStatus::Blocked, None);
      }
      if status == 200 { return ProbeResult::new("GoogleSearch", ProbeStatus::Unlocked, None); }
      ProbeResult::unknown("GoogleSearch")
  }
  ```
  commit: `feat(probe): GoogleSearch 解锁探针`

- [ ] **Task 7: Reddit** — `https://www.reddit.com/`。逻辑:429 → `Unknown` + info "Rate Limited";200/302 → `Unlocked`;403 且 body 含 "been blocked" → `Blocked`;其余 `Unknown`。无 region。

  ```rust
  pub fn parse_reddit(status: u16, body: &str) -> ProbeResult {
      if status == 429 { return ProbeResult::new("Reddit", ProbeStatus::Unknown, None).with_info("Rate Limited"); }
      if status == 200 || status == 302 { return ProbeResult::new("Reddit", ProbeStatus::Unlocked, None); }
      if status == 403 && body.contains("been blocked") { return ProbeResult::new("Reddit", ProbeStatus::Blocked, None); }
      ProbeResult::unknown("Reddit")
  }
  ```
  > 注:reqwest 默认跟随重定向,302 可能已被跟随成 200——`check()` 里用默认 client 即可,测试用 `parse_reddit` 纯函数直接覆盖 302 分支。
  commit: `feat(probe): Reddit 解锁探针`

- [ ] **Task 8: Wikipedia(可编辑性)** — `https://zh.wikipedia.org/w/index.php?title=Wikipedia:%E6%B2%99%E7%9B%92&action=edit`。逻辑:200 → `Unlocked`(可编辑);429 → `Unknown` + info "Rate Limited";其余(403 等)→ `Blocked`。名称 `"Wikipedia"`,无 region。

  ```rust
  pub fn parse_wikipedia(status: u16) -> ProbeResult {
      match status {
          200 => ProbeResult::new("Wikipedia", ProbeStatus::Unlocked, None),
          429 => ProbeResult::new("Wikipedia", ProbeStatus::Unknown, None).with_info("Rate Limited"),
          _ => ProbeResult::new("Wikipedia", ProbeStatus::Blocked, None),
      }
  }
  ```
  commit: `feat(probe): Wikipedia 可编辑性探针`

- [ ] **Task 9: OneTrust** — `https://geolocation.onetrust.com/cookieconsentpub/v1/geo/location/dnsfeed`。逻辑:200 解析 body 中 `"country":"XX"`(用 `between(body, "\"country\":\"", "\"")`),有则 `Unlocked` + region=country(若另有 `"stateName":"…"` 则 region=`"country stateName"`);无 country → `Blocked`。

  ```rust
  pub fn parse_onetrust(status: u16, body: &str) -> ProbeResult {
      if status != 200 { return ProbeResult::unknown("OneTrust"); }
      let country = between(body, "\"country\":\"", "\"");
      match country {
          Some(c) => {
              let region = match between(body, "\"stateName\":\"", "\"") {
                  Some(s) if !s.is_empty() => format!("{c} {s}"),
                  _ => c.to_string(),
              };
              ProbeResult::new("OneTrust", ProbeStatus::Unlocked, Some(region))
          }
          None => ProbeResult::new("OneTrust", ProbeStatus::Blocked, None),
      }
  }
  ```
  commit: `feat(probe): OneTrust 地理探针`

- [ ] **Task 10: Apple** — `https://gspe1-ssl.ls.apple.com/pep/gcc`。该端点返回**纯两字母国家码**(body 即如 `US`)。逻辑:200 且 body trim 后为 2 字母 → `Unlocked` region=body;否则 `Blocked`。

  ```rust
  pub fn parse_apple(status: u16, body: &str) -> ProbeResult {
      let code = body.trim();
      if status == 200 && code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) {
          return ProbeResult::new("Apple", ProbeStatus::Unlocked, Some(code.to_lowercase()));
      }
      ProbeResult::new("Apple", ProbeStatus::Blocked, None)
  }
  ```
  commit: `feat(probe): Apple 区域探针`

- [ ] **Task 11: Steam** — `https://store.steampowered.com/`。从响应 `Set-Cookie` 头取 `steamCountry=`(值形如 `US%7C…`,取前 2 字符)→ `Unlocked` region + `.with_info("Community Available")`;无 cookie → `Blocked`。**需读响应头**,故 `check()` 取 `resp.headers().get_all("set-cookie")` 拼接传入 parse。

  ```rust
  pub fn parse_steam(set_cookie: &str) -> ProbeResult {
      match extract_cookie(set_cookie, "steamCountry") {
          Some(v) if v.len() >= 2 => ProbeResult::new("Steam", ProbeStatus::Unlocked,
              Some(v[..2].to_lowercase())).with_info("Community Available"),
          _ => ProbeResult::new("Steam", ProbeStatus::Blocked, None),
      }
  }
  ```
  `check()` 内:
  ```rust
  let cookies = resp.headers().get_all("set-cookie").iter()
      .filter_map(|v| v.to_str().ok()).collect::<Vec<_>>().join("; ");
  parse_steam(&cookies)
  ```
  测试用 `parse_steam("steamCountry=US%7Cabc; path=/")` 断言 region "us" + info。
  commit: `feat(probe): Steam 商店区域探针`

> 完成 Task 6-11 后,确保 `web.rs` 顶部 `use` 已含 `between/extract_cookie`(`three_to_two` web.rs 未用,移除以免警告)。每个 Task 末尾跑 `cargo test probe::web` 应全绿。

---

### Task 12-15: streaming.rs 亚洲媒体 + TikTok(逐个 TDD)

在 `src/probe/streaming.rs` 加,沿用该文件现有 `classify_*` + struct 模式(参考 `classify_youtube`)。每个独立提交。

- [ ] **Task 12: IQiYi** — `https://www.iq.com`。简化逻辑:200 → `Unlocked`(region 解析复杂,本阶段先不取 region,留空);403/451 → `Blocked`;其余 `Unknown`。

  ```rust
  pub fn classify_iqiyi(status: u16) -> ProbeResult {
      match status {
          200 => ProbeResult::new("iQIYI", ProbeStatus::Unlocked, None),
          403 | 451 => ProbeResult::new("iQIYI", ProbeStatus::Blocked, None),
          _ => ProbeResult::unknown("iQIYI"),
      }
  }
  ```
  commit: `feat(probe): iQIYI 解锁探针`

- [ ] **Task 13: KOCOWA** — `https://www.kocowa.com/`。逻辑:body 含 "is not available in your region or country" 或 403 → `Blocked`;200 → `Unlocked`;其余 `Unknown`。

  ```rust
  pub fn parse_kocowa(status: u16, body: &str) -> ProbeResult {
      if status == 403 || body.contains("is not available in your region or country") {
          return ProbeResult::new("KOCOWA", ProbeStatus::Blocked, None);
      }
      if status == 200 { return ProbeResult::new("KOCOWA", ProbeStatus::Unlocked, None); }
      ProbeResult::unknown("KOCOWA")
  }
  ```
  commit: `feat(probe): KOCOWA 解锁探针`

- [ ] **Task 14: Viu** — `https://www.viu.com`。简化:200 → `Unlocked`(region 复杂,留空);403/451 → `Blocked`;其余 `Unknown`。

  ```rust
  pub fn classify_viu(status: u16) -> ProbeResult {
      match status {
          200 => ProbeResult::new("Viu", ProbeStatus::Unlocked, None),
          403 | 451 => ProbeResult::new("Viu", ProbeStatus::Blocked, None),
          _ => ProbeResult::unknown("Viu"),
      }
  }
  ```
  commit: `feat(probe): Viu 解锁探针`

- [ ] **Task 15: TikTok** — `https://www.tiktok.com/explore`。逻辑:200 且 body 含 `https://www.tiktok.com/hk/notfound` → `Blocked` region "hk";200 且解析 `"region":"XX"`(用 `crate::probe::unlock_util::between(body, "\"region\":\"", "\"")`)→ `Unlocked` region(小写);非 200 → `Blocked`。

  ```rust
  pub fn parse_tiktok(status: u16, body: &str) -> ProbeResult {
      use crate::probe::unlock_util::between;
      if status != 200 { return ProbeResult::new("TikTok", ProbeStatus::Blocked, None); }
      if body.contains("https://www.tiktok.com/hk/notfound") {
          return ProbeResult::new("TikTok", ProbeStatus::Blocked, Some("hk".into()));
      }
      match between(body, "\"region\":\"", "\"") {
          Some(r) if !r.is_empty() => ProbeResult::new("TikTok", ProbeStatus::Unlocked, Some(r.to_lowercase())),
          _ => ProbeResult::new("TikTok", ProbeStatus::Blocked, None),
      }
  }
  ```
  测试:`parse_tiktok(200, r#"..."region":"US"..."#)` → Unlocked region "us";含 notfound → Blocked "hk"。
  commit: `feat(probe): TikTok 解锁探针(含地区)`

> 每个 Task 在 `all_probes()` 注册对应 `Box::new(streaming::IQiYi::default()),` 等(struct 名:`IQiYi`/`Kocowa`/`Viu`/`TikTok`,各加 `pub base:String` + `Default` 真实域名 + `impl Probe`,模板同 Bing)。

---

### Task 16: 注册核对 + 实跑核实 + 全量回归 + 文档

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: 核对 `all_probes()` 注册数**

确认 `src/probe/mod.rs` 的 `all_probes()` 已含全部 13 新探针(Claude/Gemini/Bing/GoogleSearch/Reddit/Wikipedia/OneTrust/Apple/Steam/iQIYI/KOCOWA/Viu/TikTok)。原 19 + 13 = 32。

- [ ] **Step 2: 全量测试**

Run: `cargo test`
Expected: PASS,无失败。

- [ ] **Step 3: 真机实跑核实(关键——抓取探针端点会变)**

Run: `cargo build --release && ./target/release/ipano 1.1.1.1 --probe 2>&1 | grep -E "Claude|Gemini|Bing|Reddit|Wikipedia|OneTrust|Apple|Steam|TikTok|iQIYI|KOCOWA|Viu"`
Expected: 各探针出现在解锁表,状态合理(美国 IP 下 Claude/Bing/Reddit/Wikipedia 应 ✓ 解锁)。

> **若某探针结果明显异常**(如全 Unknown 或 region 乱码),curl 该端点看真实响应,微调 `parse_*` 的子串/状态判定与样本。抓取探针端点随服务变动,以实跑为准——这是验收关键步骤,不可跳过。

- [ ] **Step 4: CHANGELOG**

在 `CHANGELOG.md` 的「未发布 v0.19.0」块下追加:

```markdown
### 阶段 B — 解锁探针扩充(13 简单探针)
- AI:Claude、Gemini(含地区)。
- 搜索/工具/商店:Bing、GoogleSearch、Reddit、Wikipedia(可编辑性)、OneTrust、Apple(区域)、Steam(社区+区域)。
- 亚洲流媒体 + 短视频:iQIYI、KOCOWA、Viu、TikTok(含地区)。
- `ProbeResult` 增 `info` 备注字段,解锁表加「备注」列(如 Community Available / Rate Limited)。
```

- [ ] **Step 5: 提交**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): 记录 v0.19.0 阶段 B 解锁探针扩充"
```

---

## Self-Review

- **Spec 覆盖**:阶段 B 简单探针(spec 分阶段表)= Claude/Gemini + Bing/GoogleSearch/Reddit/Wikipedia/OneTrust/Apple/Steam + IQiYi/KOCOWA/Viu/TikTok 共 13,逐一有 Task(3/4/5/6-11/12-15)。`ProbeResult.info` = Task 1。`unlock_util` = Task 2。MetaAI/SonyLiv/InstagramMusic/GooglePlay/CDN **明确留阶段 C**,本计划不含——与 spec 一致。
- **占位扫描**:无 TBD;每探针含 `parse_*`/`classify_*` 完整代码、端点、提交信息。Task 6-15 沿用 Task 3-5 已展示的 struct+Probe 模板(同文件同模式),并给出各自判定代码,非空泛「类似 Task N」。
- **类型一致**:`parse_*`/`classify_*` 均返回 `ProbeResult`;`with_info` 返回 `Self`;`between`/`extract_cookie`/`three_to_two` 签名与 Task 2 定义一致;struct 均 `{ pub base: String }` + `Default` + `impl Probe`。
- **风险**:抓取探针的真实响应结构无法在写计划时完全确定——Task 16 Step 3 强制实跑核实并微调,已在计划内显式兜底(与 spec「脆弱性接受 + 失败降级 Unknown」一致)。
- **范围**:纯探针新增 + 渲染加列,不动 IP 质量/merge;独立可交付(`--probe` 自带开关)。
