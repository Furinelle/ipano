# UnlockTests 复杂探针 + CDN 定位(阶段 C)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development 或 superpowers:executing-plans 逐 Task 执行。Steps 用 checkbox(`- [ ]`)。

**Goal:** 补齐 v0.19.0 最后一批解锁探针——4 个复杂多步探针(MetaAI / SonyLiv / GooglePlay / InstagramMusic)+ 2 个 CDN 定位(Netflix CDN / YouTube CDN),探针总数 32 → 38;随后 bump 版本 0.18.0 → 0.19.0 并定稿 CHANGELOG。

**Architecture:** 沿用阶段 B 的 `trait Probe` + `parse_*` 纯函数 + httpmock 模式。复杂探针端点为 HTML/接口抓取且**比阶段 B 更脆弱**(SonyLiv 多步 JWT、Instagram POST 带会过期的 `doc_id`、MetaAI ajax 反直觉状态码)——本计划按 spec「不可得即降级」原则做**单请求简化版**:抓核心信号,任何非预期响应降级 `Unknown`,绝不伪造。CDN 两个忠实移植(Netflix 为干净 JSON;YouTube report_mapping 为文本,简化解析)。复杂探针进 `probe/web.rs`,CDN 进**新** `probe/cdn.rs`。

**Tech Stack:** Rust、reqwest(GET + 一处 POST form)、serde_json(Netflix CDN JSON)、httpmock。复用 `probe/unlock_util.rs::between`。

**前置事实(实现者必读):**
- 探针模板见已实现的 `src/probe/web.rs`(如 `Bing`/`GoogleSearch`):`struct{base}` + `Default`(真实域名)+ `impl Probe` + `parse_*` 纯函数 + 测试。
- `ProbeResult::new(name,status,region)` / `unknown(name)` / `.with_info(s)` 已存在。`ProbeStatus`:Unlocked/Restricted/Blocked/Unknown。
- `between(body, prefix, suffix) -> Option<&str>` 在 `crate::probe::unlock_util`。
- HTTP client:`crate::fetch::build_client(secs)`(测试用);探针 `check()` 收 `&Client`。
- `all_probes()` 在 `src/probe/mod.rs` 末尾;当前 32 个 `Box::new`。
- 测试命令:`cargo test`(BINARY crate,无 lib;用 `cargo test probe::web` / `probe::cdn` / `<name>`)。
- serde_json 已是依赖(渲染层在用)。

---

### Task 1: 新建 `probe/cdn.rs` + Netflix CDN

**Files:** Create `src/probe/cdn.rs`;Modify `src/probe/mod.rs`(`pub mod cdn;` + 注册);Test in `cdn.rs`.

- [ ] **Step 1: 写文件 + 失败测试**

创建 `src/probe/cdn.rs`:

```rust
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::probe::{Probe, ProbeResult, ProbeStatus};

// ===== Netflix CDN =====
// GET api.fast.com/netflix/speedtest/v2 → JSON targets[0].location.country。
// 403/451 = IP 被 Netflix 封禁。token 为 fast.com 公开测速 token。
#[derive(Deserialize)]
struct FastLocation { country: Option<String> }
#[derive(Deserialize)]
struct FastTarget { location: Option<FastLocation> }
#[derive(Deserialize)]
struct FastResp { targets: Option<Vec<FastTarget>> }

pub fn parse_netflix_cdn(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Netflix CDN", ProbeStatus::Blocked, None).with_info("IP Banned By Netflix");
    }
    if status == 200 {
        if let Ok(r) = serde_json::from_str::<FastResp>(body) {
            if let Some(country) = r.targets.and_then(|t| t.into_iter().next())
                .and_then(|t| t.location).and_then(|l| l.country) {
                if !country.is_empty() {
                    return ProbeResult::new("Netflix CDN", ProbeStatus::Unlocked, Some(country.to_lowercase()));
                }
            }
        }
    }
    ProbeResult::unknown("Netflix CDN")
}

pub struct NetflixCdn { pub base: String }
impl Default for NetflixCdn {
    fn default() -> Self { NetflixCdn { base: "https://api.fast.com".to_string() } }
}
#[async_trait]
impl Probe for NetflixCdn {
    fn name(&self) -> &'static str { "Netflix CDN" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/netflix/speedtest/v2?https=true&token=YXNkZmFzZGxmbnNkYWZoYXNkZmhrYWxm&urlCount=5", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_netflix_cdn(st, &body)
            }
            Err(_) => ProbeResult::unknown("Netflix CDN"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netflix_cdn_parse() {
        let body = r#"{"targets":[{"location":{"city":"LA","country":"US"}}]}"#;
        let r = parse_netflix_cdn(200, body);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        let b = parse_netflix_cdn(403, "");
        assert_eq!(b.status, ProbeStatus::Blocked);
        assert_eq!(b.info.as_deref(), Some("IP Banned By Netflix"));
        assert_eq!(parse_netflix_cdn(200, "{}").status, ProbeStatus::Unknown);
    }
}
```

