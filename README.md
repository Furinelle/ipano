# ipano

> **IP + Panorama** —— 一站式 IP 全景聚合检测工具。一个 IP,全景体检。

`ipano` 是一个用 Rust 编写的命令行工具,把多个 IP 信息源聚合到一份报告里:基础归属、ASN、IP 类型、风险/纯净度、代理标记等。编译为**单个静态二进制**,在 VPS 上下载即用,零运行时依赖。

## 当前状态

**v0.9.0 — P9 三网回程路由**。新增 `--route`,从本机出口对 电信/联通/移动 参考节点发起原生 ICMP traceroute,逐跳标注 AS/归属并按骨干 ASN 启发式识别回程线路(CN2/163/169/9929/CMI 等)。优先用免特权 ICMP DGRAM socket(macOS 即开即用),Linux 需 root/`cap_net_raw`,无特权时该模块自动降级,不影响其余检测。

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
- **解锁检测(`--probe`)**:从本机出口主动探测 Netflix、YouTube Premium、ChatGPT 的解锁状态与地区,失败标注未知不伪造
- **邮局连通性(`--mail`)**:TCP 连 SMTP 25/465/587 探测到 Gmail/Outlook/QQ/Yahoo/Apple 的端口连通(VPS 25 端口常被封,一眼可见)
- **三网回程路由(`--route`)**:原生 Rust ICMP traceroute 到 电信/联通/移动 北京参考节点,每跳复用 ip-api 标注 AS/归属,按骨干 ASN(CN2 AS4809 / 163 AS4134 / 169 AS4837 / 9929 / CMI AS58453 / CMNET AS9808 等)启发式识别回程线路类型与质量档(优质/普通);需 root/`cap_net_raw`,无特权自动降级

## 安装与构建

需要 [Rust 工具链](https://rustup.rs/)。

```bash
git clone https://github.com/Furinelle/ipano.git
cd ipano
cargo build --release
# 二进制在 target/release/ipano
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
ipano --probe          # 解锁检测(Netflix/YouTube/ChatGPT)
ipano --mail           # 邮局连通性(SMTP 25/465/587)
ipano --ping0-token <TOKEN>   # 复用浏览器解出的 ping0 token(60 秒内有效)
ipano --route          # 三网回程路由(原生 traceroute,需 root/cap_net_raw)
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

## 能力边界

`ipano` 跑在服务端,**无法**获取以下客户端浏览器行为(它们需要真实浏览器):浏览器指纹、WebRTC 泄露、DNS 泄露检测。报告中这类项会明确标注"CLI 不适用",不伪造数据。

**关于 ping0.cc**:ping0 现已被 Cloudflare Turnstile 验证码全站接管,且其 token 60 秒过期,无法程序化抓取(强行绕过验证码不在本工具范围)。ipano 仅支持 **cookie 复用**:在浏览器中解开 ping0 验证码后,把 `token` cookie 值通过环境变量 `IPANO_PING0_TOKEN` 提供(60 秒内有效),ipano 会在该窗口内复用;未提供或已失效时,ping0 源自动标注降级,不影响其它源。

**关于三网回程路由(`--route`)**:原生 ICMP traceroute 需 raw/dgram socket。ipano 优先用免特权 ICMP DGRAM socket(macOS 即可、Linux 受 `net.ipv4.ping_group_range` 许可时可),失败回退 raw socket(需 root/`cap_net_raw`),两者皆不可用时该模块整体降级标注「需 root 运行」,不影响其余检测。回程线路识别基于骨干 ASN 表,结果为**启发式**,仅供参考(CN2 GIA/GT 的细分需进一步看 59.43 节点,当前统一标 CN2)。当前仅 IPv4。

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
| P7 | **邮局连通性**(SMTP 25/465/587,`--mail`)| ✅ |
| P8 | **ping0 token 手动复用**(`--ping0-token`,浏览器解验证码后提供,否则降级)| ✅ |
| P9 | **三网回程路由**(原生 Rust traceroute + 三网节点表 + 回程线路识别 + 每跳 AS/geo 标注,`--route`,需 root,无特权降级)| ✅ |

完整设计见 [`docs/superpowers/specs/2026-06-11-ipano-design.md`](docs/superpowers/specs/2026-06-11-ipano-design.md);地基实现计划见 [`docs/superpowers/plans/2026-06-11-ipano-foundation.md`](docs/superpowers/plans/2026-06-11-ipano-foundation.md)。

## 架构

```
main → cli → orchestrator
   ├─ egress     本机出口 IP 探测(多端点取众数)
   ├─ fetch      共享 reqwest 客户端
   ├─ sources/   每源一个文件,统一 Source trait,并发抓取
   ├─ aggregate  按优先级合并多源 → MergedReport
   └─ render/    terminal(彩色表)· json
```

新增数据源 = 加一个实现 `Source` trait 的文件并在 `all_sources()` 注册,不动其它代码。

## 致谢

- [spiritysdx/za `ecs.sh`](https://gitlab.com/spiritysdx/za) —— VPS 综合测评脚本,形态参考
- [xykt/IPQuality](https://github.com/xykt/IPQuality) —— IP 质量检测黄金标准,完整度对标

## 开发

```bash
cargo test          # 运行全部单元/集成测试
cargo build --release
```

测试采用解析层纯函数单测 + httpmock 模拟抓取,不依赖真实网络。
