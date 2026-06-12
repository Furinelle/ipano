# 更新日志

本项目的所有重要变更都记录在此文件。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/),版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.12.0] - 2026-06-13

P11:流媒体解锁大扩(18 服务 + Region + Native/DNS 区分)。

### 新增

- **18 流媒体服务**:`--probe` 从 Netflix/YouTube Premium/ChatGPT 三项扩为完整矩阵:Netflix · YouTube Premium · Disney+ · HBO Max · Hulu · Prime Video · Bilibili CN · Bilibili HK/TW · AbemaTV · DAZN · BBC iPlayer · Crunchyroll · Paramount+ · Peacock · Discovery+ · Spotify · TVB Anywhere+ · Funimation · ChatGPT(共 19 项)
- **Region 地区列**:有 JSON API 的服务(AbemaTV/Bilibili/DAZN/Netflix/YouTube Premium 等)自动提取并展示两字母 ISO 国家码
- **Native/DNS 类型列**:探针机所在地区(ip.sb geoip 探测)与内容地区对比——一致为「原生/Native」,不一致为「DNS 解锁」,无地区信息为「—」
- **comfy-table 终端渲染**:终端输出改用与 `--route` 一致的 UTF8_FULL 包边表(4 列:服务/状态/地区/类型);`--markdown` 仍输出 pipe 表

### 变更

- `ProbeResult` 新增 `unlock_type` 字段(JSON 输出同步含此字段)
- 终端渲染路径:原 `render_section`(Markdown) 分裂为 `render_terminal`(comfy-table)+ `render_section`(Markdown)
- `Probe` trait 简化:移除未使用的 `hostname()` 默认方法

[0.12.0]: https://github.com/Furinelle/ipano/releases/tag/v0.12.0

## [0.11.0] - 2026-06-13

P10:三网回程深化(多城市 + 骨干补全 + CN2 细分 + 单 socket 提速)。

### 新增

- **三网 × 四城 = 12 目标**:`--route` 从北京三网扩为 电信/联通/移动 × 北京/上海/广州/成都;参考 IP 取自社区 backtrace 工具(zhanghanyun/backtrace)事实标准集;JSON `route[]` 新增 `city` 字段
- **CN2 GIA/GT 细分**:对电信 CN2(AS4809)按路径里的 `59.43` / `202.97` 段启发式细分——含 `59.43` 且不绕 `202.97`(163 骨干)判 **GIA**(精品),绕 163 判 **GT**,无 `59.43` 维持通用 CN2
- **骨干 ASN 表补全**:联通补 AS4847(CUII 族)/AS4808/AS17623(169);移动补 AS56048/AS134774(CMNET)、AS58807(**CMIN2 精品**,纠正此前误标联通的 bug);新增 `59.43→4809`、`202.97→4134`、`218.105/210.51→9929`、`219.158→4837`、`223.120.16-19→58807`、`223.118-121→58453` 的前缀兜底(ip-api 无 AS 号时)

### 变更

- **单 socket 并行引擎**:`probe::route::engine` 从「每目标一 socket + 串行」改为「单 ICMP socket + 每目标独立 seq 段(base=idx×64)」;12 条 trace 探测包一次性全发、回包按 seq 段归位,总耗时从约 12×window 压到约 1 个 window。延续 P9「无跨 socket 串扰」结论(多 socket 会被内核广播 Time Exceeded 串扰)
- 移动 CMI(AS58453)质量档由「优质」修正为「普通」(精品移动线为 CMIN2)

[0.11.0]: https://github.com/Furinelle/ipano/releases/tag/v0.11.0

## [0.10.0] - 2026-06-13

P12:邮件端口全面化。

### 新增

