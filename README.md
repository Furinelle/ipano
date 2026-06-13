# ipano

> **IP + Panorama** —— 一站式 IP 全景聚合检测工具。一个 IP,全景体检。

[![Release](https://img.shields.io/github/v/release/Furinelle/ipano?logo=github&label=release)](https://github.com/Furinelle/ipano/releases/latest)
[![Build](https://github.com/Furinelle/ipano/actions/workflows/release.yml/badge.svg)](https://github.com/Furinelle/ipano/actions/workflows/release.yml)

`ipano` 是一个用 Rust 编写的命令行工具,把多个 IP 信息源聚合到一份报告里:基础归属、ASN、IP 类型、风险/纯净度、代理标记等。编译为**单个静态二进制**,在 VPS 上下载即用,零运行时依赖。

## 当前状态

**v0.16.0 — 三网回国 + 国际测速重做**。`--speedtest` 不再下载国际 CDN，改为对 **speedtest.net 三网(电信/联通/移动)+ 香港 + 教育/广电 + 国际(美/日/新)** 服务器测 **延迟 + 下载 + 上传**(host 运行时按 server id 解析，适配各 vantage）。可选 `--speedtest=cn/ct/cu/cm/hk/edu/intl/us/jp/sg/all`、server id 列表或 `=list` 看全部可选节点，逗号可组合；不带值=默认 6 代表(港·沪联通·京联通·苏电信·浙电信·沪移动)。单连接单流，结果仅供参考。⚠️ 破坏性:旧 `[[speedtest]] {name,url}` 配置改为 `[speedtest] spec` + `[[speedtest.custom]]`。v0.14.x(P14)：`-A`/`--all` 同时启用 --probe/--mail/--route/--dnsbl；配置文件 `~/.config/ipano/config.toml` 持久化语言/超时/常开模块/ping0 token，CLI 参数优先覆盖。`--dnsbl` 对当前查询 IPv4 并发检查 12 个主流邮件/滥用黑名单(Spamhaus ZEN / SpamCop / Barracuda / CBL / SORBS / UCEProtect / DroneBL 等),DNS 反向查询 4s 超时,结果 comfy-table 呈现。v0.12.0(P11)：`--probe` 从 3 项扩为 **19 项**(Netflix · YouTube Premium · Disney+ · HBO Max · Hulu · Prime Video · Bilibili CN · Bilibili HK/TW · AbemaTV · DAZN · BBC iPlayer · Crunchyroll · Paramount+ · Peacock · Discovery+ · Spotify · TVB Anywhere+ · Funimation · ChatGPT),新增 **Region** 地区列与 **Native/DNS** 类型列(探针机地区 vs 内容地区自动对比),终端输出改为 comfy-table 包边表。

## 功能(当前版本)

- **双查询模式**:无参数查本机出口 IP(IPv4/IPv6),带参数查任意指定 IP
- **多源并发聚合**:同时查询 [ip-api](https://ip-api.com)、[ipinfo](https://ipinfo.io)、[ip.sb](https://ip.sb),单源失败自动降级、不拖垮整体
- **混合式合并**:基础字段按源优先级去重合一,报告标注各源成功/失败状态
- **双输出**:彩色终端报告 + 机器可读 JSON
- **风险/纯净度**:接入 ip.net.coffee `iprisk` 接口,呈现纯净度、滥用评分、信誉威胁值、AI 判定及代理/VPN/Tor/机房等标记
- **欺诈分**:接入 [ippure](https://ippure.com) `fraudScore`(仅本机出口模式;查指定 IP 时该源自动跳过,因其 API 只返回调用者 IP)
- **西方欺诈库(可选 key)**:配置环境变量后启用 [AbuseIPDB](https://www.abuseipdb.com)(`IPANO_ABUSEIPDB_KEY`,滥用置信度)与 [IPQS](https://www.ipqualityscore.com)(`IPANO_IPQS_KEY`,欺诈分 + proxy/vpn/tor);未配置则自动跳过并标注,绝不伪造
- **横向对比 + 启发式结论**:各源关键判定(代理/VPN/Tor/类型/风险分)并排对比,叠加启发式风险结论
- **Markdown 导出 + 中英 i18n**:`--markdown` 输出可粘贴的报告,`--lang en` 切换英文
- **三网回国 + 国际测速(`--speedtest`)**:对 speedtest.net 三网(电信/联通/移动)+ 香港 + 教育/广电 + 国际(美/日/新)服务器测 **延迟 + 下载 + 上传**;`--speedtest=list` 看全部可选节点,`=cn/ct/cu/cm/hk/edu/intl/us/jp/sg/all` 或 server id 列表选择(逗号可组合),不带值=默认 6 代表;host 运行时按 server id 解析;速率/延迟着色;配置文件 `[speedtest] spec` 设默认、`[[speedtest.custom]]` 加自定义节点;单连接单流仅供参考,因耗流量较大不含在 --all 内
- **一键全跑(`-A` / `--all`)**:同时启用 --probe/--mail/--route/--dnsbl,VPS 上线后一条命令完成全景体检(测速因耗流量需单独 --speedtest)
- **配置文件(`~/.config/ipano/config.toml`)**:持久化 lang/timeout/no_color/ping0_token 及各模块常开(always.probe/mail/route/dnsbl);CLI 参数优先覆盖;文件不存在时静默跳过
- **DNSBL 黑名单检测(`--dnsbl`)**:并发查询 12 个主流 DNSBL(Spamhaus ZEN/SpamCop/Barracuda/CBL/SORBS/UCEProtect L1-L2/DroneBL/PSBL/0Spam/Backscatterer);DNS 反向查询(IPv4 反转追加 DNSBL 域名),4s 超时;comfy-table 展示命中数与每条状态;`--markdown` pipe 表;`--json` 含 `dnsbl[]` 字段;IPv6 返回空跳过
- **解锁检测(`--probe`)**:19 项并发探测(Netflix/Disney+/HBO Max/Hulu/Prime Video/Bilibili CN/HK·TW/AbemaTV/DAZN/BBC iPlayer/Crunchyroll/Paramount+/Peacock/Discovery+/Spotify/TVB Anywhere+/Funimation/YouTube Premium/ChatGPT);返回解锁状态、地区码(有 API 的服务)及 Native/DNS 类型(探针机地区 vs 内容地区对比);comfy-table 包边表呈现,`--markdown` 输出 pipe 表
- **邮件端口连通性(`--mail`)**:6 协议矩阵 SMTP/SMTPS/POP3/POP3S/IMAP/IMAPS × 15 家邮局(Gmail/Outlook/Office365/Yahoo/Apple/QQ/163/Sina/Sohu/Yandex/Zoho/GMX/MailRU/AOL/FastMail),包边表呈现(VPS 25 端口常被封,一眼可见)
- **三网回程路由(`--route`)**:原生 Rust ICMP traceroute 到 电信/联通/移动 × 北京/上海/广州/成都 12 个参考节点(单 socket 并行),每跳复用 ip-api 标注 AS/归属,按骨干 ASN(CN2 AS4809 / 163 AS4134 / 169 AS4837 / 9929 / CMIN2 AS58807 / CMI AS58453 / CMNET AS9808 等)启发式识别回程线路类型与质量档(优质/普通),并对电信 CN2 细分 GIA/GT;需 root/`cap_net_raw`,无特权自动降级

## 安装

### 预编译二进制(推荐,无需 Rust、不在本机编译)

静态 musl 二进制,任意 x86_64 / aarch64 Linux 下载即用:

```bash
# 一键脚本(自动识别架构,默认装到 /usr/local/bin)
curl -fsSL https://raw.githubusercontent.com/Furinelle/ipano/main/scripts/install.sh | sh

# 或手动下载对应架构
curl -fsSL https://github.com/Furinelle/ipano/releases/latest/download/ipano-x86_64-unknown-linux-musl.tar.gz | tar xz
./ipano 1.1.1.1
```

部署到一堆 VPS:**编一次、把单文件 `scp` 过去即可**,小内存机器无需本地编译——编译吃内存(`rustc`/LLVM 要 1GB+),但跑起来很轻(单文件、几十 MB 内存、秒启动)。

### 从源码编译

需要 [Rust 工具链](https://rustup.rs/):

```bash
git clone https://github.com/Furinelle/ipano.git && cd ipano
cargo build --release
# 二进制在 target/release/ipano

# 想自己产可分发的静态二进制:
rustup target add x86_64-unknown-linux-musl && sudo apt install -y musl-tools
cargo build --release --target x86_64-unknown-linux-musl
```

## 用法

```bash
ipano                  # 查本机出口 IP(v4 + v6)
ipano 1.1.1.1          # 查指定 IP
ipano -4               # 仅 IPv4
ipano -6               # 仅 IPv6
ipano --json 8.8.8.8   # 输出 JSON
ipano --markdown 1.1.1.1   # 输出 Markdown(含各源对比表 + 启发式结论)
ipano --lang en        # 英文输出(结论/对比/Markdown)
ipano --probe          # 解锁检测(19 服务,含 Region + Native/DNS 类型)
ipano --mail           # 邮件端口连通性(6 协议 × 15 家邮局矩阵)
ipano --ping0-token <TOKEN>   # 复用浏览器解出的 ping0 token(60 秒内有效)
ipano --route          # 三网回程路由(原生 traceroute,需 root/cap_net_raw)
ipano --dnsbl          # DNSBL 黑名单检测(12 个主流列表,仅 IPv4)
ipano -A               # 一键全跑(--probe + --mail + --route + --dnsbl)
ipano --speedtest      # 三网回国+国际测速(延迟+下载+上传,默认 6 代表,耗流量,不含在 --all)
ipano --speedtest=list # 列出全部可选测速节点(三网/港/教育/美日新)
ipano --speedtest=cn,jp # 选择三网全部 + 日本(分组/server id 逗号可组合)
ipano --no-color       # 关闭彩色
ipano --timeout 5      # 单源超时(秒,默认 8)
```

终端输出示例:

```
═══ IP 全景报告  1.1.1.1 ═══
┌────────┬──────────────────────────────────┐
│ 字段   ┆ 值                               │
╞════════╪══════════════════════════════════╡
│ ASN    ┆ AS13335 Cloudflare, Inc.         │
│ 归属   ┆ AU Queensland Brisbane           │
│ 经纬度 ┆ -27.46,153.02                    │
│ 时区   ┆ Australia/Brisbane               │
│ rDNS   ┆ one.one.one.one                  │
└────────┴──────────────────────────────────┘
源状态  ✓ipapi ✓ipinfo ✓ipsb
```

`--route` 三网回程路由示例(在有 root/`cap_net_raw` 的 VPS 上):

```
三网回程路由(traceroute)

| 运营商 | 目标节点                 | 入境线           | 回程线路              | 质量       | 跳数 |
|--------|--------------------------|------------------|-----------------------|------------|------|
| 电信   | 北京电信 219.141.140.10  | 电信 CN2 GIA     | 电信 CN2 GIA (AS4809) | [精品线路] | 12   |
| 电信   | 上海电信 202.96.209.133  | 电信 CN2 GT      | 电信 CN2 GT (AS4809)  | [优质线路] | 13   |
| 电信   | 成都电信 61.139.2.69     | 电信 163         | 电信 163 (AS4134)     | [普通线路] | 14   |
| 联通   | 北京联通 202.106.195.68  | 联通 9929/CUII   | 联通 9929/CUII (AS9929)| [优质线路] | 11  |
| 移动   | 北京移动 221.179.155.161 | 移动 CMIN2       | 移动 CMIN2 (AS58807)  | [精品线路] | 11   |
| 移动   | 成都移动 211.137.96.205  | 移动 CMI         | 移动 CMI (AS58453)    | [普通线路] | 13   |
```

(共 12 行,三网 × 四城,此处略示;逐跳明细含每跳 IP/RTT/AS/归属;无特权时整条降级标注「需 root 运行」,不影响其余检测。示例值仅为示意,实际随测点而变。)

## 能力边界

`ipano` 跑在服务端,**无法**获取以下客户端浏览器行为(它们需要真实浏览器):浏览器指纹、WebRTC 泄露、DNS 泄露检测。报告中这类项会明确标注"CLI 不适用",不伪造数据。

**关于 ping0.cc**:ping0 现已被 Cloudflare Turnstile 验证码全站接管,且其 token 60 秒过期,无法程序化抓取(强行绕过验证码不在本工具范围)。ipano 仅支持 **cookie 复用**:在浏览器中解开 ping0 验证码后,把 `token` cookie 值通过环境变量 `IPANO_PING0_TOKEN` 提供(60 秒内有效),ipano 会在该窗口内复用;未提供或已失效时,ping0 源自动标注降级,不影响其它源。

**关于三网回程路由(`--route`)**:原生 ICMP traceroute 需 raw/dgram socket。ipano 优先用免特权 ICMP DGRAM socket(macOS 即可、Linux 受 `net.ipv4.ping_group_range` 许可时可),失败回退 raw socket(需 root/`cap_net_raw`),两者皆不可用时该模块整体降级标注「需 root 运行」,不影响其余检测。回程线路识别基于骨干 ASN 表 + IP 前缀兜底,结果为**启发式**,仅供参考;电信 CN2 的 GIA/GT 细分按路径里的 `59.43`/`202.97` 段判定(含 `59.43` 不绕 `202.97` → GIA,绕 163 → GT,无 `59.43` → 通用 CN2),同样为启发式。当前仅 IPv4。

## 路线图

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 项目骨架、核心抽象 | ✅ |
| P1 | 免 key 基础源(ip-api/ipinfo/ip.sb)+ 合并渲染 | ✅ |
| P2 | **ip.net.coffee 风控/纯净度源**(trust_score/abuser/rep_threat/AI 判定)+ ping0 cookie 复用降级 | ✅ |
| P3 | **ippure 欺诈源**(fraudScore,egress 专用)+ ip-api 代理/机房交叉确认 | ✅ |
| P4 | **西方欺诈库**(AbuseIPDB + IPQS,key 可选,无 key 自动跳过)| ✅ |
| P5 | **横向对比表 + 启发式结论 + markdown 导出 + 中英 i18n** | ✅ |
| P6 | **解锁检测**(Netflix/YouTube/ChatGPT,`--probe`)| ✅ |
| P11 | **流媒体解锁大扩**(19 服务 + Region + Native/DNS 区分,`--probe`)| ✅ |
| P13 | **DNSBL 黑名单检测**(12 个主流列表,DNS 反向查询,`--dnsbl`)| ✅ |
| P14 | **--all 一键全跑 + 配置文件**(`~/.config/ipano/config.toml`,`-A`)| ✅ |
| P15 | **多节点下载测速** → v0.16 重做为**三网回国+国际测速**(speedtest.net 三网/港/教育/美日新,延迟+下载+上传,运行时解析,全目录可选,`--speedtest`)| ✅ |
| P7 | **邮局连通性**(SMTP 25/465/587,`--mail`)| ✅ |
| P8 | **ping0 token 手动复用**(`--ping0-token`,浏览器解验证码后提供,否则降级)| ✅ |
| P9 | **三网回程路由**(原生 Rust traceroute + 三网节点表 + 回程线路识别 + 每跳 AS/geo 标注,`--route`,需 root,无特权降级)| ✅ |
| P10 | **三网回程深化**(三网 × 四城 12 目标 + CN2 GIA/GT 细分 + 骨干 ASN 补全 + 单 socket 并行提速)| ✅ |
| P12 | **邮件端口全面化**(6 协议 SMTP/SMTPS/POP3/POP3S/IMAP/IMAPS × 15 家邮局矩阵,`--mail`)| ✅ |

完整设计见 [`docs/superpowers/specs/2026-06-11-ipano-design.md`](docs/superpowers/specs/2026-06-11-ipano-design.md);地基实现计划见 [`docs/superpowers/plans/2026-06-11-ipano-foundation.md`](docs/superpowers/plans/2026-06-11-ipano-foundation.md)。

## 架构

```
main → cli → orchestrator
   ├─ egress       本机出口 IP 探测(多端点取众数)
   ├─ fetch        共享 reqwest 客户端
   ├─ sources/     IP 信息源:每源一个文件,统一 Source trait,并发抓取
   │               ip-api · ipinfo · ip.sb · ip.net.coffee · ippure · ping0 · AbuseIPDB · IPQS
   ├─ probe/       主动探测(从本机出口发起,与查询 IP 无关,各自只跑一次):
   │               streaming/ai(解锁)· mail(SMTP 连通)· route(原生 traceroute 三网回程)
   │               · dnsbl(黑名单)· speedtest(三网回国+国际测速)
   ├─ aggregate    按优先级合并多源 → MergedReport
   ├─ heuristics   启发式风险结论
   └─ render/      terminal(彩色表)· json · markdown
```

新增数据源 = 加一个实现 `Source` trait 的文件并在 `all_sources()` 注册;新增探测器 = 在 `probe/` 加一个模块并在编排处接线 —— 都不动其它代码。

## 致谢

- [spiritysdx/za `ecs.sh`](https://gitlab.com/spiritysdx/za) —— VPS 综合测评脚本,形态参考
- [xykt/IPQuality](https://github.com/xykt/IPQuality) —— IP 质量检测黄金标准,完整度对标

## 开发

```bash
cargo test          # 运行全部单元/集成测试
cargo build --release
```

测试采用解析层纯函数单测 + httpmock 模拟抓取,不依赖真实网络。
