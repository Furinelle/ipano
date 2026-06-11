# 更新日志

本项目的所有重要变更都记录在此文件。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/),版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

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
