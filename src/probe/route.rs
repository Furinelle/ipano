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

/// 测试城市(三网各四城)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum City {
    Beijing,   // 北京
    Shanghai,  // 上海
    Guangzhou, // 广州
    Chengdu,   // 成都
}

impl City {
    /// 稳定展示次序键(北京→上海→广州→成都)
    pub fn order(self) -> u8 {
        match self {
            City::Beijing => 0,
            City::Shanghai => 1,
            City::Guangzhou => 2,
            City::Chengdu => 3,
        }
    }
}

/// (运营商, 城市, 节点名, 参考 IP)。三网 × 北京/上海/广州/成都 = 12 个骨干参考节点。
/// 目标 IP 取自社区 backtrace 工具(zhanghanyun/backtrace)的事实标准集;识别均为启发式。
pub fn targets() -> Vec<(Carrier, City, &'static str, Ipv4Addr)> {
    use Carrier::*;
    use City::*;
    vec![
        // 电信四城
        (Telecom, Beijing, "北京电信", Ipv4Addr::new(219, 141, 140, 10)),
        (Telecom, Shanghai, "上海电信", Ipv4Addr::new(202, 96, 209, 133)),
        (Telecom, Guangzhou, "广州电信", Ipv4Addr::new(58, 60, 188, 222)),
        (Telecom, Chengdu, "成都电信", Ipv4Addr::new(61, 139, 2, 69)),
        // 联通四城
        (Unicom, Beijing, "北京联通", Ipv4Addr::new(202, 106, 195, 68)),
        (Unicom, Shanghai, "上海联通", Ipv4Addr::new(210, 22, 97, 1)),
        (Unicom, Guangzhou, "广州联通", Ipv4Addr::new(210, 21, 196, 6)),
        (Unicom, Chengdu, "成都联通", Ipv4Addr::new(119, 6, 6, 6)),
        // 移动四城
        (Mobile, Beijing, "北京移动", Ipv4Addr::new(221, 179, 155, 161)),
        (Mobile, Shanghai, "上海移动", Ipv4Addr::new(211, 136, 112, 200)),
        (Mobile, Guangzhou, "广州移动", Ipv4Addr::new(120, 196, 165, 24)),
        (Mobile, Chengdu, "成都移动", Ipv4Addr::new(211, 137, 96, 205)),
    ]
}

// ───────────────────────── 回程线路类型 ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    Cn2gia,      // 电信 CN2 GIA(AS4809,全程 CN2,精品)
    Cn2gt,       // 电信 CN2 GT(AS4809,经 163/202.97 中转)
    Cn2,         // 电信 CN2(AS4809,GIA/GT 不可辨时的通用标注)
    Chinanet163, // 电信 163(AS4134)
    Cuii9929,    // 联通 9929/CUII(AS9929/AS4847)
    Cug,         // 联通 CUG(AS10099)
    China169,    // 联通 169(AS4837/AS4808/AS17623)
    Cmin2,       // 移动 CMIN2(AS58807,精品)
    Cmi,         // 移动 CMI(AS58453)
    Cmnet,       // 移动 CMNET(AS9808/AS56040/AS56048/AS134774)
    Unknown,
}

