//! P10 traceroute 运行引擎:单 ICMP socket 并行(libc)+ ip-api 批量逐跳标注。
//!
//! P9 时三条 trace 各开一个 socket、串行跑(并发会让内核把 Time Exceeded 广播到多个 ICMP
//! socket,按相同 seq 互相抢收造成串扰)。P10 扩到 12 目标(三网×四城),串行会慢到 12×window。
//! 改为「单 socket + 每目标独立 seq 段」:只有一个 ICMP socket(无跨 socket 串扰),12 条 trace
//! 的探测包一次性全发出,回包按 seq 段归位到各自目标 → 总耗时压到约 1 个 window。
//!
//! socket I/O 与系统强相关、无法用 mock 单测,故纯逻辑(报文构造/解析/线路识别/渲染)放在 `route.rs`
//! 单测;本文件只做「真发包」与编排,靠集成运行验证。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use super::{
    asn_from_prefix, build_echo_request, classify_entry, classify_line, is_public_v4, parse_icmp,
    refine_cn2, targets, Carrier, Hop, IcmpKind, RouteResult, MAX_HOPS,
};

/// 每目标分到的 seq 段宽度(须 ≥ MAX_HOPS;段 base = idx*SEQ_STRIDE,互不重叠)。
const SEQ_STRIDE: u16 = 64;

/// 单 socket 并行跑三网×四城 traceroute,逐跳标注 AS/geo,识别回程线路类型。
/// socket 打开失败(无特权)→ 全部目标降级标注,不阻塞其余功能。
pub async fn run_routes(client: &Client, timeout_secs: u64) -> Vec<RouteResult> {
    let tgts = targets();
    let ips: Vec<Ipv4Addr> = tgts.iter().map(|(_, _, _, ip)| *ip).collect();

    // 发完全部 TTL 后的统一收包窗口;目标多、China 节点 RTT 高,留 6-12s。
    let window = Duration::from_secs(timeout_secs.clamp(6, 12));

    let traced = match tokio::task::spawn_blocking(move || trace_all(&ips, MAX_HOPS, window)).await {
        Ok(r) => r,
        Err(_) => Err("need_privilege".to_string()),
    };

    let mut routes: Vec<RouteResult> = Vec::with_capacity(tgts.len());
    let mut to_annotate: HashSet<Ipv4Addr> = HashSet::new();

    match traced {
        Ok(per_target) => {
            for ((carrier, city, name, ip), hops) in tgts.iter().zip(per_target) {
                for hp in &hops {
                    if let Some(a) = hp.addr {
                        if is_public_v4(a) {
                            to_annotate.insert(a);
                        }
                    }
                }
                routes.push(RouteResult {
                    carrier: *carrier,
                    city: *city,
                    target_name: name.to_string(),
                    target: *ip,
                    hops,
                    line: super::LineType::Unknown,
                    entry: super::LineType::Unknown,
                    degraded: None,
                });
            }
        }
        Err(reason) => {
            // socket 打开失败:整批降级
            for (carrier, city, name, ip) in tgts.iter() {
                routes.push(RouteResult {
                    carrier: *carrier,
                    city: *city,
                    target_name: name.to_string(),
                    target: *ip,
                    hops: Vec::new(),
                    line: super::LineType::Unknown,
                    entry: super::LineType::Unknown,
                    degraded: Some(reason.clone()),
                });
            }
        }
    }

    // 一次 ip-api /batch 标注所有公网跳
    let geo = annotate(client, &to_annotate).await;
    for r in routes.iter_mut() {
        if r.degraded.is_some() {
            continue;
        }
        let mut asns: Vec<u32> = Vec::new();
        let mut ips_in_path: Vec<Ipv4Addr> = Vec::new();
        for hp in r.hops.iter_mut() {
            if let Some(a) = hp.addr {
                ips_in_path.push(a);
                if let Some(g) = geo.get(&a) {
                    hp.asn = g.asn;
                    hp.as_org = g.as_org.clone();
                    hp.country = g.country.clone();
                    hp.city = g.city.clone();
                }
                // ip-api 未给出 AS 号时,用前缀启发式兜底(不覆盖已有结果)
                if hp.asn.is_none() {
                    hp.asn = asn_from_prefix(a);
                }
                if let Some(n) = hp.asn {
                    asns.push(n);
                }
            }
        }
        // 回程线路 + 国际入境线,再对 CN2 做 GIA/GT 细分
        r.line = refine_cn2(classify_line(r.carrier, &asns), &ips_in_path);
        r.entry = refine_cn2(classify_entry(&asns), &ips_in_path);
    }

    // 稳定展示顺序:电信→联通→移动,同运营商内 北京→上海→广州→成都
    routes.sort_by_key(|r| (carrier_order(r.carrier), r.city.order()));
    routes
}

