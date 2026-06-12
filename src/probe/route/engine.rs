//! P9 traceroute 运行引擎:原生 ICMP socket(libc)+ ip-api 批量逐跳标注。
//!
//! socket I/O 与系统强相关、无法用 mock 单测,故纯逻辑(报文构造/解析/线路识别/渲染)放在 `route.rs` 单测;
//! 本文件只做「真发包」与编排,靠集成运行验证。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use super::{
    build_echo_request, classify_line, is_public_v4, parse_icmp, targets, Carrier, Hop, IcmpKind,
    RouteResult, MAX_HOPS,
};

/// 并发跑三网 traceroute,逐跳标注 AS/geo,识别回程线路类型。
/// 任一目标失败/无特权 → 该条降级标注,不阻塞其余。
pub async fn run_routes(client: &Client, timeout_secs: u64) -> Vec<RouteResult> {
    // 发完全部 TTL 后的收包窗口;China 节点 RTT 较高,留 4-10s
    let window = Duration::from_secs(timeout_secs.clamp(4, 10));

    // 串行跑三条 trace:并发会让内核把 Time Exceeded 广播到多个 ICMP socket,
    // 各 trace 按相同 seq 互相抢收造成串扰(三条路径会混成一样)。串行 + 每条独立
    // seq 段(base = i*64)双重隔离,杜绝跨 trace 与残留在途包混入。
    let mut routes: Vec<RouteResult> = Vec::new();
    let mut to_annotate: HashSet<Ipv4Addr> = HashSet::new();

    for (i, (carrier, name, ip)) in targets().into_iter().enumerate() {
        let seq_base = (i as u16) * 64;
        let w = window;
        let res = match tokio::task::spawn_blocking(move || trace_one(ip, MAX_HOPS, w, seq_base)).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        match res {
            Ok(hops) => {
                for hp in &hops {
                    if let Some(a) = hp.addr {
                        if is_public_v4(a) {
                            to_annotate.insert(a);
                        }
                    }
                }
                routes.push(RouteResult {
                    carrier,
                    target_name: name.to_string(),
                    target: ip,
                    hops,
                    line: super::LineType::Unknown,
                    degraded: None,
                });
            }
            Err(reason) => routes.push(RouteResult {
                carrier,
                target_name: name.to_string(),
                target: ip,
                hops: Vec::new(),
                line: super::LineType::Unknown,
                degraded: Some(reason),
            }),
        }
    }

    // 一次 ip-api /batch 标注所有公网跳
    let geo = annotate(client, &to_annotate).await;
    for r in routes.iter_mut() {
        if r.degraded.is_some() {
            continue;
        }
        let mut asns: Vec<u32> = Vec::new();
        for hp in r.hops.iter_mut() {
            if let Some(a) = hp.addr {
                if let Some(g) = geo.get(&a) {
                    hp.asn = g.asn;
                    hp.as_org = g.as_org.clone();
                    hp.country = g.country.clone();
                    hp.city = g.city.clone();
                    if let Some(n) = g.asn {
                        asns.push(n);
                    }
                }
            }
        }
        r.line = classify_line(r.carrier, &asns);
    }

    // 维持 电信→联通→移动 的稳定展示顺序
    routes.sort_by_key(|r| match r.carrier {
        Carrier::Telecom => 0,
        Carrier::Unicom => 1,
        Carrier::Mobile => 2,
    });
    routes
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

// ───────────────────────── 原生 ICMP traceroute ─────────────────────────

/// 单目标 traceroute:成功返回逐跳(asn/geo 暂空,后续批量填),失败(无特权等)返回降级标记。
#[cfg(unix)]
fn trace_one(target: Ipv4Addr, max_hops: u8, window: Duration, seq_base: u16) -> Result<Vec<Hop>, String> {
    use std::os::raw::c_int;
    use std::time::Instant;

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

        // 每条 trace 用独立 id(防御性;Linux DGRAM 会改写 id,真正隔离靠 seq 段)
        let id = (std::process::id() as u16).wrapping_add(seq_base);
        let dest = make_sockaddr(target);

        // 先把 TTL 1..=max 全部发出(seq = seq_base + ttl,各 trace seq 段互不重叠),按 ttl 记发送时刻
        let mut send_times: BTreeMap<u8, Instant> = BTreeMap::new();
        for ttl in 1..=max_hops {
            let ttl_i = ttl as c_int;
            libc::setsockopt(
                fd,
                libc::IPPROTO_IP,
                libc::IP_TTL,
                &ttl_i as *const _ as *const libc::c_void,
                std::mem::size_of::<c_int>() as libc::socklen_t,
            );
            let pkt = build_echo_request(id, seq_base + ttl as u16, 32);
            libc::sendto(
                fd,
                pkt.as_ptr() as *const libc::c_void,
                pkt.len(),
                0,
                &dest as *const libc::sockaddr_in as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            );
            send_times.insert(ttl, Instant::now());
        }

        // 在 window 内收包,按 ttl 归位;只认本条 trace 的 seq 段
        let deadline = Instant::now() + window;
        let mut hops: BTreeMap<u8, Hop> = BTreeMap::new();
        let mut dest_ttl: Option<u8> = None;
        let mut buf = [0u8; 1500];
        while Instant::now() < deadline {
            let mut from: libc::sockaddr_in = std::mem::zeroed();
            let mut fromlen = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let n = libc::recvfrom(
                fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
                &mut from as *mut libc::sockaddr_in as *mut libc::sockaddr,
                &mut fromlen,
            );
            if n <= 0 {
                continue; // 超时(EAGAIN)/被打断,继续看 deadline
            }
            let Some(info) = parse_icmp(&buf[..n as usize]) else {
                continue;
            };
            // 只接受落在本条 trace seq 段内的回包,别条/残留在途包一律丢弃
            if info.seq <= seq_base || info.seq > seq_base + max_hops as u16 {
                continue;
            }
            let ttl = (info.seq - seq_base) as u8;
            let from_ip = Ipv4Addr::from(u32::from_be(from.sin_addr.s_addr));
            let rtt = send_times
                .get(&ttl)
                .map(|t| t.elapsed().as_secs_f64() * 1000.0);
            match info.kind {
                IcmpKind::EchoReply | IcmpKind::DestUnreachable => {
                    hops.entry(ttl).or_insert_with(|| mk_hop(ttl, from_ip, rtt));
                    dest_ttl = Some(dest_ttl.map_or(ttl, |d| d.min(ttl)));
                }
                IcmpKind::TimeExceeded => {
                    hops.entry(ttl).or_insert_with(|| mk_hop(ttl, from_ip, rtt));
                }
                IcmpKind::Other => {}
            }
            // 已到目标且其之前所有跳都已收齐 → 提前结束
            if let Some(dt) = dest_ttl {
                if (1..=dt).all(|s| hops.contains_key(&s)) {
                    break;
                }
            }
        }
        libc::close(fd);

        let max_ttl = dest_ttl.unwrap_or(max_hops);
        let mut out: Vec<Hop> = (1..=max_ttl)
            .map(|s| hops.get(&s).cloned().unwrap_or_else(|| empty_hop(s)))
            .collect();
        // 截掉最后一个有应答跳之后的连续无应答跳,避免一长串 *
        match out.iter().rposition(|h| h.addr.is_some()) {
            Some(last) => out.truncate(last + 1),
            None => out.clear(),
        }
        Ok(out)
    }
}

#[cfg(not(unix))]
fn trace_one(_target: Ipv4Addr, _max_hops: u8, _window: Duration, _seq_base: u16) -> Result<Vec<Hop>, String> {
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