impl LineType {
    /// 中英展示文案
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            LineType::Cn2gia => lang.pick("电信 CN2 GIA (AS4809)", "Telecom CN2 GIA (AS4809)"),
            LineType::Cn2gt => lang.pick("电信 CN2 GT (AS4809)", "Telecom CN2 GT (AS4809)"),
            LineType::Cn2 => lang.pick("电信 CN2 (AS4809)", "Telecom CN2 (AS4809)"),
            LineType::Chinanet163 => lang.pick("电信 163 (AS4134)", "Telecom 163 (AS4134)"),
            LineType::Cuii9929 => lang.pick("联通 9929/CUII (AS9929)", "Unicom 9929/CUII (AS9929)"),
            LineType::Cug => lang.pick("联通 CUG (AS10099)", "Unicom CUG (AS10099)"),
            LineType::China169 => lang.pick("联通 169 (AS4837)", "Unicom 169 (AS4837)"),
            LineType::Cmin2 => lang.pick("移动 CMIN2 (AS58807)", "Mobile CMIN2 (AS58807)"),
            LineType::Cmi => lang.pick("移动 CMI (AS58453)", "Mobile CMI (AS58453)"),
            LineType::Cmnet => lang.pick("移动 CMNET (AS9808)", "Mobile CMNET (AS9808)"),
            LineType::Unknown => lang.pick("未识别", "Unknown"),
        }
    }
    /// 线路等级(精品/优质/普通/未知,启发式),对齐 oneclickvirt/backtrace
    pub fn grade(self) -> Grade {
        match self {
            LineType::Cn2gia | LineType::Cmin2 => Grade::Boutique,
            LineType::Cn2gt | LineType::Cn2 | LineType::Cuii9929 | LineType::Cug => Grade::Premium,
            LineType::Chinanet163 | LineType::China169 | LineType::Cmi | LineType::Cmnet => Grade::Standard,
            LineType::Unknown => Grade::Unknown,
        }
    }
}

/// 回程线路等级(对齐 oneclickvirt/backtrace 三档:精品 > 优质 > 普通)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Grade { Boutique, Premium, Standard, Unknown }

impl Grade {
    /// 带方括号的展示标签,如「[精品线路]」
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Grade::Boutique => lang.pick("[精品线路]", "[Boutique]"),
            Grade::Premium  => lang.pick("[优质线路]", "[Premium]"),
            Grade::Standard => lang.pick("[普通线路]", "[Standard]"),
            Grade::Unknown  => "—",
        }
    }
    /// 终端着色:精品紫 / 优质绿 / 普通黄 / 未知灰
    pub fn color(self) -> comfy_table::Color {
        use comfy_table::Color;
        match self {
            Grade::Boutique => Color::Magenta,
            Grade::Premium  => Color::Green,
            Grade::Standard => Color::Yellow,
            Grade::Unknown  => Color::DarkGrey,
        }
    }
}

/// 骨干 ASN → (归属运营商, 线路类型, 优先级)。优先级越高越「精品」,识别时取路径中本运营商最高优先级者。
fn backbone_of(asn: u32) -> Option<(Carrier, LineType, u8)> {
    match asn {
        // 电信
        4809 => Some((Carrier::Telecom, LineType::Cn2, 9)), // GIA/GT 由 refine_cn2 进一步细分
        4134 => Some((Carrier::Telecom, LineType::Chinanet163, 3)),
        // 联通
        9929 => Some((Carrier::Unicom, LineType::Cuii9929, 9)),
        4847 => Some((Carrier::Unicom, LineType::Cuii9929, 8)), // CUII 骨干族(China Networks Inter-Exchange)
        10099 => Some((Carrier::Unicom, LineType::Cug, 7)),
        4837 | 4808 | 17623 => Some((Carrier::Unicom, LineType::China169, 3)),
        // 移动
        58807 => Some((Carrier::Mobile, LineType::Cmin2, 9)), // CMIN2 精品(原误标联通,已纠正)
        58453 => Some((Carrier::Mobile, LineType::Cmi, 4)),
        9808 | 56040 | 56048 | 134774 => Some((Carrier::Mobile, LineType::Cmnet, 3)),
        _ => None,
    }
}

/// 仅凭 IP 前缀的兜底 ASN 推断:ip-api 未给出 AS 号时使用。
/// 前缀集源自社区 backtrace 工具(zhanghanyun/backtrace)的 `ipAsn`,均为启发式。
pub fn asn_from_prefix(ip: Ipv4Addr) -> Option<u32> {
    let o = ip.octets();
    match (o[0], o[1], o[2]) {
        (59, 43, _) => Some(4809),                        // 电信 CN2
        (202, 97, _) => Some(4134),                       // 电信 163 骨干
        (218, 105, _) | (210, 51, _) => Some(9929),       // 联通 9929/CUII
        (219, 158, _) => Some(4837),                      // 联通 169 骨干
        (223, 120, 16) | (223, 120, 17) | (223, 120, 19) => Some(58807), // 移动 CMIN2(更细前缀先匹配)
        (223, 118, _) | (223, 119, _) | (223, 120, _) | (223, 121, _) => Some(58453), // 移动 CMI
        _ => None,
    }
}

