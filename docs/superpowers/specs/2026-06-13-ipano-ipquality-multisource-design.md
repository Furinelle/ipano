# 设计:IP 质量检测多源扩充(对标 securityCheck)

- **日期**:2026-06-13
- **状态**:设计已确认,待写实现计划
- **目标版本**:v0.17.0(分两阶段:v0.17.0 免key源 / v0.18.0 keyed源)
- **对标**:[oneclickvirt/securityCheck](https://github.com/oneclickvirt/securityCheck)(聚合 ~19 个 IP 数据库,逐字段标注来源编号)

## 背景与问题

securityCheck 的「信息全」来自**聚合 ~19 个数据库 + 逐字段标注来源**。ipano 当前仅 8 源(ping0 / ip-api / ipinfo / ip.sb / ip.net.coffee / ippure / abuseipdb / ipqs),且缺少这些独特字段:使用/公司类型、ASN/公司滥用分、真人/机器人流量占比、浏览器/设备/OS 类型、黑名单记录统计、~300 项 DNSBL。

ipano 架构早为此设计:`trait Source` + `all_sources()` 注册 + `aggregate::merge` 合并。**加源 ≈ 加一个实现 `Source` 的文件**(P1–P4 即如此堆叠)。本设计补齐源与字段,使 ipano 的默认报告达到 securityCheck 级别的完整度,同时保留 ipano「干净合并」的可读性优势。

## 决策记录

| 议题 | 选择 | 理由 / 否决项 |
|---|---|---|
| 范围 | **全 19 源,含 keyed** | 用户明确要求全量对标;否决「只补几个亮点字段」。 |
| 推进 | **分两阶段**(本轮只出 spec+plan) | 免key源可本机验证;keyed源需用户 key。否决「一次性硬干到底」(keyed 无法验证、成本失控)。 |
| 触发 | **新源并进默认报告**(非独立 flag) | 用户选定;默认报告即更全。并发 fan-out + 超时兜底,延迟由最慢源 + timeout 封顶,加源不线性增延迟。无 key 的源瞬时跳过。 |
| 输出 | **默认合并 + `--raw` 逐源** | 默认保 ipano 干净风格(分数取多源中位/多数 + 标注参与源 + 分歧提示);`--raw` 出 securityCheck 同款逐字段逐源详表。 |
| 源编号 | **ipano 源名缩写**(如 `[ipqs]` `[vt]` `[cf]`) | 用户选定;比 securityCheck 的 `[0]-[I]` 直观,免查图例。否决数字/字母编号。 |
| DNSBL | **扩到 ~300**(取自 multirbl.valli.org 列表) | 用户选定;保留在 `--dnsbl` 标志下(300 次 DNS 查询太重,不进默认报告),并发 + 超时封顶。 |
| key 配置 | **沿用 env 约定,无 key 自动跳过并标注** | 与现有 `IPANO_ABUSEIPDB_KEY`/`IPANO_IPQS_KEY` 一致;绝不伪造数据。 |

## 架构

```
sources/<name>.rs   每个源实现 trait Source(fetch → SourceData)
sources/mod.rs      all_sources() 注册(新增源加 Box::new)
aggregate.rs        merge():多源 → MergedReport(扩充字段 + 合并规则)
model.rs            SourceData / MergedReport 扩充字段
render/terminal.rs  默认合并渲染(新增「IP 质量」字段 + 分歧标注)
render/raw.rs       【新】--raw 逐字段逐源 [缩写] 详表
render/json.rs      JSON 扩充字段
cli.rs              新增 --raw 标志
probe/dnsbl.rs      DNSBL 列表 12 → ~300
```

新源不改变核心流:`run_all` 并发拉所有源 → 每源产出 `SourceData` → `merge` 合一 → 渲染。

### 1. 源清单(19 源;沿用 ipano 源名缩写;✅ 已接)

**已有(4 与 securityCheck 重合 + 4 额外)**:ipinfo✅ · ip-api✅ · abuseipdb✅ · ipqs✅ · ip.sb✅ · ip.net.coffee✅ · ippure✅ · ping0✅

**阶段一 — 免 key 源(本会话实现+验证)**:

| 缩写 | 源 | 端点(实现期 curl 核实) | 提供字段 |
|---|---|---|---|
| `ipwhois` | ipwhois.io | `http://ipwho.is/<ip>` | 国家/ASN/isp/类型(is proxy/hosting) |
| `dbip` | db-ip.com | `https://api.db-ip.com/v2/free/<ip>` | 国家/城市/ASN |
| `bdc` | bigdatacloud | `https://api.bigdatacloud.net/data/ip-geolocation-full?ip=<ip>` 免key tier | 地理/是否数据中心/危险性 |
| `ipapiis` | ipapi.is | `https://api.ipapi.is/?q=<ip>` 免key限额 | **ASN/公司滥用分**、是否数据中心/代理/VPN/滥用 |
| `ipapicom` | ipapi.com | `https://ipapi.co/<ip>/json/` 免key限额 | 地理/ASN/org |
| `ip2loc` | ip2location.io | `https://api.ip2location.io/?ip=<ip>` 免key限额 | 使用类型/代理/类型 |

**阶段二 — keyed 源(需用户 key)**:

| 缩写 | 源 | key env | 提供字段 |
|---|---|---|---|
| `scam` | scamalytics | `IPANO_SCAMALYTICS_KEY`(+user) | 欺诈分/代理判定 |
| `ipreg` | ipregistry | `IPANO_IPREGISTRY_KEY` | 公司类型/是否云/中继/匿名/滥用 |
| `ipdata` | ipdata.co | `IPANO_IPDATA_KEY` | **信任/VPN/代理/威胁得分**、blocklist |
| `vt` | virustotal | `IPANO_VIRUSTOTAL_KEY` | **黑名单记录统计**(无害/恶意/可疑/无记录) |
| `cf` | cloudflare radar | `IPANO_CF_TOKEN` | **真人/机器人流量占比、浏览器/设备/OS 类型** |
| `ipintel` | getipintel.net | `IPANO_IPINTEL_EMAIL`(email 为必填参数) | 代理/VPN 概率 |
| `ipfighter` | ipfighter | `IPANO_IPFIGHTER_KEY`(端点实现期核实) | 欺诈补充 |
| `fraudlogix` | fraudlogix | `IPANO_FRAUDLOGIX_KEY` | 欺诈/威胁 |
| `dkly` | dkly(端点实现期核实) | 视情况 | 代理/风险补充 |

> 阶段二中 `ipfighter`/`fraudlogix`/`dkly` 端点与鉴权在实现计划阶段逐个 curl 核实;不可用或无公开 API 者降级为「不接入并在 spec 注明」,不强行爬网页(ToS 风险)。

### 2. 新增 model 字段(`SourceData` + `MergedReport` + JSON)

- `usage_type: Option<String>`(Commercial/hosting/business/ISP…)
- `company_type: Option<String>`(isp/hosting/business)
- `asn_abuse_score: Option<f64>` / `company_abuse_score: Option<f64>`(ipapi.is)
- `threat_level: Option<String>`(low/medium/high)
- `human_traffic_pct: Option<f64>` / `bot_traffic_pct: Option<f64>`(cloudflare)
- `browser_dist` / `device_dist` / `os_dist: Option<String>`(cloudflare,如「主流78% 其他21%」)
- `is_datacenter` / `is_cloud` / `is_relay` / `is_anonymous` / `is_bogon: Option<bool>`(部分已有,补全)
- `blacklist_harmless/malicious/suspicious/undetected: Option<u32>`(virustotal)

每字段在 `SourceData` 里按源各存一份(供 `--raw` 逐源展示),`merge` 后在 `MergedReport` 存合并值。

### 3. 合并规则(默认报告)

- **数值分**(信任/欺诈/滥用/威胁):取所有有值源的**中位数**,旁标参与源缩写 + 若极差 > 阈值给「分歧」提示。
- **布尔判定**(代理/VPN/Tor/数据中心…):**多数决**;少数派源列出(如「No(11/12 源)· 仅 ipqs 报 Yes」)。
- **分类字段**(使用/公司类型):取**众数**,并列其余取值。
- 沿用现有源优先级(ip.net.coffee/ipqs 等高信号源在并列时优先)。

### 4. 输出

- **默认**(terminal/markdown):现有报告 + 「IP 质量」补充字段(滥用分/流量占比/使用类型/黑名单统计),分歧用 ⚠ 标注。沿用 comfy-table。
- **`--raw`**(新):securityCheck 同款逐字段逐源详表,每行 `字段: 值 [源缩写] 值 [源缩写]…`。顶部不需数字图例(缩写自解释)。
- **JSON**:`sources[]` 每源原始字段 + 顶层合并字段;新增上述字段。

### 5. CLI

新增 `--raw`(布尔):启用逐源详细输出(默认关,出合并版)。`--seccheck` **不引入**(新源并进默认)。

### 6. DNSBL 扩展(`--dnsbl`,12 → ~300)

`probe/dnsbl.rs` 的列表从 12 条扩为 ~300 条(取自 multirbl.valli.org 公布的 DNSBL 全集,内置为静态表)。并发查询 + 单条 4s 超时 + 全局上限(如 8s)。输出汇总「Total/Clean/Blacklisted/Other」+ 命中列表。仅 IPv4,留在 `--dnsbl` 标志下(不进默认报告)。

### 7. 错误处理与降级

- 无 key 的 keyed 源 → 跳过 + 报告标注「needs key / skipped」(与现有 AbuseIPDB/IPQS 一致,绝不伪造)。
- 源超时/失败 → 该源标失败,不拖垮整体(现有 `run_all` 已具备)。
- 免key源限额触发(429)→ 当次标降级,不重试轰炸。

### 8. 测试

- 每源:解析样本 JSON → SourceData 的纯函数单测(参照现有 sources 测试模式,httpmock)。
- merge:多源中位/多数/众数合并规则单测(含分歧场景)。
- `--raw` 渲染:含某字段多源标注的快照断言。
- dnsbl:列表非空、去重、`is_listed_addr` 校验保留。
- JSON:新字段形状。

## 诚实标注(README)

- 多源聚合,**各源可能打架**;默认报告给合并判定,`--raw` 看原始分歧。
- keyed 源未配置则跳过标注,不伪造。
- cloudflare 流量占比/设备分布为 **Radar 聚合数据**(按 ASN/地区),非该 IP 精确画像,仅供参考。
- 数据仅供参考,不代表 100% 准确(securityCheck 自己也这么声明)。

## 改动文件

| 文件 | 改动 |
|---|---|
| `src/sources/*.rs` | 阶段一新增 6 个免key源文件;阶段二新增至多 9 个 keyed 源文件 |
| `src/sources/mod.rs` | `all_sources()` 注册新源 |
| `src/model.rs` | `SourceData`/`MergedReport` 新增字段 |
| `src/aggregate.rs` | merge 合并新字段 + 中位/多数/众数规则 |
| `src/render/terminal.rs` | 默认报告补充字段 + 分歧标注 |
| `src/render/raw.rs` | 【新】`--raw` 逐源详表 |
| `src/render/json.rs` | JSON 新字段 |
| `src/cli.rs` | `--raw` 标志 |
| `src/probe/dnsbl.rs` | DNSBL 12 → ~300 |
| `README.md` / `CHANGELOG.md` | 文档 + 版本 |

## 范围边界(YAGNI)

- **不爬网页**:无公开 API 的源(部分 scamalytics/ipintel 变体)宁可不接,不做 HTML 抓取(易封 + ToS 风险)。
- **不做** key 落盘管理 UI;仅 env 变量。
- **不做** cloudflare 之外的「设备/OS 画像」臆造(只有 Radar 提供,无则留空)。
- **不引入** `--seccheck` 独立子命令(新源并进默认)。
- 阶段二 keyed 源**逐个看公开 API 可得性**,不可得者明确放弃并在文档说明,不凑数。

## 分阶段交付

- **阶段一(v0.17.0,本会话可实现+VPS验证)**:6 免key源 + 全部新字段 + merge 规则 + `--raw` 渲染骨架 + DNSBL ~300。
- **阶段二(v0.18.0,需用户 key)**:keyed 源逐个接入,复用阶段一的字段与渲染。
