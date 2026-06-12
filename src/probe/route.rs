//! P9 三网回程路由(原生 traceroute)
//!
//! 从本机出口向 电信/联通/移动 三网参考节点各发一条原生 ICMP traceroute,
//! 每跳复用 ip-api 的 IP 信息层做 AS/geo 标注,再按骨干 ASN 表启发式识别回程线路类型。
//!
//! 特权:优先尝试 `SOCK_DGRAM`/`IPPROTO_ICMP`(macOS 免 root;Linux 受 ping_group_range 许可时免 root),
//! 失败回退 `SOCK_RAW`/`IPPROTO_ICMP`(需 root/cap_net_raw),两者皆失败 → 降级标注「需 root 运行」,不阻塞其余功能。
//! 识别结果均为启发式,仅供参考。

use std::net::Ipv4Addr;
use serde::Serialize;
use crate::i18n::Lang;

/// 默认最大跳数
pub const MAX_HOPS: u8 = 30;

// ───────────────────────── 三网参考目标 ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Carrier {
    Telecom, // 电信
    Unicom,  // 联通
    Mobile,  // 移动
}

impl Carrier {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Carrier::Telecom => lang.pick("电信", "Telecom"),
            Carrier::Unicom => lang.pick("联通", "Unicom"),
            Carrier::Mobile => lang.pick("移动", "Mobile"),
        }
    }
}

/// (运营商, 节点名, 参考 IP)。北京三网常用骨干节点,可后续扩充上海/广州。
pub fn targets() -> Vec<(Carrier, &'static str, Ipv4Addr)> {
    vec![
        (Carrier::Telecom, "北京电信", Ipv4Addr::new(219, 141, 136, 12)),
        (Carrier::Unicom, "北京联通", Ipv4Addr::new(202, 106, 50, 1)),
        (Carrier::Mobile, "北京移动", Ipv4Addr::new(211, 136, 25, 153)),
    ]
}

// ───────────────────────── 回程线路类型 ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    Cn2,         // 电信 CN2(AS4809,GIA/GT 需进一步看 59.43 节点,此处统一标 CN2)
    Chinanet163, // 电信 163(AS4134)
    Cuii9929,    // 联通 9929/CUII(AS9929/AS58807)
    Cug,         // 联通 CUG(AS10099)
    China169,    // 联通 169(AS4837)
    Cmi,         // 移动 CMI(AS58453)
    Cmnet,       // 移动 CMNET(AS9808/AS56040)
    Unknown,
}

impl LineType {
    /// 中英展示文案
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            LineType::Cn2 => lang.pick("电信 CN2 (AS4809)", "Telecom CN2 (AS4809)"),
            LineType::Chinanet163 => lang.pick("电信 163 (AS4134)", "Telecom 163 (AS4134)"),
            LineType::Cuii9929 => lang.pick("联通 9929/CUII (AS9929)", "Unicom 9929/CUII (AS9929)"),
            LineType::Cug => lang.pick("联通 CUG (AS10099)", "Unicom CUG (AS10099)"),
            LineType::China169 => lang.pick("联通 169 (AS4837)", "Unicom 169 (AS4837)"),
            LineType::Cmi => lang.pick("移动 CMI (AS58453)", "Mobile CMI (AS58453)"),
            LineType::Cmnet => lang.pick("移动 CMNET (AS9808)", "Mobile CMNET (AS9808)"),
            LineType::Unknown => lang.pick("未识别", "Unknown"),
        }
    }
    /// 质量档:优质 / 普通 / 未知(启发式)
    pub fn quality(self, lang: Lang) -> &'static str {
        match self {
            LineType::Cn2 | LineType::Cuii9929 | LineType::Cug | LineType::Cmi => {
                lang.pick("优质", "Premium")
            }
            LineType::Chinanet163 | LineType::China169 | LineType::Cmnet => {
                lang.pick("普通", "Standard")
            }
            LineType::Unknown => lang.pick("—", "—"),
        }
    }
}

