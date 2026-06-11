# ipano —— 一站式 IP 全景聚合检测工具 · 设计文档

- **日期**:2026-06-11
- **名称**:`ipano`(IP + Panorama,"IP 全景")
- **定位**:一站式聚合多源 IP 检测工具,一个 IP,全景体检
- **语言/形态**:Rust,编译为单个静态二进制(rustls,无系统级 OpenSSL 依赖)
- **状态**:设计已确认,待写实现计划

---

## 1. 目标与背景

聚合社区主流的 IP 信息/纯净度源,在一个命令里给出一份"全景体检报告":基础归属、IP 类型、风险评分、代理/VPN/Tor 标记、流媒体与 AI 服务解锁、邮局连通性。

参照系:
- [`spiritysdx/za` ecs.sh](https://gitlab.com/spiritysdx/za):综合 VPS 测评,IP 检测部分较浅(地理/三网回程/NAT),作为形态参考。
- [`xykt/IPQuality`](https://github.com/xykt/IPQuality)(即 `IP.Check.Place`):黄金标准,聚合 IPinfo/ipregistry/ipapi/AbuseIPDB/IP2Location/IPQS/DB-IP/Scamalytics,六大模块。本项目对标其完整度,并补齐其覆盖较弱的"风控值/原生 IP/纯净度"华人流派源(ping0 等)。

差异化:**ping0 的自研大数据风控值/原生 IP 判定是真正独家数据**,西方欺诈库无法提供;ippure / ip.net.coffee 等多为底层库的再封装,提供交叉确认与补充字段。

---

## 2. 核心决策

| 维度 | 决策 |
|---|---|
| 形态 | Rust,单静态二进制,rustls |
| 查询对象 | 无参 = 查本机出口 IP(v4 + v6);带参 = 查指定 IP |
| 数据源范围 | 全家桶:ping0 + ippure + ip.net.coffee + 西方欺诈库 + 基础地理/ASN |
| 聚合方式 | 混合式:基础信息去重合一 + 关键判定横向对比表 + 启发式结论 |
| 认证 | 抓取优先,API key 可选;无 key 的源自动跳过并标注 |
| 抓取架构 | 混合 + 优雅降级:HTTP 优先,ping0 试复刻 token,失败标记降级;可选 `--browser` 后端 |
| 功能范围 | **全功能**:基础/类型/风险/标记/流媒体解锁/AI 解锁/邮局连通性/**三网回程路由(原生 traceroute)** |
| 测线路 | 原生 Rust traceroute,每跳复用 IP 信息层标注 AS/geo + 回程线路类型识别;需特权,无特权自动降级 |

---

## 3. 能力边界(诚实声明)

CLI 跑在服务端,**以下属客户端浏览器行为,本工具拿不到**,报告中明确标注"CLI 不适用",不伪造:
- 浏览器指纹
- WebRTC 泄露检测
- DNS 泄露检测(客户端解析层面)

可获取的是各源的**服务端 IP 信息**(地理 / ASN / 风险 / 纯净度 / 类型 / 标记)以及**主动探测类**结果(流媒体/AI/邮局连通性/三网回程路由,由本机实际发起请求测得)。

**特权要求**:三网回程路由(traceroute)需要 raw socket,Linux 上需 root 或 `cap_net_raw`。**无特权时该模块自动降级**(跳过并提示"需 root 运行"),不影响其余功能正常输出。

---

## 4. 架构与模块边界

```
main → cli(clap) → orchestrator
   ├─ egress.rs        本机出口 IP 探测(v4/v6,多端点取众数,去抖)
   ├─ fetch.rs         共享 HTTP 客户端:cookie store / 重试退避 / 超时 / UA / gzip
   ├─ browser.rs       可选 headless 后端(feature = "browser",默认不编译)
   ├─ sources/         每源一个文件,实现 Source trait(并发抓取)
   │     ping0 · ippure · netcoffee · ipinfo · ipapi · ipsb
   │     scamalytics · ipqs · abuseipdb · ip2location · dbip
   ├─ challenge/ping0  ping0 token 复刻 + 降级
   ├─ probe/           主动探测模块
   │     streaming(Netflix/Disney+/YouTube/TikTok/PrimeVideo/Spotify)
   │     ai(ChatGPT/Claude/Gemini 等区域可用性)
   │     mail(Gmail/Outlook/Yahoo/Apple/QQ/Mail.ru 25/465/587 连通)
   │     route(原生 traceroute 引擎 + 三网节点表 + 回程线路类型识别)
   │       └ 每跳复用 sources 的 IP 信息层做 AS/geo 标注
   ├─ aggregate.rs     合并去重 + 对比表 + 启发式结论
   ├─ config.rs        ~/.config/ipano/config.toml + 环境变量
   └─ render/          terminal(彩色表) · json · markdown
```

**设计原则**:每个源 / 探测器是独立单元,只通过 trait 暴露"输入 IP → 结构化结果或失败原因",新增 = 加一个文件,不动其它代码。单元可独立测试(mock HTTP 响应)。

---

## 5. 核心抽象

### 5.1 `Source` trait
```rust
#[async_trait]
trait Source {
    fn id(&self) -> &'static str;          // "ping0"
    fn needs_key(&self) -> Option<&str>;   // Some("IPQS") → 需 key 才启用
    async fn fetch(&self, ip: IpAddr, ctx: &Ctx) -> SourceResult;
}

type SourceResult = Result<SourceData, SourceError>;

enum SourceError {
    Unavailable,      // 站点不可达/解析失败
    RateLimited,      // 触发限流
    NeedsKey,         // 未配置必需的 key
    ChallengeFailed,  // 反爬挑战(如 ping0 token)未通过
    Timeout,
    Parse(String),
}
```

### 5.2 统一数据模型(canonical)
所有源输出归一,缺失为 `None`:
- **基础**:`ip` / `version` / `asn` / `as_org` / `isp` / `org` / 国家·地区·城市 / 经纬度 / 时区 / `rdns`
- **类型**:`ip_type`(原生 native / 广播 broadcast / 机房 IDC / 家宽 residential / 移动 mobile / 商业 business) + ping0 `is_native`
- **风险分**(各源独立保留,不强行折算成单一数字):`ping0_risk`(风控值)、`ping0_purity`(纯净度)、`scamalytics_score`、`ipqs_score`、`abuseipdb_confidence`
- **标记**:`is_proxy` / `is_vpn` / `is_tor` / `is_hosting` / `is_relay` / `is_bogon`

`SourceData` 携带 `source_id` 与原始字段映射,便于 `--json` 输出原始数据。

---

## 6. 数据源清单(全家桶)+ 每源策略

| 源 | 独家/价值 | 获取方式 | Key | 备注 |
|---|---|---|---|---|
| **ping0.cc** | 风控值/纯净度/原生 IP(独家) | HTTP + token 复刻,失败降级 | 否 | 皇冠明珠,最难最高价值 |
| **ippure.com** | 纯净度/风险再确认 | HTTP 抓服务端字段 | 否 | 渲染重,仅取服务端可得字段 |
| **ip.net.coffee** | 地理/连通/分流/评分 | HTTP 抓服务端字段 | 否 | HTML 解析 |
| ipinfo.io | 基础地理/ASN(高准) | HTTP `/json` | 可选 | 免 key 有限额,有 token 更稳 |
| ip-api.com | 基础地理/ASN/proxy | HTTP `/json` | 否 | 免费源,基线 |
| ip.sb | 出口 IP / 基础 | HTTP | 否 | egress 探测候选端点 |
| scamalytics | 欺诈分 | 抓 web 结果(免 key) | 可选 | 优先抓页面 |
| IPQS | 欺诈分/proxy/VPN | API | 需 key | 无 key 跳过并标注 |
| AbuseIPDB | 滥用置信度 | API | 需 key | 同上 |
| IP2Location | 类型/proxy | API | 需 key | 同上 |
| DB-IP | 地理/类型 | API/抓取 | 可选 | 同上 |

> 实现时每个源的真实端点/字段需在对应阶段逐一逆向核实并写入该源模块的文档注释。

---

## 7. 聚合与输出(混合式)

### 7.1 合并逻辑
- **基础信息去重合一**:每个 canonical 基础字段按"源优先级表"选取最可靠来源(例:地理 ipinfo > ip-api > ping0;ASN ipinfo > ping0)。报告标注该字段取自哪个源、几源一致。
- **关键判定横向对比**:类型/原生、风控/纯净度、proxy/vpn/tor 各源并列成表,分歧用 ⚠ 高亮。
- **启发式结论**:简单规则(最坏值 + 多数表决)生成一句话结论,**明确标注为启发式,非权威**。

### 7.2 终端输出样例
```
═══ IP 全景报告  1.2.3.4 (IPv4) ═══
基础   AS13335 Cloudflare · 美国·洛杉矶 · 34.05,-118.24 · rDNS one.one.one.one
       └ 地理取自 ipinfo / ASN 取自 ping0，3 源一致

关键判定对比
源            类型      风控/纯净度   proxy  vpn  tor
ping0         机房IDC   88 / 12       —      —    —      ← 风控偏高
ipinfo        hosting   —             ✓      —    —
scamalytics   —         25(low)       —      ✓    —      ⚠ 与 ping0 分歧
ipqs          —         71(high)      ✓      ✓    —
abuseipdb     —         0%            —      —    —

解锁     Netflix ✓自制+完整  Disney+ ✗  YouTube ✓(US)  ChatGPT ✓  Claude ✓
邮局     Gmail ✓  Outlook ✓  Yahoo ✗(25封)  QQ ✓
回程     电信 CN2 GIA(AS4809)  联通 169(AS4837)  移动 CMI(AS58453)  [需 root]
结论     机房 IP · 风控偏高 · 多源检出 VPN/代理 · 非原生
源状态   ✓ipinfo ✓ipapi ✓ping0 ✓scamalytics ✗ipqs(需key) ⊘ippure(降级)
```

### 7.3 输出模式
- 默认:彩色终端报告(中文)
- `--json`:机器可读,含**各源原始数据** + 合并视图
- `--markdown`:导出 md 报告

---

## 8. CLI 接口

```
ipano                       # 查本机 v4 + v6
ipano 1.2.3.4               # 查指定 IP
ipano -4 / -6               # 限定协议族
ipano --json                # 机器可读
ipano --markdown            # 导出 md
ipano --source ping0,ipinfo # 只跑指定源
ipano --no-stream --no-mail # 关闭主动探测模块(加速)
ipano --route               # 显式开启三网回程路由(需 root)
ipano --no-route            # 关闭回程路由
ipano --browser             # 本机有 Chrome 时启用浏览器后端
ipano --init-config         # 生成配置骨架
ipano --lang zh|en          # 语言(默认 zh)
--timeout <秒> / --no-color
```

---

## 9. 配置与密钥

- 配置文件:`~/.config/ipano/config.toml`
- 环境变量覆盖:`IPANO_IPQS_KEY` / `IPANO_ABUSEIPDB_KEY` / `IPANO_IPINFO_TOKEN` …
- `--init-config` 生成带注释的骨架
- 无 key 的需认证源 → 自动跳过 + 报告标注 `✗(需key)`

```toml
[keys]
ipinfo   = ""   # 可选,提高限额
ipqs     = ""   # 需注册免费 key
abuseipdb = ""
ip2location = ""

[options]
lang = "zh"
timeout = 8
```

---

## 10. 错误处理与降级

- **并发 + 部分成功**:所有启用源/探测器用 `tokio` 并发执行,单源超时/失败仅标注该源状态,**绝不拖垮整体**。
- 退出码:只要拿到基础信息即 `0`;完全失败(网络全断)非 0。
- 每个 `SourceError` 在报告"源状态"行有明确图标:`✓` 成功 / `✗(需key)` / `⊘(降级)` / `⏱(超时)` / `⚠(限流)`。
- ping0 token 复刻失败 → `ChallengeFailed` 降级,不崩溃;若启用 `--browser` 则回退到浏览器后端重试。

---

## 11. 并发与性能

- 全部源并发抓取,单源独立超时(默认 8s,可调)。
- 主动探测(流媒体/AI/邮局)默认开启,可用 `--no-stream` / `--no-mail` 关闭以加速。
- 目标:默认全量在 10s 量级内出结果(取决于最慢源与网络)。

---

## 12. 技术栈

`tokio` · `reqwest`(rustls + cookies + gzip)· `serde`/`serde_json` · `clap`(derive)· `comfy-table` · `owo-colors` · `anyhow`/`thiserror` · `toml` · `futures`。
回程路由:`socket2` / `pnet`(raw socket traceroute)。
可选:`chromiumoxide`(`browser` feature,默认不编译,保持零依赖单二进制)。

---

## 13. 增量交付路线(全功能)

| 阶段 | 内容 | 产出 |
|---|---|---|
| P0 | 骨架:CLI / 模型 / fetch / egress / render + ipinfo 单源跑通 | 能跑 |
| **P1** | 免 key 源:ip-api / ipinfo / ip.sb + 合并渲染 | **首个可用 MVP** |
| P2 | **ping0**:token 复刻 + 降级 | 拿到独家数据 |
| P3 | ippure + ip.net.coffee(仅服务端字段) | 三家到齐 |
| P4 | key 源:scamalytics(抓)/ipqs/abuseipdb/ip2location/dbip + 配置 | 全家桶聚合 |
| P5 | 对比表 + 启发式结论 + markdown + i18n | 聚合成品 |
| P6 | 主动探测:流媒体 + AI 解锁 | 解锁模块 |
| P7 | 主动探测:邮局连通性 | 邮局模块 |
| P8 | `--browser` 后端(feature) + ping0 浏览器回退 | 最稳形态 |
| P9 | **三网回程路由**:原生 traceroute 引擎 + 三网节点表 + 回程线路类型识别 + 每跳复用 IP 信息层标注 + 无特权降级 | 测线路 |

每阶段独立可交付、可测试;源/探测器互相隔离,可乱序补齐。

---

## 14. 风险与对策

| 风险 | 对策 |
|---|---|
| ping0 token 算法复杂/改版 | 复刻失败即降级标注;`--browser` 回退;算法逻辑集中在 `challenge/ping0` 便于维护 |
| ippure/ip.net.coffee 渲染重、字段抓取脆 | 只取稳定的服务端字段;失败降级不阻塞;明确标注其为"再确认"源 |
| 站点反爬升级/封 IP | UA 轮换 + 退避重试 + 超时;每源独立失败 |
| 西方库需 key | 抓取优先,key 可选,无 key 优雅跳过 |
| 主动探测被目标限流 | 可关闭;并发限速 |
| 回程路由需特权 | 无 root/`cap_net_raw` 时自动降级跳过并提示,不阻塞其余功能 |
| 回程线路类型识别不准 | 维护已知骨干 ASN 表(CN2 GIA/GT/163/CUVIP/CMI/AS9929/AS4837…),识别逻辑集中可迭代;识别结果标注"启发式" |

---

## 15. 暂不做 / 未来

- GUI / Web 前端(本项目是 CLI;未来可在 `--json` 之上另起前端)
- 客户端指纹/WebRTC/DNS 泄露(CLI 能力外,已声明)
- 历史记录/持久化数据库(v1 无状态)

---

## 16. 项目位置

`/Users/furina/Documents/Github/ipano/`,独立 git 仓库。