在 `src/probe/mod.rs` 加 `pub mod cdn;`(模块声明区)+ `all_probes()` 末尾加 `Box::new(cdn::NetflixCdn::default()),`。

- [ ] **Step 2:** `cargo test probe::cdn` → PASS(1 test)。
- [ ] **Step 3: commit** `git add src/probe/cdn.rs src/probe/mod.rs && git commit -m "feat(probe): 新建 cdn.rs + Netflix CDN 定位(api.fast.com)"`

---

### Task 2: YouTube CDN(cdn.rs)

**Files:** Modify `src/probe/cdn.rs`、`src/probe/mod.rs`;Test in `cdn.rs`.

- [ ] **Step 1: 失败测试** — 追加到 `cdn.rs` 的 `mod tests`:

```rust
    #[test]
    fn youtube_cdn_parse() {
        // report_mapping 文本含 router/host 信息;200 非空 = 可达。
        let r = parse_youtube_cdn(200, "router 1.2.3.4 => sault.<...>.googlevideo.com");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert!(r.info.is_some());
        assert_eq!(parse_youtube_cdn(403, "").status, ProbeStatus::Blocked);
        assert_eq!(parse_youtube_cdn(200, "").status, ProbeStatus::Unknown);
    }
```

- [ ] **Step 2:** `cargo test youtube_cdn_parse` → FAIL(未定义)。
- [ ] **Step 3: 实现** — 加到 `cdn.rs`(Netflix CDN 之后,`#[cfg(test)]` 之前):

```rust
// ===== YouTube CDN =====
// GET redirector.googlevideo.com/report_mapping → 文本响应,含落地 CDN 节点信息。
// 简化:200+非空 = 可达(info 给 CDN 描述);403/451 = 封锁;空/异常 = Unknown。
// 注:report_mapping 文本格式随时变动,以实跑为准(见 Task 7)。
pub fn parse_youtube_cdn(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("YouTube CDN", ProbeStatus::Blocked, None);
    }
    if status == 200 && !body.trim().is_empty() {
        let info = if body.contains("=>") { "GGC / Video Server" } else { "Reachable" };
        return ProbeResult::new("YouTube CDN", ProbeStatus::Unlocked, None).with_info(info);
    }
    ProbeResult::unknown("YouTube CDN")
}

pub struct YoutubeCdn { pub base: String }
impl Default for YoutubeCdn {
    fn default() -> Self { YoutubeCdn { base: "https://redirector.googlevideo.com".to_string() } }
}
#[async_trait]
impl Probe for YoutubeCdn {
    fn name(&self) -> &'static str { "YouTube CDN" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/report_mapping", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_youtube_cdn(st, &body)
            }
            Err(_) => ProbeResult::unknown("YouTube CDN"),
        }
    }
}
```

注册 `Box::new(cdn::YoutubeCdn::default()),`。

- [ ] **Step 4:** `cargo test probe::cdn` → PASS。
- [ ] **Step 5: commit** `git add src/probe/cdn.rs src/probe/mod.rs && git commit -m "feat(probe): YouTube CDN 定位(redirector.googlevideo.com)"`

---

### Task 3: MetaAI(web.rs)

> 端点反直觉:`meta.ai/ajax` 返回 **400/404 = 可用**、**200 = 地区封锁(GeoBlocked)**、**403 = 封锁**。简化为单请求,不做 meta.com/legal region 解析。

**Files:** Modify `src/probe/web.rs`、`src/probe/mod.rs`;Test in `web.rs`.

- [ ] **Step 1: 失败测试**(追加 web.rs `mod tests`):

```rust
    #[test]
    fn meta_ai_branches() {
        assert_eq!(parse_meta_ai(404).status, ProbeStatus::Unlocked);
        assert_eq!(parse_meta_ai(400).status, ProbeStatus::Unlocked);
        assert_eq!(parse_meta_ai(200).status, ProbeStatus::Blocked);
        assert_eq!(parse_meta_ai(200).info.as_deref(), Some("GeoBlocked"));
        assert_eq!(parse_meta_ai(403).status, ProbeStatus::Blocked);
        assert_eq!(parse_meta_ai(500).status, ProbeStatus::Unknown);
    }
```