/// 骨干 ASN → (归属运营商, 线路类型, 优先级)。优先级越高越「精品」,识别时取路径中本运营商最高优先级者。
fn backbone_of(asn: u32) -> Option<(Carrier, LineType, u8)> {
    match asn {
        4809 => Some((Carrier::Telecom, LineType::Cn2, 9)),
        4134 => Some((Carrier::Telecom, LineType::Chinanet163, 3)),
        9929 => Some((Carrier::Unicom, LineType::Cuii9929, 9)),
        58807 => Some((Carrier::Unicom, LineType::Cuii9929, 8)),
        10099 => Some((Carrier::Unicom, LineType::Cug, 7)),
        4837 => Some((Carrier::Unicom, LineType::China169, 3)),
        58453 => Some((Carrier::Mobile, LineType::Cmi, 9)),
        9808 | 56040 => Some((Carrier::Mobile, LineType::Cmnet, 3)),
        _ => None,
    }
}

/// 在一条 trace 的 ASN 集合里,识别该运营商的回程线路类型(启发式:取本运营商最高优先级骨干)
pub fn classify_line(carrier: Carrier, asns: &[u32]) -> LineType {
    let mut best: Option<(LineType, u8)> = None;
    for &asn in asns {
        if let Some((c, lt, prio)) = backbone_of(asn) {
            if c == carrier && best.map(|(_, p)| prio > p).unwrap_or(true) {
                best = Some((lt, prio));
            }
        }
    }
    best.map(|(lt, _)| lt).unwrap_or(LineType::Unknown)
}

// ───────────────────────── 数据结构 ─────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Hop {
    pub ttl: u8,
    pub addr: Option<Ipv4Addr>, // None = 该跳无应答(*)
    pub rtt_ms: Option<f64>,
    pub asn: Option<u32>,
    pub as_org: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteResult {
    pub carrier: Carrier,
    pub target_name: String,
    pub target: Ipv4Addr,
    pub hops: Vec<Hop>,
    pub line: LineType,
    /// 降级原因(如「需 root 运行」);非空时 hops 为空
    pub degraded: Option<String>,
}

// ───────────────────────── ICMP 报文(纯逻辑,可测) ─────────────────────────