- **6 协议矩阵**:`--mail` 从 SMTP 3 端口 × 5 家 扩为 **SMTP/SMTPS/POP3/POP3S/IMAP/IMAPS × 15 家**(Gmail/Outlook/Office365/Yahoo/Apple/QQ/163/Sina/Sohu/Yandex/Zoho/GMX/MailRU/AOL/FastMail);各邮局按 smtp/pop/imap 主机分别探测,不提供某协议者对应格标 `—`
- **comfy-table 包边矩阵**:`--mail` 终端输出改用与主报告/`--route` 一致的包边表;`--markdown` 仍输出 pipe 表
- JSON `mail[]` 结构调整为 `{provider, protocols:[{proto, port, open}]}`

[0.10.0]: https://github.com/Furinelle/ipano/releases/tag/v0.10.0

## [0.9.1] - 2026-06-12

P9 增强:国际入境线识别 + 终端表格化。

### 新增

- **国际入境线识别(`入境线`列)**:不限运营商,识别全路径里优先级最高的骨干,揭示三网各经哪家入境(常见如三网均经联通 CUG / AS10099 入境);JSON `route[]` 新增 `entry` 字段
- **终端 comfy-table 包边表**:`--route` 终端输出改用与主报告一致的包边表(概览表 + 逐跳表),`--markdown` 仍输出 pipe 表
- 降级时概览表后给出 `sudo` / `setcap` 重试提示;README 顶部加 Release/Build 徽章

[0.9.1]: https://github.com/Furinelle/ipano/releases/tag/v0.9.1

## [0.9.0] - 2026-06-12

P9:三网回程路由(原生 traceroute)。

### 新增

- **原生 ICMP traceroute 引擎**:`probe::route` 用 libc raw/dgram socket 自行构造 ICMP Echo、按 TTL 递增逐跳探测,解析 Time Exceeded/Echo Reply 并按 seq 归位每跳;无第三方 traceroute 依赖
- **三网参考节点**:对 电信(北京 219.141.136.12)/联通(202.106.50.1)/移动(211.136.25.153)三网骨干节点各发一条 trace,从本机出口发起、与查询 IP 无关、只跑一次
- **逐跳 AS/geo 标注**:一次 ip-api `/batch` 请求标注路径上所有公网跳的 ASN/组织/国家/城市(跳过私网/CGNAT/198.18 基准段)
- **回程线路启发式识别**:按骨干 ASN 表(CN2 AS4809、163 AS4134、169 AS4837、9929/CUII AS9929、CUG AS10099、CMI AS58453、CMNET AS9808)识别各运营商回程线路类型与质量档(优质/普通)
- **`--route` 开关**:贯通终端、Markdown、JSON(`route` 数组,含逐跳明细与线路判定);末尾连续无应答跳自动截断,避免一长串 `*`
- **优雅降级**:优先免特权 ICMP DGRAM socket(macOS 即可、Linux 受 `ping_group_range` 许可时可),失败回退 raw socket(需 root/`cap_net_raw`),两者皆不可用时该条降级标注「需 root 运行」,不阻塞其余功能
- Cargo:新增 `libc` 依赖(raw/dgram socket 系统调用)

### 修复

- **三网 trace 串扰**:并发跑三条 traceroute 时,内核把 ICMP Time Exceeded 广播到多个 ICMP socket,各 trace 按相同 seq 互相抢收,导致三条路径混成一样(VPS 实测发现)。改为**串行**执行,且每条 trace 用**独立 seq 段**(base=i·64)隔离,只接受落在本段内的回包,杜绝跨 trace 与残留在途包混入

### 说明

- **仅 IPv4**:P9 暂只做 IPv4 traceroute(ICMPv6 后续);线路识别结果均为启发式,仅供参考
- CN2 GIA/GT 的细分需进一步看 59.43 节点,当前统一标 CN2,后续可细化
- socket I/O 无法 mock 单测,纯逻辑(报文构造/解析/线路识别/渲染/公网过滤)以 13 个单元测试覆盖,真发包靠集成运行验证
- 默认关闭,需显式 `--route`(主动外发 ICMP + 需特权)
- 文档:README 架构图补全 `probe/` 探测层(streaming/ai/mail/route)并新增 `--route` 输出示例