- [ ] **Step 2:** `cargo test meta_ai_branches` → FAIL。
- [ ] **Step 3: 实现**(web.rs,Steam 之后,`#[cfg(test)]` 之前):

```rust
// ===== MetaAI =====
// GET meta.ai/ajax(带浏览器 UA)→ 400/404=可用;200=地区封锁;403=封锁。
pub fn parse_meta_ai(status: u16) -> ProbeResult {
    match status {
        400 | 404 => ProbeResult::new("MetaAI", ProbeStatus::Unlocked, None),
        200 => ProbeResult::new("MetaAI", ProbeStatus::Blocked, None).with_info("GeoBlocked"),
        403 | 451 => ProbeResult::new("MetaAI", ProbeStatus::Blocked, None),
        _ => ProbeResult::unknown("MetaAI"),
    }
}
pub struct MetaAI { pub base: String }
impl Default for MetaAI {
    fn default() -> Self { MetaAI { base: "https://www.meta.ai".to_string() } }
}
#[async_trait]
impl Probe for MetaAI {
    fn name(&self) -> &'static str { "MetaAI" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/ajax", self.base);
        let req = client.get(&url)
            .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36");
        match req.send().await {
            Ok(resp) => parse_meta_ai(resp.status().as_u16()),
            Err(_) => ProbeResult::unknown("MetaAI"),
        }
    }
}
```

注册 `Box::new(web::MetaAI::default()),`。

- [ ] **Step 4:** `cargo test probe::web` → PASS。
- [ ] **Step 5: commit** `git commit -am "feat(probe): MetaAI 解锁探针(简化单请求)"`

---

### Task 4: SonyLiv(web.rs)

> 简化:单请求 `sonyliv.com/signin`,解析 `country_code:"XX"`。不做多步 JWT。

**Files:** Modify `src/probe/web.rs`、`src/probe/mod.rs`.

- [ ] **Step 1: 失败测试:**

```rust
    #[test]
    fn sonyliv_branches() {
        let r = parse_sonyliv(200, r#"...country_code:"IN"..."#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("in"));
        assert_eq!(parse_sonyliv(403, "").status, ProbeStatus::Blocked);
        assert_eq!(parse_sonyliv(200, "no code here").status, ProbeStatus::Unknown);
    }
```

- [ ] **Step 2:** `cargo test sonyliv_branches` → FAIL。
- [ ] **Step 3: 实现:**

```rust
// ===== SonyLiv =====
// GET sonyliv.com/signin → 403=封锁;200 解析 country_code:"XX"=可用;无码=Unknown。
pub fn parse_sonyliv(status: u16, body: &str) -> ProbeResult {
    if status == 403 { return ProbeResult::new("SonyLiv", ProbeStatus::Blocked, None); }
    if status == 200 {
        if let Some(cc) = between(body, "country_code:\"", "\"") {
            return ProbeResult::new("SonyLiv", ProbeStatus::Unlocked, Some(cc.to_lowercase()));
        }
    }
    ProbeResult::unknown("SonyLiv")
}
pub struct SonyLiv { pub base: String }
impl Default for SonyLiv {
    fn default() -> Self { SonyLiv { base: "https://www.sonyliv.com".to_string() } }
}
#[async_trait]
impl Probe for SonyLiv {
    fn name(&self) -> &'static str { "SonyLiv" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/signin", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_sonyliv(st, &body)
            }
            Err(_) => ProbeResult::unknown("SonyLiv"),
        }
    }
}
```

注册 `Box::new(web::SonyLiv::default()),`。

- [ ] **Step 4-5:** `cargo test probe::web` → PASS;`git commit -am "feat(probe): SonyLiv 解锁探针(简化单请求)"`

---

### Task 5: GooglePlay(web.rs)

> 解析两种 region 模式:`"zQmIje":"XX"` 或 `<div class="yVZQTb">XX<`。region=cn → 封锁。

- [ ] **Step 1: 失败测试:**