/// RFC 1071 因特网校验和
pub fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for c in &mut chunks {
        sum += u16::from_be_bytes([c[0], c[1]]) as u32;
    }
    if let [last] = chunks.remainder() {
        sum += (*last as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// 构造 ICMP Echo Request:type=8 code=0 + id/seq + payload
pub fn build_echo_request(id: u16, seq: u16, payload_len: usize) -> Vec<u8> {
    let mut pkt = vec![0u8; 8 + payload_len];
    pkt[0] = 8; // echo request
    pkt[1] = 0;
    pkt[4..6].copy_from_slice(&id.to_be_bytes());
    pkt[6..8].copy_from_slice(&seq.to_be_bytes());
    for (i, b) in pkt[8..].iter_mut().enumerate() {
        *b = (i & 0xff) as u8;
    }
    let csum = internet_checksum(&pkt);
    pkt[2..4].copy_from_slice(&csum.to_be_bytes());
    pkt
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcmpKind {
    EchoReply,       // 到达目标
    TimeExceeded,    // 中间路由(TTL 耗尽)
    DestUnreachable, // 目标不可达(也视为「到达」)
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IcmpInfo {
    pub kind: IcmpKind,
    pub seq: u16,
}

/// 解析收到的 ICMP 报文,返回类型与其对应的探测 seq。
/// 自动跳过可能存在的外层 IP 头(RAW socket 带、DGRAM 多不带)。
/// TimeExceeded/DestUnreachable 从内嵌的原始报文里取回 seq。
pub fn parse_icmp(buf: &[u8]) -> Option<IcmpInfo> {
    let icmp = strip_ip_header(buf);
    if icmp.len() < 8 {
        return None;
    }
    match icmp[0] {
        0 => Some(IcmpInfo {
            kind: IcmpKind::EchoReply,
            seq: u16::from_be_bytes([icmp[6], icmp[7]]),
        }),
        11 | 3 => {
            // 内嵌:原始 IP 头 + 原始 ICMP 前 8 字节
            let inner = strip_ip_header(&icmp[8..]);
            if inner.len() < 8 || inner[0] != 8 {
                return None;
            }
            let kind = if icmp[0] == 11 {
                IcmpKind::TimeExceeded
            } else {
                IcmpKind::DestUnreachable
            };
            Some(IcmpInfo {
                kind,
                seq: u16::from_be_bytes([inner[6], inner[7]]),
            })
        }
        _ => Some(IcmpInfo { kind: IcmpKind::Other, seq: 0 }),
    }
}

/// 若 buf 以 IPv4 头开头(版本号 4),跳过 IHL*4 字节,否则原样返回
fn strip_ip_header(buf: &[u8]) -> &[u8] {
    if !buf.is_empty() && (buf[0] >> 4) == 4 {
        let ihl = ((buf[0] & 0x0f) as usize) * 4;
        if ihl >= 20 && buf.len() >= ihl {
            return &buf[ihl..];
        }
    }
    buf
}

/// 仅对公网可路由 IPv4 做 geo 标注:跳过私网/环回/链路本地/CGNAT 等
pub fn is_public_v4(ip: Ipv4Addr) -> bool {
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.octets()[0] == 0
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 0x40) // 100.64/10 CGNAT
        || (ip.octets()[0] == 198 && (ip.octets()[1] & 0xfe) == 18)) // 198.18/15 基准测试(Surge TUN 网关)
}

// ───────────────────────── 渲染 ─────────────────────────

/// 渲染三网回程路由区(Markdown 表,终端与 markdown 通用)
pub fn render_section(routes: &[RouteResult], lang: Lang) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "## {}\n", lang.pick("三网回程路由(traceroute)", "China route (traceroute)")).ok();
    writeln!(out, "> {}", lang.pick(
        "启发式识别,仅供参考;需 root/cap_net_raw,无特权自动降级",
        "Heuristic; for reference only. Needs root/cap_net_raw, auto-degrades otherwise",
    )).ok();
    writeln!(out).ok();

    // 概览表
    writeln!(out, "| {} | {} | {} | {} | {} |",
        lang.pick("运营商", "Carrier"),
        lang.pick("目标节点", "Target"),
        lang.pick("回程线路", "Return line"),
        lang.pick("质量", "Quality"),
        lang.pick("跳数", "Hops"),
    ).ok();
    writeln!(out, "|---|---|---|---|---|").ok();
    for r in routes {
        if let Some(reason) = &r.degraded {
            // "need_privilege" 为稳定标记,渲染期按语言翻译;其它原因原样透出
            let txt = if reason == "need_privilege" {
                lang.pick("需 root 运行", "needs root/cap_net_raw")
            } else {
                reason.as_str()
            };
            writeln!(out, "| {} | {} {} | {} | — | — |",
                r.carrier.label(lang), r.target_name, r.target, txt).ok();
        } else {
            writeln!(out, "| {} | {} {} | {} | {} | {} |",
                r.carrier.label(lang), r.target_name, r.target,
                r.line.label(lang), r.line.quality(lang), r.hops.len()).ok();
        }
    }
    if routes.iter().any(|r| r.degraded.is_some()) {
        writeln!(out, "\n> {}", lang.pick(
            "部分目标因无特权已降级:用 `sudo ipano --route`,或先 `sudo setcap cap_net_raw+ep <二进制>` 一次后免 sudo",
            "Some targets degraded (no privilege): run `sudo ipano --route`, or `sudo setcap cap_net_raw+ep <binary>` once to avoid sudo",
        )).ok();
    }
    out.push('\n');

    // 每条 trace 的逐跳明细(仅非降级)
    for r in routes {
        if r.degraded.is_some() || r.hops.is_empty() {
            continue;
        }
        writeln!(out, "### {} {} {}\n", r.carrier.label(lang), r.target_name, r.target).ok();
        writeln!(out, "| # | IP | RTT | AS | {} |",
            lang.pick("归属", "Location")).ok();
        writeln!(out, "|---|---|---|---|---|").ok();
        for h in &r.hops {
            let ip = h.addr.map(|a| a.to_string()).unwrap_or_else(|| "*".into());
            let rtt = h.rtt_ms.map(|v| format!("{:.1}ms", v)).unwrap_or_else(|| "—".into());
            let asn = h.asn.map(|a| format!("AS{}", a)).unwrap_or_else(|| "—".into());
            let org = h.as_org.clone().unwrap_or_default();
            let loc = match (&h.country, &h.city) {
                (Some(c), Some(ci)) => format!("{} {}", c, ci),
                (Some(c), None) => c.clone(),
                _ => "—".into(),
            };
            writeln!(out, "| {} | {} | {} | {} {} | {} |", h.ttl, ip, rtt, asn, org, loc).ok();
        }
        out.push('\n');
    }
    out
}