/// CN2 GIA/GT 细分(启发式):仅当线路判为通用 `Cn2`(AS4809)时,看路径里的 59.43 / 202.97 段。
/// 含 59.43 且绕 202.97(163 骨干)→ 判 GT;含 59.43 且不绕 163 → 判 GIA;无 59.43 → 维持通用 CN2。
pub fn refine_cn2(line: LineType, hop_ips: &[Ipv4Addr]) -> LineType {
    if line != LineType::Cn2 {
        return line;
    }
    let has_cn2 = hop_ips.iter().any(|ip| {
        let o = ip.octets();
        o[0] == 59 && o[1] == 43
    });
    if !has_cn2 {
        return LineType::Cn2;
    }
    let via_163 = hop_ips.iter().any(|ip| {
        let o = ip.octets();
        o[0] == 202 && o[1] == 97
    });
    if via_163 {
        LineType::Cn2gt
    } else {
        LineType::Cn2gia
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

/// 国际入境线识别:不限运营商,取全路径里优先级最高的骨干。
/// 揭示「这条 trace 实际经哪家入境」(如三网都经联通 CUG)。
pub fn classify_entry(asns: &[u32]) -> LineType {
    let mut best: Option<(LineType, u8)> = None;
    for &asn in asns {
        if let Some((_c, lt, prio)) = backbone_of(asn) {
            if best.map(|(_, p)| prio > p).unwrap_or(true) {
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
    pub city: City,
    pub target_name: String,
    pub target: Ipv4Addr,
    pub hops: Vec<Hop>,
    /// 该运营商自己的回程骨干(启发式)
    pub line: LineType,
    /// 回程线路等级(精品/优质/普通,由 line 推导)
    pub grade: Grade,
    /// 国际入境线:全路径里优先级最高的骨干(不限运营商),揭示「经哪家入境」
    pub entry: LineType,
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
    writeln!(out, "| {} | {} | {} | {} | {} | {} |",
        lang.pick("运营商", "Carrier"),
        lang.pick("目标节点", "Target"),
        lang.pick("入境线", "Entry"),
        lang.pick("回程线路", "Return line"),
        lang.pick("质量", "Quality"),
        lang.pick("跳数", "Hops"),
    ).ok();
    writeln!(out, "|---|---|---|---|---|---|").ok();
    for r in routes {
        if let Some(reason) = &r.degraded {
            // "need_privilege" 为稳定标记,渲染期按语言翻译;其它原因原样透出
            let txt = if reason == "need_privilege" {
                lang.pick("需 root 运行", "needs root/cap_net_raw")
            } else {
                reason.as_str()
            };
            writeln!(out, "| {} | {} {} | — | {} | — | — |",
                r.carrier.label(lang), r.target_name, r.target, txt).ok();
        } else {
            writeln!(out, "| {} | {} {} | {} | {} | {} | {} |",
                r.carrier.label(lang), r.target_name, r.target,
                r.entry.label(lang), r.line.label(lang), r.line.grade().label(lang), r.hops.len()).ok();
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

/// 渲染三网回程路由区(comfy-table 包边表,终端用;与主报告风格一致)
pub fn render_terminal(routes: &[RouteResult], lang: Lang, no_color: bool) -> String {
    use comfy_table::{presets::UTF8_FULL, Cell, Table};
    let mut out = String::new();
    out.push_str(&format!("═══ {} ═══\n",
        lang.pick("三网回程路由(traceroute)", "China route (traceroute)")));
    out.push_str(&format!("{}\n", lang.pick(
        "启发式识别,仅供参考;需 root/cap_net_raw,无特权自动降级",
        "Heuristic; for reference only. Needs root/cap_net_raw, auto-degrades otherwise",
    )));

    // 概览表
    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(vec![
        lang.pick("运营商", "Carrier"),
        lang.pick("目标节点", "Target"),
        lang.pick("入境线", "Entry"),
        lang.pick("回程线路", "Return line"),
        lang.pick("质量", "Quality"),
        lang.pick("跳数", "Hops"),
    ]);
    for r in routes {
        let target = format!("{} {}", r.target_name, r.target);
        if let Some(reason) = &r.degraded {
            let txt = if reason == "need_privilege" {
                lang.pick("需 root 运行", "needs root/cap_net_raw")
            } else { reason.as_str() };
            t.add_row(vec![Cell::new(r.carrier.label(lang)), Cell::new(target),
                Cell::new("—"), Cell::new(txt), Cell::new("—"), Cell::new("—")]);
        } else {
            let grade_cell = {
                let c = Cell::new(r.grade.label(lang));
                if no_color { c } else { c.fg(r.grade.color()) }
            };
            t.add_row(vec![
                Cell::new(r.carrier.label(lang)), Cell::new(target),
                Cell::new(r.entry.label(lang)), Cell::new(r.line.label(lang)),
                grade_cell, Cell::new(r.hops.len().to_string()),
            ]);
        }
    }
    out.push_str(&t.to_string());
    out.push('\n');

    if routes.iter().any(|r| r.degraded.is_some()) {
        out.push_str(&format!("{}\n", lang.pick(
            "部分目标因无特权已降级:用 `sudo ipano --route`,或先 `sudo setcap cap_net_raw+ep <二进制>` 一次后免 sudo",
            "Some targets degraded (no privilege): run `sudo ipano --route`, or `sudo setcap cap_net_raw+ep <binary>` once",
        )));
    }

    // 每条 trace 的逐跳明细
    for r in routes {
        if r.degraded.is_some() || r.hops.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{} {} {}\n", r.carrier.label(lang), r.target_name, r.target));
        let mut ht = Table::new();
        ht.load_preset(UTF8_FULL);
        ht.set_header(vec!["#", "IP", "RTT", "AS", lang.pick("归属", "Location")]);
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
            ht.add_row(vec![h.ttl.to_string(), ip, rtt, format!("{} {}", asn, org), loc]);
        }
        out.push_str(&ht.to_string());
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
    fn entry_picks_dominant_backbone_any_carrier() {
        // 路径经联通 CUG(10099)+169(4837):入境线取优先级最高者 = CUG,不限运营商
        assert_eq!(classify_entry(&[4837, 10099, 9808]), LineType::Cug);
        // 目标是电信(无电信骨干)→ 该运营商回程线 Unknown,但入境线仍能识别其它家骨干
        assert_eq!(classify_line(Carrier::Telecom, &[10099, 4847]), LineType::Unknown);
        // 4847(CUII 族,prio 8)优先级高于 CUG(prio 7),入境线取 CUII
        assert_eq!(classify_entry(&[10099, 4847]), LineType::Cuii9929);
    }

    #[test]
    fn classify_extended_asn_unicom_mobile() {
        // 补全的联通 169 骨干 4808/17623
        assert_eq!(classify_line(Carrier::Unicom, &[4808]), LineType::China169);
        assert_eq!(classify_line(Carrier::Unicom, &[17623]), LineType::China169);
        // 补全的移动 CMNET 骨干 56048/134774
        assert_eq!(classify_line(Carrier::Mobile, &[56048]), LineType::Cmnet);
        assert_eq!(classify_line(Carrier::Mobile, &[134774]), LineType::Cmnet);
    }

    #[test]
    fn mobile_cmin2_is_premium_and_beats_cmnet() {
        // AS58807 = 移动 CMIN2 精品(prio 9),路径同时有 CMNET(9808)时取 CMIN2
        assert_eq!(classify_line(Carrier::Mobile, &[9808, 58807]), LineType::Cmin2);
        assert_eq!(LineType::Cmin2.grade(), Grade::Boutique);
        // 58807 归属移动(历史误标联通,已纠正):给联通分类应识别不到
        assert_eq!(classify_line(Carrier::Unicom, &[58807]), LineType::Unknown);
    }

    #[test]
    fn cn2_gia_gt_refine() {
        // 含 59.43 且不绕 163 → GIA
        let gia = refine_cn2(LineType::Cn2, &[
            Ipv4Addr::new(59, 43, 130, 1),
            Ipv4Addr::new(202, 96, 209, 133),
        ]);
        assert_eq!(gia, LineType::Cn2gia);
        // 含 59.43 且绕 202.97(163 骨干)→ GT
        let gt = refine_cn2(LineType::Cn2, &[
            Ipv4Addr::new(202, 97, 50, 1),
            Ipv4Addr::new(59, 43, 12, 9),
        ]);
        assert_eq!(gt, LineType::Cn2gt);
        // 无 59.43 → 维持通用 CN2
        assert_eq!(refine_cn2(LineType::Cn2, &[Ipv4Addr::new(202, 97, 1, 1)]), LineType::Cn2);
        // 非 CN2 线路 → 原样返回,不误改
        assert_eq!(refine_cn2(LineType::China169, &[Ipv4Addr::new(59, 43, 1, 1)]), LineType::China169);
    }

    #[test]
    fn asn_prefix_fallback() {
        assert_eq!(asn_from_prefix(Ipv4Addr::new(59, 43, 130, 1)), Some(4809));
        assert_eq!(asn_from_prefix(Ipv4Addr::new(202, 97, 50, 1)), Some(4134));
        assert_eq!(asn_from_prefix(Ipv4Addr::new(218, 105, 0, 1)), Some(9929));
        assert_eq!(asn_from_prefix(Ipv4Addr::new(219, 158, 1, 1)), Some(4837));
        // 223.120.16/17/19 = CMIN2,须先于宽前缀 223.120 → CMI 命中
        assert_eq!(asn_from_prefix(Ipv4Addr::new(223, 120, 19, 5)), Some(58807));
        assert_eq!(asn_from_prefix(Ipv4Addr::new(223, 120, 18, 5)), Some(58453));
        assert_eq!(asn_from_prefix(Ipv4Addr::new(223, 118, 0, 1)), Some(58453));
        // 非骨干前缀 → None
        assert_eq!(asn_from_prefix(Ipv4Addr::new(8, 8, 8, 8)), None);
    }

    #[test]
    fn line_grade_labels() {
        // 三档:精品(CN2 GIA / 移动 CMIN2)> 优质(CN2 / 联通 9929)> 普通(163/169/CMI/CMNET)
        assert_eq!(LineType::Cn2gia.grade(), Grade::Boutique);
        assert_eq!(LineType::Cmin2.grade(), Grade::Boutique);
        assert_eq!(LineType::Cn2.grade(), Grade::Premium);
        assert_eq!(LineType::Cuii9929.grade(), Grade::Premium);
        assert_eq!(LineType::Chinanet163.grade(), Grade::Standard);
        assert_eq!(LineType::Cmi.grade(), Grade::Standard);
        assert_eq!(LineType::Unknown.grade(), Grade::Unknown);
        // 括号标签
        assert_eq!(Grade::Boutique.label(Lang::Zh), "[精品线路]");
        assert_eq!(Grade::Premium.label(Lang::En), "[Premium]");
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
            city: City::Beijing,
            target_name: "北京电信".into(),
            target: Ipv4Addr::new(219, 141, 136, 12),
            hops: vec![],
            line: LineType::Unknown,
            grade: Grade::Unknown,
            entry: LineType::Unknown,
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
            city: City::Beijing,
            target_name: "北京电信".into(),
            target: Ipv4Addr::new(219, 141, 136, 12),
            hops: vec![
                Hop { ttl: 1, addr: Some(Ipv4Addr::new(192, 168, 1, 1)), rtt_ms: Some(0.8), asn: None, as_org: None, country: None, city: None },
                Hop { ttl: 2, addr: None, rtt_ms: None, asn: None, as_org: None, country: None, city: None },
                Hop { ttl: 3, addr: Some(Ipv4Addr::new(219, 141, 136, 12)), rtt_ms: Some(33.2), asn: Some(4809), as_org: Some("Chinanet".into()), country: Some("CN".into()), city: Some("Beijing".into()) },
            ],
            line: LineType::Cn2,
            grade: LineType::Cn2.grade(),
            entry: LineType::Cn2,
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