[0.9.0]: https://github.com/Furinelle/ipano/releases/tag/v0.9.0

## [0.8.0] - 2026-06-12

P8:ping0 token 手动复用。

### 新增

- **`--ping0-token` CLI 选项**:在浏览器解开 ping0 的 Cloudflare Turnstile 验证码后,把 `token` cookie 值传入即可复用(60 秒内有效),优先级高于环境变量 `IPANO_PING0_TOKEN`;`all_sources` 接受该 token 并注入 ping0 源
- 未提供或失效时 ping0 源自动降级(NeedsKey/ChallengeFailed),不影响其它源

### 说明

- **不实现自动绕验证码**:headless 浏览器自动通过 Turnstile 属绕过 bot 检测,不在本工具范围。仅支持"人工解验证码 + 复用其产出的 token"这一合法路径
- token 仅作运行期凭证,不落盘

[0.8.0]: https://github.com/Furinelle/ipano/releases/tag/v0.8.0

## [0.7.0] - 2026-06-12

P7:邮局连通性检测。

### 新增

- **邮局连通性探测**:`probe::mail` 用 tokio TCP 并发探测 Gmail/Outlook/QQ/Yahoo/Apple 的 SMTP 25/465/587 端口连通性,超时即视为不通
- **`--mail` 开关**:从本机出口发起、与查询 IP 无关、只跑一次;贯通终端、Markdown、JSON(`mail` 数组)
- Cargo:tokio 启用 `net`+`time` feature(TcpStream/timeout)

### 说明

- VPS 出站 25 端口常被服务商封锁,本检测可一眼看出哪些邮局哪些端口可达
- 默认关闭,需显式 `--mail`(主动外发 TCP 连接 + 增加延迟)

[0.7.0]: https://github.com/Furinelle/ipano/releases/tag/v0.7.0

## [0.6.0] - 2026-06-12

P6:流媒体 + AI 解锁检测。

### 新增

- **解锁探测框架**:`Probe` trait + 并发 `run_all` + `ProbeResult{name,status,region}`,`ProbeStatus` 四态(Unlocked/Restricted/Blocked/Unknown)
- **Netflix**:请求非自制剧标题页,200=完全解锁 / 404=仅自制剧 / 403=封锁
- **YouTube Premium**:解析 `/premium` 页 countryCode 与可用性
- **ChatGPT**:请求 OpenAI 合规端点,200=可用 / 403=受限地区封锁
- **`--probe` 开关**:解锁检测从本机出口发起、与查询 IP 无关、只跑一次;贯通终端、Markdown、JSON(`probes` 数组)
- 探测失败统一降级为 Unknown,不伪造

### 说明

- 解锁判定依赖第三方端点行为,可能随其改版漂移;分类逻辑独立成纯函数并单测,运行期失败即降级
- 解锁检测默认关闭(主动外发请求 + 增加延迟),需显式 `--probe`

[0.6.0]: https://github.com/Furinelle/ipano/releases/tag/v0.6.0

## [0.5.0] - 2026-06-12

P5:横向对比表 + 启发式结论 + Markdown 导出 + 中英 i18n。

### 新增

- **各源横向对比**:`MergedReport` 保留各成功源原始数据(`raw`),并排呈现每源的代理/VPN/Tor/类型/风险分判定
- **启发式结论**:`heuristics::conclude` 综合代理/VPN/Tor、IP 类型、纯净度、欺诈/滥用分给出风险结论(双语)
- **Markdown 导出**:`--markdown` 输出含基础信息、对比表、结论、源状态的可粘贴报告
- **i18n**:`--lang zh|en`(默认 zh),贯通终端结论区、对比表、Markdown
- 终端报告新增"启发式结论"区

### 说明

- 启发式阈值:IPQS/ippure 欺诈分 ≥75、AbuseIPDB 置信度 ≥50、ping0 风控值 ≥75 视为高风险;纯净度 <40 视为偏低
- 基础信息表标签暂保持中文;i18n 当前覆盖结论、对比表与 Markdown 输出