fn carrier_order(c: Carrier) -> u8 {
    match c {
        Carrier::Telecom => 0,
        Carrier::Unicom => 1,
        Carrier::Mobile => 2,
    }
}

// ───────────────────────── ip-api 批量标注 ─────────────────────────

struct Geo {
    asn: Option<u32>,
    as_org: Option<String>,
    country: Option<String>,
    city: Option<String>,
}

#[derive(Deserialize)]
struct BatchItem {
    status: Option<String>,
    query: Option<String>,
    #[serde(rename = "as")]
    as_field: Option<String>,
    country: Option<String>,
    city: Option<String>,
}

async fn annotate(client: &Client, ips: &HashSet<Ipv4Addr>) -> HashMap<Ipv4Addr, Geo> {
    let mut map = HashMap::new();
    if ips.is_empty() {
        return map;
    }
    let list: Vec<String> = ips.iter().map(|i| i.to_string()).collect();
    let url = "http://ip-api.com/batch?fields=status,query,as,country,city";
    let items: Vec<BatchItem> = match client.post(url).json(&list).send().await {
        Ok(resp) => match resp.json().await {
            Ok(v) => v,
            Err(_) => return map,
        },
        Err(_) => return map,
    };
    for it in items {
        if it.status.as_deref() != Some("success") {
            continue;
        }
        let Some(q) = it.query.as_deref().and_then(|q| q.parse::<Ipv4Addr>().ok()) else {
            continue;
        };
        let (asn, as_org) = it
            .as_field
            .as_deref()
            .map(crate::sources::ipapi::split_as)
            .unwrap_or((None, None));
        map.insert(
            q,
            Geo {
                asn,
                as_org,
                country: it.country,
                city: it.city,
            },
        );
    }
    map
}

// ───────────────────────── 原生 ICMP traceroute(单 socket 并行) ─────────────────────────

