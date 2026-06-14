# 设计:IP 测试对标融合怪 —— IP 质量全字段渲染 + UnlockTests 解锁探针扩充

- **日期**:2026-06-14
- **状态**:设计已确认,待写实现计划
- **目标版本**:v0.19.0
- **对标**:[oneclickvirt/UnlockTests](https://github.com/oneclickvirt/UnlockTests)(解锁检测)+ [spiritysdx/ecs.sh 融合怪](https://gitlab.com/spiritysdx/za)(综合 IP 测试输出)

## 背景与问题

对比融合怪(ecs.sh)的完整输出,ipano 在「有关 IP 的测试」上有两类缺口:

1. **IP 质量字段渲染不全**:`SourceData`/`MergedReport` 的 model **已含**字段,但 `--raw`/默认报告/JSON 的渲染层只印了子集。融合怪逐源详表里有、ipano model 有数据却没渲染的字段:浏览器分布、OS 分布、是否 Tor、是否爬虫、是否移动、是否滥用者、是否 Bogon、VT 未检出数,以及若干逐源数值分(信任/欺诈/AbuseIPDB)。

2. **解锁探针覆盖窄**:ipano `--probe` 现有 19 项(西方流媒体 + 番剧 + ChatGPT),融合怪解锁模块 ~29 项,ipano 缺 AI 多家、搜索/工具、商店/区域、亚洲流媒体、短视频、CDN 定位等。

ipano 探针架构早为扩展设计:`trait Probe` + `all_probes()` 注册 + 并发 `run_all_with_native_check`。**加探针 ≈ 加一个实现 `Probe` 的 struct**。本设计补齐渲染字段与解锁探针,使 ipano 的 IP 测试达到融合怪级别完整度。

## 决策记录

| 议题 | 选择 | 理由 / 否决项 |
|---|---|---|
| item 1 渲染范围 | **--raw + 默认报告 + JSON 三处都补** | 用户选定;model 已有数据,三处对齐才完整。 |
| item 2 探针来源 | **port 自 oneclickvirt/UnlockTests** | 用户指定参考;其检测逻辑(端点/状态码/region 解析)经实战维护。 |
| Sora | **放弃** | 用户核实 Sora 已停止服务,不接入。 |
| TikTok Region | **并入 TikTok 探针** | TikTok 探针本就解析并返回 region 字段,无需单列。 |
| 模块组织 | **按域拆分,不再扩 streaming.rs** | `streaming.rs` 已 41K;新增按域落 `ai.rs`/新 `web.rs`/新 `cdn.rs`,`streaming.rs` 仅加亚洲媒体类。 |
| ProbeResult.info | **新增 `info: Option<String>`** | 融合怪有「Community Available」「Rate Limited」「Proxy Detected」等备注,现 model 无处安放。 |
| 抓取脆弱性 | **接受,解析失败降级 Unknown** | HTML/启发式抓取本质脆弱(融合怪靠持续维护);ipano 同款,失败不拖垮整体。 |
| 范围边界 | **不爬需登录/付费内容,不做反爬绕过** | 仅复刻 UnlockTests 的公开端点探测;复杂多步(SonyLiv/Meta/Instagram)如实端口核实不可得则降级标注。 |

## 架构

```
probe/mod.rs       ProbeResult 加 info 字段;all_probes() 注册新探针
probe/ai.rs        现有 + Claude / Gemini / MetaAI
probe/web.rs       【新】Bing / GoogleSearch / GooglePlay / Reddit / Wikipedia / OneTrust / Apple / Steam / InstagramMusic
probe/streaming.rs 现有 + IQiYi / KOCOWA / Viu / SonyLiv / TikTok
probe/cdn.rs       【新】Netflix CDN / YouTube CDN
probe/unlock_util.rs 【新】region 三→二码 / cookie 解析 / 正则提取等纯函数
model.rs           (item 1)SourceData/MergedReport 字段已存在,无需新增
aggregate.rs       (item 1)merge 补合并未覆盖字段到 MergedReport
render/raw.rs      (item 1)逐源详表加缺失字段 line!
render/terminal.rs (item 1)默认报告「风险判定」面板加高信号合并字段
render/json.rs     (item 1)JSON 暴露缺失字段
```

新探针不改核心流:`run_all_with_native_check` 并发跑所有探针 → 每探针产出 `ProbeResult` → 现有流媒体表渲染(自动复用)。

## item 1:IP 质量全字段渲染补全

### 待补字段(model 已有,渲染层缺)

`--raw` 逐源详表新增 `line!`:

- `browser_dist`(浏览器分布)· `os_dist`(OS 分布)
- `is_tor`(是否 Tor)· `is_crawler`(是否爬虫)· `is_mobile`(是否移动)· `is_abuser`(是否滥用者)· `is_bogon`(是否 Bogon)· `is_hosting`(是否托管)· `is_residential`(是否住宅)
- `blacklist_undetected`(VT 未检出数)
- 逐源数值分:`trust_score`(信任)· `fraud_score`(欺诈)· `abuseipdb_score`(AbuseIPDB 滥用)

### 默认报告(terminal.rs「风险判定」面板)

补高信号**合并**字段(非逐源):是否 Tor、浏览器/OS 分布摘要、VT 未检出数。保持面板简洁,只挑用户一眼想看的;逐源细节留 `--raw`。需在 `aggregate.rs` merge 中把这些字段合并进 `MergedReport`(多数决/众数,沿用现有规则)。

### JSON(json.rs)

`sources[]` 逐源原样输出全部 `SourceData` 字段(多数已自动序列化,核对遗漏);顶层合并字段补齐。

### 不动 model

`SourceData`/`MergedReport` 字段在阶段二已建齐,item 1 **只动渲染 + merge 合并**,不加 model 字段。

## item 2:UnlockTests 解锁探针扩充

### ProbeResult 扩字段

```rust
pub struct ProbeResult {
    pub name: String,
    pub status: ProbeStatus,
    pub region: Option<String>,
    pub unlock_type: UnlockType,
    pub info: Option<String>,   // 新增:Community Available / Rate Limited / Proxy Detected 等
}
```

`ProbeResult::new`/`unknown` 默认 `info: None`。渲染表(terminal/markdown)加「备注」列,空显 `—`。JSON 自动随 `Serialize` 暴露。

### 探针检测方法(全部 port 自 UnlockTests `transnation/`)

| 探针 | 端点 | 判定要点 |
|---|---|---|
| Claude | `claude.ai/` → 200=Yes;`/cdn-cgi/trace` 取 loc 作 region,识别 TOR | 简单+region |
| Gemini | `gemini.google.com` → 200 正则 `,2,1,200,"([A-Z]{3})"` 三→二码;403/451=No | 简单+region |
| MetaAI | `meta.ai/ajax` → 400/404=Yes(`meta.com/legal/` 重定向路径取 region);403 回退 `meta.ai/` | 多步 |
| Bing | `bing.com` → 200 正则 `Region:"([^"]*)"`;cn.bing.com→cn;403/451=No | 简单+region |
| GoogleSearch | `google.com/search?q=…` → 200+body 含标记=Yes;"unusual traffic"/403/451=No;429=Rate Limited | 简单 |
| GooglePlay | `play.google.com/store/games` → 正则取 region;cn 检测 | 多步 |
| Reddit | `reddit.com/` → 200/302=Yes;403+"been blocked"=No;429=限流 | 简单 |
| Wikipedia | `zh.wikipedia.org/…&action=edit` → 200=可编辑 Yes;429=限流;否则 No | 简单(语义=可编辑) |
| OneTrust | `geolocation.onetrust.com/cookieconsentpub/v1/geo/location/dnsfeed` → 正则 country+stateName | 纯地理 |
| Apple | `gspe1-ssl.ls.apple.com/pep/gcc` → loc 作 region | 简单+region |
| Steam | `store.steampowered.com/` + `steamcommunity.com/` → cookie `steamCountry=` 取 region;社区可达→info "Community Available" | 简单+info |
| TikTok | `tiktok.com/explore` → 正则 `"region":"(\w+)"`;含 `/hk/notfound`=No(region hk) | 简单+region |
| IQiYi | `iq.com` → 取 region | 简单+region |
| KOCOWA | `kocowa.com/` → 200=Yes;403/"not available in your region"=No | 简单 |
| Viu | `viu.com` → 取末位 region | 简单+region |
| SonyLiv | `sonyliv.com/signin` 正则 `country_code:"([A-Z]{2})"`;多步 JWT + `apiv2.sonyliv.com`;403/Proxy=No | 多步 |
| InstagramMusic | POST `instagram.com/api/graphql`(需 Origin/Referer 头)→ 200+check=Yes;429=Too Many Requests | 多步+特殊头 |
| Netflix CDN | `api.fast.com/netflix/speedtest/v2?https=true&token=…&urlCount=5` → JSON `targets[0].location.country` 作 region;403/451=No | CDN 特殊解析 |
| YouTube CDN | `redirector.googlevideo.com/report_mapping` → 解析 cdnInfo 字符串(Video Server / Google Global Cache CDN) | CDN 特殊解析 |

> 实现期对每个端点 `curl` 核实可达性与响应结构;多步复杂探针(MetaAI/SonyLiv/InstagramMusic/GooglePlay)如核实后端点失效或结构大改,降级为标 Unknown 并在文档注明,不强行逆向反爬。

### 复用纯函数(probe/unlock_util.rs)

抽出可单测的纯函数:region 三字母→两字母码(ISO 3166)、cookie 字段提取(`steamCountry=`)、正则提取助手(若现有 `utils` 已有则复用)。每个探针的 `classify`/解析逻辑尽量做成纯函数,便于 httpmock 之外的单测。

### 注册与渲染

全部探针在 `all_probes()` 加 `Box::new(...)`;渲染零改(自动走现有「流媒体 & AI 解锁检测」表 + Native/DNS 推断 + 新「备注」列)。section 标题保持现有「流媒体 & AI 解锁检测」。

## 错误处理与降级

- HTML/启发式抓取脆弱:解析失败、超时、意外状态码 → `ProbeStatus::Unknown`,不拖垮整体并发(现有 `run_all_with_native_check` 已并发 + 单探针隔离)。
- 仿浏览器 `User-Agent` 及特定头(MetaAI/Instagram/SonyLiv)在各探针内设置。
- 限流(429)→ status `Unknown` + info "Rate Limited",不重试轰炸。
- 探针数量增多,默认报告解锁表会变长——`--probe` 已是显式开关(非默认报告),可接受。

## 测试

- 每探针:`classify_*`/解析纯函数单测 + httpmock 端到端(仿现有 `ai.rs::chatgpt_check_blocked` 模式),覆盖 Yes/No/Unknown/region/info 分支。
- `unlock_util.rs`:三→二码、cookie 提取、正则提取纯函数单测。
- CDN:样本 JSON → region/cdnInfo 解析单测。
- 渲染:`--raw`/默认/JSON 新字段快照断言;解锁表「备注」列断言。
- 全量保持 `cargo test` 绿;现有 234 测试不回归。

## 诚实标注(README)

- 解锁探测为**公开端点启发式抓取**,结果随各服务页面/接口变动可能失效,仅供参考(与 UnlockTests 同源,同样声明)。
- CDN 定位反映探测时落地的 PoP 节点,非固定。
- 无 region 的服务留空;限流/失败标 Unknown,不伪造。

## 改动文件

| 文件 | 改动 |
|---|---|
| `src/probe/mod.rs` | `ProbeResult` 加 `info`;`all_probes()` 注册 19 新探针;渲染表加备注列 |
| `src/probe/ai.rs` | 加 Claude / Gemini / MetaAI |
| `src/probe/web.rs` | 【新】Bing / GoogleSearch / GooglePlay / Reddit / Wikipedia / OneTrust / Apple / Steam / InstagramMusic |
| `src/probe/streaming.rs` | 加 IQiYi / KOCOWA / Viu / SonyLiv / TikTok |
| `src/probe/cdn.rs` | 【新】Netflix CDN / YouTube CDN |
| `src/probe/unlock_util.rs` | 【新】region 转码 / cookie / 正则纯函数 |
| `src/aggregate.rs` | (item 1)merge 补合并未覆盖字段 |
| `src/render/raw.rs` | (item 1)逐源详表加缺失字段 |
| `src/render/terminal.rs` | (item 1)默认报告补高信号合并字段 |
| `src/render/json.rs` | (item 1)JSON 暴露缺失字段 |
| `README.md` / `CHANGELOG.md` | 文档 + 版本 v0.19.0 |

## 范围边界(YAGNI)

- **不接 Sora**(已停服)。
- **不爬登录/付费内容**,不做反爬绕过、不逆向私有 API;仅复刻 UnlockTests 公开端点。
- **不新增 item 1 的 model 字段**(阶段二已建齐),只补渲染 + merge。
- **不拆分** `--probe` 为多子命令(全部并进现有解锁表)。
- 复杂多步探针不可得即降级标注,不凑数。

## 分阶段交付(交给实现计划细化)

- **阶段 A**:item 1 全字段渲染补全(纯本地,零网络,先落地验证)。
- **阶段 B**:简单探针(AI 3 + web 简单类 + 亚洲媒体 + TikTok)+ `ProbeResult.info` + unlock_util。
- **阶段 C**:复杂多步(MetaAI / SonyLiv / InstagramMusic / GooglePlay)+ CDN 定位(Netflix / YouTube)。