```rust
    #[test]
    fn google_play_branches() {
        let r = parse_google_play(200, r#"x "zQmIje":"US" y"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        assert_eq!(parse_google_play(200, r#""zQmIje":"CN""#).status, ProbeStatus::Blocked);
        assert_eq!(parse_google_play(200, "nothing").status, ProbeStatus::Blocked);
        assert_eq!(parse_google_play(500, "").status, ProbeStatus::Unknown);
    }
```

- [ ] **Step 2:** FAIL。
- [ ] **Step 3: 实现:**

```rust
// ===== GooglePlay =====
// GET play.google.com/store/games → 解析 region(两种模式);cn=封锁;有码=可用。
fn extract_google_play_region(body: &str) -> Option<String> {
    between(body, "\"zQmIje\":\"", "\"")
        .or_else(|| between(body, "<div class=\"yVZQTb\">", "<"))
        .map(|s| s.trim().to_string())
}
pub fn parse_google_play(status: u16, body: &str) -> ProbeResult {
    if status != 200 { return ProbeResult::unknown("GooglePlay"); }
    match extract_google_play_region(body) {
        Some(r) if r.eq_ignore_ascii_case("cn") =>
            ProbeResult::new("GooglePlay", ProbeStatus::Blocked, Some("cn".into())),
        Some(r) if !r.is_empty() =>
            ProbeResult::new("GooglePlay", ProbeStatus::Unlocked, Some(r.to_lowercase())),
        _ => ProbeResult::new("GooglePlay", ProbeStatus::Blocked, None),
    }
}
pub struct GooglePlay { pub base: String }
impl Default for GooglePlay {
    fn default() -> Self { GooglePlay { base: "https://play.google.com".to_string() } }
}
#[async_trait]
impl Probe for GooglePlay {
    fn name(&self) -> &'static str { "GooglePlay" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/store/games", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_google_play(st, &body)
            }
            Err(_) => ProbeResult::unknown("GooglePlay"),
        }
    }
}
```

注册 `Box::new(web::GooglePlay::default()),`。

- [ ] **Step 4-5:** `cargo test probe::web` → PASS;`git commit -am "feat(probe): GooglePlay 解锁探针"`

---

### Task 6: InstagramMusic(web.rs)— POST,最脆弱

> POST `instagram.com/api/graphql` 带固定 payload(含 `doc_id`,**会过期**)。200+无错误标记=可用;429=限流;其余=Unknown。payload 失效时整体降级,以实跑为准。

- [ ] **Step 1: 失败测试**(只测纯函数 parse,不测真 POST):

```rust
    #[test]
    fn instagram_branches() {
        assert_eq!(parse_instagram(200, r#"{"data":{"xdt_api__v1__media__shortcode__web_info":{}}}"#).status, ProbeStatus::Unlocked);
        assert_eq!(parse_instagram(200, r#"{"errors":["login_required"]}"#).status, ProbeStatus::Blocked);
        assert_eq!(parse_instagram(429, "").status, ProbeStatus::Unknown);
        assert_eq!(parse_instagram(429, "").info.as_deref(), Some("Too Many Requests"));
        assert_eq!(parse_instagram(500, "").status, ProbeStatus::Unknown);
    }
```

- [ ] **Step 2:** FAIL。
- [ ] **Step 3: 实现:**

```rust
// ===== InstagramMusic(授权音频)=====
// POST instagram.com/api/graphql(固定 payload,含会过期的 doc_id)。
// 200+含媒体数据=可用;200+错误/login=封锁;429=限流;其余=Unknown。
const IG_PAYLOAD: &str = "av=0&__d=www&__user=0&__a=1&__req=3&doc_id=10015901848480474&variables=%7B%22shortcode%22%3A%22C2YEAdOh9AB%22%7D&fb_api_req_friendly_name=PolarisPostActionLoadPostQueryQuery&server_timestamps=true";

pub fn parse_instagram(status: u16, body: &str) -> ProbeResult {
    if status == 429 {
        return ProbeResult::new("InstagramMusic", ProbeStatus::Unknown, None).with_info("Too Many Requests");
    }
    if status == 200 {
        if body.contains("login_required") || body.contains("\"errors\"") || body.contains("usepc") {
            return ProbeResult::new("InstagramMusic", ProbeStatus::Blocked, None);
        }
        if body.contains("xdt_api") || body.contains("\"data\"") {
            return ProbeResult::new("InstagramMusic", ProbeStatus::Unlocked, None);
        }
    }
    ProbeResult::unknown("InstagramMusic")
}
pub struct InstagramMusic { pub base: String }
impl Default for InstagramMusic {
    fn default() -> Self { InstagramMusic { base: "https://www.instagram.com".to_string() } }
}
#[async_trait]
impl Probe for InstagramMusic {
    fn name(&self) -> &'static str { "InstagramMusic" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/api/graphql", self.base);
        let req = client.post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(reqwest::header::ORIGIN, "https://www.instagram.com")
            .header(reqwest::header::REFERER, "https://www.instagram.com/p/C2YEAdOh9AB/")
            .header("X-FB-Friendly-Name", "PolarisPostActionLoadPostQueryQuery")
            .body(IG_PAYLOAD);
        match req.send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_instagram(st, &body)
            }
            Err(_) => ProbeResult::unknown("InstagramMusic"),
        }
    }
}
```