/// 一次性对 N 个目标各发 1..=max_hops 个探测包(单 socket,seq = idx*SEQ_STRIDE + ttl),
/// 在统一 window 内收包按 seq 段归位。成功返回每目标的逐跳(asn/geo 暂空,后续批量填);
/// socket 打开失败(无特权)→ 返回降级标记,整批降级。
#[cfg(unix)]
fn trace_all(targets: &[Ipv4Addr], max_hops: u8, window: Duration) -> Result<Vec<Vec<Hop>>, String> {
    use std::os::raw::c_int;
    use std::time::Instant;

    let n = targets.len();
    unsafe {
        let fd = open_icmp_socket()?;

        // 收包超时 200ms,便于在 window 内多次轮询
        let tv = libc::timeval {
            tv_sec: 0,
            tv_usec: 200_000,
        };
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );

        let id = std::process::id() as u16;

        // 把 N×max_hops 个探测包全部发出。seq = idx*SEQ_STRIDE + ttl,各目标 seq 段互不重叠。
        // 按 seq 记发送时刻,用于算 RTT,也作「这是本次发出的包」的白名单。
        let mut send_times: HashMap<u16, Instant> = HashMap::new();
        for (idx, &target) in targets.iter().enumerate() {
            let dest = make_sockaddr(target);
            let base = (idx as u16) * SEQ_STRIDE;
            for ttl in 1..=max_hops {
                let ttl_i = ttl as c_int;
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_TTL,
                    &ttl_i as *const _ as *const libc::c_void,
                    std::mem::size_of::<c_int>() as libc::socklen_t,
                );
                let seq = base + ttl as u16;
                let pkt = build_echo_request(id, seq, 32);
                libc::sendto(
                    fd,
                    pkt.as_ptr() as *const libc::c_void,
                    pkt.len(),
                    0,
                    &dest as *const libc::sockaddr_in as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                );
                send_times.insert(seq, Instant::now());
            }
        }

        // 统一 window 内收包,按 seq → (目标 idx, ttl) 归位
        let deadline = Instant::now() + window;
        let mut hops_per: Vec<BTreeMap<u8, Hop>> = vec![BTreeMap::new(); n];
        let mut dest_ttl_per: Vec<Option<u8>> = vec![None; n];
        let mut done: HashSet<usize> = HashSet::new();
        let mut buf = [0u8; 1500];
        while Instant::now() < deadline {
            let mut from: libc::sockaddr_in = std::mem::zeroed();
            let mut fromlen = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let nbytes = libc::recvfrom(
                fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
                &mut from as *mut libc::sockaddr_in as *mut libc::sockaddr,
                &mut fromlen,
            );
            if nbytes <= 0 {
                continue; // 超时(EAGAIN)/被打断,继续看 deadline
            }
            let Some(info) = parse_icmp(&buf[..nbytes as usize]) else {
                continue;
            };
            // 只认本次发出的 seq(白名单),别条/残留在途包一律丢弃
            let Some(sent_at) = send_times.get(&info.seq) else {
                continue;
            };
            let idx = (info.seq / SEQ_STRIDE) as usize;
            let ttl = (info.seq % SEQ_STRIDE) as u8;
            if idx >= n || ttl == 0 || ttl > max_hops {
                continue;
            }
            let from_ip = Ipv4Addr::from(u32::from_be(from.sin_addr.s_addr));
            let rtt = Some(sent_at.elapsed().as_secs_f64() * 1000.0);
            match info.kind {
                IcmpKind::EchoReply | IcmpKind::DestUnreachable => {
                    hops_per[idx]
                        .entry(ttl)
                        .or_insert_with(|| mk_hop(ttl, from_ip, rtt));
                    dest_ttl_per[idx] = Some(dest_ttl_per[idx].map_or(ttl, |d| d.min(ttl)));
                }
                IcmpKind::TimeExceeded => {
                    hops_per[idx]
                        .entry(ttl)
                        .or_insert_with(|| mk_hop(ttl, from_ip, rtt));
                }
                IcmpKind::Other => {}
            }
            // 该目标已到终点且其前所有跳都收齐 → 标记完成;全部完成则提前结束
            if let Some(dt) = dest_ttl_per[idx] {
                if (1..=dt).all(|s| hops_per[idx].contains_key(&s)) {
                    done.insert(idx);
                    if done.len() == n {
                        break;
                    }
                }
            }
        }
        libc::close(fd);

        // 各目标:补齐缺跳为 *,截掉最后一个有应答跳之后的连续无应答
        let mut out: Vec<Vec<Hop>> = Vec::with_capacity(n);
        for idx in 0..n {
            let max_ttl = dest_ttl_per[idx].unwrap_or(max_hops);
            let mut hops: Vec<Hop> = (1..=max_ttl)
                .map(|s| hops_per[idx].get(&s).cloned().unwrap_or_else(|| empty_hop(s)))
                .collect();
            match hops.iter().rposition(|h| h.addr.is_some()) {
                Some(last) => hops.truncate(last + 1),
                None => hops.clear(),
            }
            out.push(hops);
        }
        Ok(out)
    }
}

#[cfg(not(unix))]
fn trace_all(_targets: &[Ipv4Addr], _max_hops: u8, _window: Duration) -> Result<Vec<Vec<Hop>>, String> {
    Err("need_privilege".to_string())
}

#[cfg(unix)]
fn mk_hop(ttl: u8, addr: Ipv4Addr, rtt_ms: Option<f64>) -> Hop {
    Hop {
        ttl,
        addr: Some(addr),
        rtt_ms,
        asn: None,
        as_org: None,
        country: None,
        city: None,
    }
}

#[cfg(unix)]
fn empty_hop(ttl: u8) -> Hop {
    Hop {
        ttl,
        addr: None,
        rtt_ms: None,
        asn: None,
        as_org: None,
        country: None,
        city: None,
    }
}

/// 优先 DGRAM(免特权),回退 RAW(需 root);皆失败 → 降级标记 "need_privilege"
#[cfg(unix)]
unsafe fn open_icmp_socket() -> Result<std::os::raw::c_int, String> {
    let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, libc::IPPROTO_ICMP);
    if fd >= 0 {
        return Ok(fd);
    }
    let fd = libc::socket(libc::AF_INET, libc::SOCK_RAW, libc::IPPROTO_ICMP);
    if fd >= 0 {
        return Ok(fd);
    }
    Err("need_privilege".to_string())
}

#[cfg(unix)]
unsafe fn make_sockaddr(ip: Ipv4Addr) -> libc::sockaddr_in {
    let mut sa: libc::sockaddr_in = std::mem::zeroed();
    sa.sin_family = libc::AF_INET as libc::sa_family_t;
    sa.sin_port = 0;
    sa.sin_addr = libc::in_addr {
        s_addr: u32::from(ip).to_be(),
    };
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        sa.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
    }
    sa
}