[0.5.0]: https://github.com/Furinelle/ipano/releases/tag/v0.5.0

## [0.4.0] - 2026-06-12

P4:西方欺诈库(key 可选)。

### 新增

- **AbuseIPDB 源**:接入 `/api/v2/check`(env `IPANO_ABUSEIPDB_KEY`),提供滥用置信度 `abuseipdb_score` 与 totalReports→is_abuser;经 header `Key` 鉴权,429 识别为限流
- **IPQS 源**:接入 `/api/json/ip/{key}/{ip}`(env `IPANO_IPQS_KEY`),提供欺诈分 `ipqs_score` 与 proxy/vpn/tor/crawler/mobile/recent_abuse 标记及 asn/geo;success=false 时降级
- **key 可选语义**:两源 `needs_key()` 标注所需环境变量,未配置 key 时返回 NeedsKey 自动跳过并标注,不阻塞其它源、不伪造数据
- 数据模型新增 `abuseipdb_score`/`ipqs_score` 字段,贯通终端风险区与 JSON

### 说明

- scamalytics 免 key 抓取返回 403、IP2Location 需 key,本阶段未接入;后续如有免 key 通道再补
- 各欺诈分按源独立保留(AbuseIPDB 置信度、IPQS 欺诈分、ippure 欺诈分),不强行折算

[0.4.0]: https://github.com/Furinelle/ipano/releases/tag/v0.4.0

## [0.3.0] - 2026-06-12

P3:ippure 欺诈源(egress 专用)。

### 新增

- **ippure 源**:接入 `my.ippure.com/v1/info`,提供 fraudScore(欺诈分)、isResidential、isBroadcast 及 asn/geo;`SourceData`/`MergedReport` 新增 `fraud_score` 字段,贯通终端"欺诈分"行与 JSON 输出
- **egress 守卫**:ippure API 只返回调用者出口 IP,无法查指定 IP。源在返回 ip 与查询 ip 不符时自动跳过(降级),仅在无参查本机模式贡献数据
- ip-api 的 proxy/hosting/mobile 字段(P1 已含)作为指定 IP 模式下的代理/机房交叉确认

### 说明

- 西方欺诈库 scamalytics 免 key 抓取返回 403,IPQS/AbuseIPDB 等需 key,统一推迟到后续 key 可选阶段,不在 P3 强接
- 欺诈分(ippure,越高越危险)与纯净度(net.coffee,越高越干净)、风控值(ping0,越高越危险)按源独立保留

[0.3.0]: https://github.com/Furinelle/ipano/releases/tag/v0.3.0

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

## [0.1.0] - 2026-06-12

地基 MVP(设计文档 P0–P1)。

### 新增

- **CLI 框架**:`ipano [IP]`,支持 `-4`/`-6`/`--json`/`--no-color`/`--timeout` 参数(基于 clap)
- **双查询模式**:无参数查本机出口 IP(IPv4/IPv6,多端点取众数),带参数查指定 IP
- **核心抽象**:`Source` trait + `run_all` 并发调度,单源失败不影响整体
- **数据源**:ip-api、ipinfo、ip.sb 三个免 key 基础源(每源拆分纯解析层 + 抓取层)
- **聚合**:`merge()` 按源优先级(ipinfo > ipsb > ipapi)去重合并基础字段,记录各源成功/失败状态
- **渲染**:彩色终端报告(comfy-table + owo-colors)与机器可读 JSON 双输出
- **测试**:19 个单元/集成测试(解析层纯函数 + httpmock 模拟抓取,不依赖真实网络)

### 说明

- 编译为单静态二进制(rustls,无系统 OpenSSL 依赖)
- `SourceData` 已预留 `is_vpn`/`is_tor`/`ip_type` 等字段,供后续 ping0 及欺诈库源填充

[0.1.0]: https://github.com/Furinelle/ipano/releases/tag/v0.1.0