// ───────────────────────── 运行引擎 ─────────────────────────

mod engine;
pub use engine::run_routes;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_zero_for_self_consistent_packet() {
        // 一个含正确校验和的包,整体再算校验和应为 0
        let pkt = build_echo_request(0x1234, 7, 32);
        assert_eq!(internet_checksum(&pkt), 0);
    }

    #[test]
    fn echo_request_has_type_and_seq() {
        let pkt = build_echo_request(0xABCD, 5, 16);
        assert_eq!(pkt.len(), 24);
        assert_eq!(pkt[0], 8); // echo request
        assert_eq!(u16::from_be_bytes([pkt[4], pkt[5]]), 0xABCD);
        assert_eq!(u16::from_be_bytes([pkt[6], pkt[7]]), 5);
    }

    #[test]
    fn parse_echo_reply() {
        // 无 IP 头,直接 ICMP echo reply(type 0),seq=9
        let mut buf = vec![0u8; 8];
        buf[0] = 0;
        buf[6..8].copy_from_slice(&9u16.to_be_bytes());
        let info = parse_icmp(&buf).unwrap();
        assert_eq!(info.kind, IcmpKind::EchoReply);
        assert_eq!(info.seq, 9);
    }

    #[test]
    fn parse_time_exceeded_extracts_inner_seq() {
        // 外层 ICMP type=11 + 内嵌原始 IP 头(20B)+ 原始 echo 前 8B(seq=4)
        let mut outer = vec![0u8; 8];
        outer[0] = 11; // time exceeded
        let mut inner_ip = vec![0u8; 20];
        inner_ip[0] = 0x45; // IPv4, IHL=5
        let mut inner_icmp = vec![0u8; 8];
        inner_icmp[0] = 8; // 原始 echo request
        inner_icmp[6..8].copy_from_slice(&4u16.to_be_bytes());
        let mut buf = outer.clone();
        buf.extend_from_slice(&inner_ip);
        buf.extend_from_slice(&inner_icmp);
        let info = parse_icmp(&buf).unwrap();
        assert_eq!(info.kind, IcmpKind::TimeExceeded);
        assert_eq!(info.seq, 4);
    }

    #[test]
    fn parse_skips_outer_ip_header() {
        // 外层带 IPv4 头(RAW socket 形态)+ echo reply seq=2
        let mut buf = vec![0u8; 20];
        buf[0] = 0x45;
        let mut icmp = vec![0u8; 8];
        icmp[0] = 0;
        icmp[6..8].copy_from_slice(&2u16.to_be_bytes());
        buf.extend_from_slice(&icmp);
        let info = parse_icmp(&buf).unwrap();
        assert_eq!(info.kind, IcmpKind::EchoReply);
        assert_eq!(info.seq, 2);
    }

    #[test]
    fn classify_picks_premium_cn2_over_163() {
        // 路径里既有 163(4134)又有 CN2(4809),应识别为 CN2
        let lt = classify_line(Carrier::Telecom, &[4134, 4809, 4134]);
        assert_eq!(lt, LineType::Cn2);
    }

    #[test]
    fn classify_unicom_9929() {
        assert_eq!(classify_line(Carrier::Unicom, &[4837, 9929]), LineType::Cuii9929);
        assert_eq!(classify_line(Carrier::Unicom, &[4837]), LineType::China169);
    }

    #[test]
    fn classify_mobile_cmi_vs_cmnet() {
        assert_eq!(classify_line(Carrier::Mobile, &[9808, 58453]), LineType::Cmi);
        assert_eq!(classify_line(Carrier::Mobile, &[9808]), LineType::Cmnet);
    }

    #[test]
    fn classify_ignores_other_carrier_asns() {
        // 给电信传联通的 ASN,识别不到电信骨干 → Unknown
        assert_eq!(classify_line(Carrier::Telecom, &[4837, 9929]), LineType::Unknown);
    }

    #[test]
    fn line_quality_labels() {
        assert_eq!(LineType::Cn2.quality(Lang::Zh), "优质");
        assert_eq!(LineType::Chinanet163.quality(Lang::Zh), "普通");
        assert_eq!(LineType::Cmi.quality(Lang::En), "Premium");
    }

    #[test]
    fn public_v4_filter() {
        assert!(is_public_v4(Ipv4Addr::new(219, 141, 136, 12)));
        assert!(!is_public_v4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!is_public_v4(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(!is_public_v4(Ipv4Addr::new(100, 64, 0, 1))); // CGNAT
        assert!(!is_public_v4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(!is_public_v4(Ipv4Addr::new(198, 18, 0, 1))); // 198.18/15 基准/Surge TUN
        assert!(!is_public_v4(Ipv4Addr::new(198, 19, 255, 1)));
    }

    #[test]
    fn render_degraded_row() {
        let routes = vec![RouteResult {
            carrier: Carrier::Telecom,
            target_name: "北京电信".into(),
            target: Ipv4Addr::new(219, 141, 136, 12),
            hops: vec![],
            line: LineType::Unknown,
            degraded: Some("需 root 运行".into()),
        }];
        let s = render_section(&routes, Lang::Zh);
        assert!(s.contains("三网回程路由"));
        assert!(s.contains("需 root 运行"));
        assert!(s.contains("北京电信"));
        // 有降级目标时应给出 sudo 重试提示
        assert!(s.contains("sudo"));
    }

    #[test]
    fn render_hops_table() {
        let routes = vec![RouteResult {
            carrier: Carrier::Telecom,
            target_name: "北京电信".into(),
            target: Ipv4Addr::new(219, 141, 136, 12),
            hops: vec![
                Hop { ttl: 1, addr: Some(Ipv4Addr::new(192, 168, 1, 1)), rtt_ms: Some(0.8), asn: None, as_org: None, country: None, city: None },
                Hop { ttl: 2, addr: None, rtt_ms: None, asn: None, as_org: None, country: None, city: None },
                Hop { ttl: 3, addr: Some(Ipv4Addr::new(219, 141, 136, 12)), rtt_ms: Some(33.2), asn: Some(4809), as_org: Some("Chinanet".into()), country: Some("CN".into()), city: Some("Beijing".into()) },
            ],
            line: LineType::Cn2,
            degraded: None,
        }];
        let s = render_section(&routes, Lang::Zh);
        assert!(s.contains("电信 CN2 (AS4809)"));
        assert!(s.contains("优质"));
        assert!(s.contains("AS4809"));
        assert!(s.contains("*")); // 无应答跳
        assert!(s.contains("33.2ms"));
    }
}