注册 `Box::new(web::InstagramMusic::default()),`。

- [ ] **Step 4-5:** `cargo test probe::web` → PASS;`git commit -am "feat(probe): InstagramMusic 授权音频探针(POST graphql, 脆弱)"`

---

### Task 7: 注册核对 + 实跑核实 + 版本定稿

**Files:** Modify `Cargo.toml`、`CHANGELOG.md`、`README.md`.

- [ ] **Step 1: 核对注册数** — `grep -c 'Box::new' src/probe/mod.rs` 应为 38(32 + 6)。
- [ ] **Step 2: 全量测试** — `cargo test` → PASS(预期 ≥ 269 = 258 + ~11 新测试)。
- [ ] **Step 3: 实跑核实(关键)** — `cargo build --release && ./target/release/ipano --probe 2>&1 | grep -E "MetaAI|SonyLiv|GooglePlay|InstagramMusic|Netflix CDN|YouTube CDN"`。
  - 各探针应出现且状态合理。**若某探针全 Unknown 或明显异常**(尤其 InstagramMusic 的 doc_id 可能已失效、YouTube CDN 文本格式可能变),curl 该端点看真实响应,微调 `parse_*` 与样本;无法修复者保留降级 Unknown 并在 CHANGELOG/README 注明该探针「端点不稳定」。不可跳过本步。
- [ ] **Step 4: bump 版本** — `Cargo.toml` 的 `version = "0.18.0"` 改为 `version = "0.19.0"`;运行 `cargo build` 让 `Cargo.lock` 同步。
- [ ] **Step 5: 定稿 CHANGELOG** — 把 `## [未发布] v0.19.0 进行中` 改为 `## [0.19.0] - 2026-06-14`,并在其下追加阶段 C 段:

```markdown
### 阶段 C — 复杂探针 + CDN 定位(探针 32→38)

- 复杂探针(简化单请求,对标 UnlockTests):MetaAI、SonyLiv、GooglePlay、InstagramMusic(POST graphql,端点较脆弱)。
- CDN 定位(新 `probe/cdn.rs`):Netflix CDN(api.fast.com,落地国家)、YouTube CDN(redirector.googlevideo.com,落地节点)。
- `--probe` 探针总数 19 → 38(本版 A+B+C 合计)。
```

- [ ] **Step 6: README 探针数更新** — 在 README 解锁检测段把探针数(如「19 项」)更新为「38 项」,并补一句:复杂/CDN 探针为公开端点启发式,InstagramMusic/YouTube CDN 端点不稳定、可能降级 Unknown。
- [ ] **Step 7: commit** `git add -A && git commit -m "chore(release): v0.19.0 探针 38 项 + CHANGELOG/README 定稿"`

---

## Self-Review

- **Spec 覆盖**:阶段 C(spec 分阶段表)= MetaAI/SonyLiv/InstagramMusic/GooglePlay + Netflix CDN/YouTube CDN,逐一有 Task(3/4/6/5/1/2)。版本 bump + 文档定稿 = Task 7。
- **占位扫描**:无 TBD;每探针含 `parse_*` 完整代码 + 端点 + 提交信息。
- **类型一致**:`parse_*` 返回 `ProbeResult`;`between` 复用;CDN 用 serde_json 结构体;POST 用 reqwest `.post().body()`。
- **简化与诚实**:SonyLiv 去 JWT 多步、MetaAI 去 legal-region、InstagramMusic 用可能过期的 doc_id、YouTube CDN 简化文本解析——均按 spec「不可得即降级 Unknown」原则,Task 7 Step 3 强制实跑核实并允许保留降级 + 文档注明。不伪造。
- **范围**:6 探针 + 版本定稿;不动 IP 质量/merge。完成后 v0.19.0(A+B+C)可发布。
