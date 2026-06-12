use std::net::{IpAddr, Ipv4Addr};
use futures::future::join_all;
use serde::Serialize;
use tokio::time::{timeout, Duration};

/// 检测使用的 DNSBL 列表(12 个主流邮件/滥用黑名单)
static DNSBL_LISTS: &[&str] = &[
    "zen.spamhaus.org",         // Spamhaus ZEN — 业界最权威
    "bl.spamcop.net",           // SpamCop BL
    "b.barracudacentral.org",   // Barracuda Reputation Block List
    "cbl.abuseat.org",          // Composite Blocking List
    "dnsbl.sorbs.net",          // SORBS 综合
    "spam.dnsbl.sorbs.net",     // SORBS Spam
    "dnsbl-1.uceprotect.net",   // UCEPROTECT Level 1
    "dnsbl-2.uceprotect.net",   // UCEPROTECT Level 2
    "dnsbl.dronebl.org",        // DroneBL
    "psbl.surriel.com",         // Passive Spam Block List
    "bl.0spam.org",             // 0Spam
    "ips.backscatterer.org",    // Backscatterer
];

#[derive(Debug, Clone, Serialize)]
pub struct DnsblResult {
    pub list: String,
    pub listed: bool,
}

/// 将 IPv4 地址反转用于 DNSBL 查询(1.2.3.4 → "4.3.2.1")
pub fn reverse_ipv4(ip: Ipv4Addr) -> String {
    let o = ip.octets();
    format!("{}.{}.{}.{}", o[3], o[2], o[1], o[0])
}

/// DNSBL 命中的判定:返回的 A 记录必须落在 127.0.0.0/8。
/// 标准约定:命中返回 127.0.0.x;未命中返回 NXDOMAIN。
/// 仅检查「能否解析」会被 ISP 的 NXDOMAIN 劫持(返回门户 IP)误判为全部命中。
fn is_listed_addr(addr: IpAddr) -> bool {
    matches!(addr, IpAddr::V4(v4) if v4.octets()[0] == 127)
}

/// 检查 IP 是否在单个 DNSBL 列表中(DNS 查询 4s 超时)
async fn check_one(reversed: &str, list: &'static str) -> DnsblResult {
    let host = format!("{}.{}:0", reversed, list);
    // 命中 = 解析成功且至少一条 A 记录在 127.0.0.0/8;NXDOMAIN/超时/劫持 = 未命中
    let listed = match timeout(Duration::from_secs(4), tokio::net::lookup_host(&host)).await {
        Ok(Ok(addrs)) => addrs.map(|sa| sa.ip()).any(is_listed_addr),
        _ => false,
    };
    DnsblResult { list: list.to_string(), listed }
}

/// 并发检查 IPv4 对所有 DNSBL 列表的命中情况
pub async fn check_all(ip: Ipv4Addr) -> Vec<DnsblResult> {
    let reversed = reverse_ipv4(ip);
    join_all(DNSBL_LISTS.iter().map(|&list| check_one(&reversed, list))).await
}

/// 终端渲染(comfy-table 包边表;命中/清白着色,no_color 时退化为纯文本)
pub fn render_terminal(results: &[DnsblResult], ip: &str, lang: crate::i18n::Lang, no_color: bool) -> String {
    use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
    let listed_count = results.iter().filter(|r| r.listed).count();
    let mut out = format!("═══ {} {} ═══\n",
        lang.pick("DNSBL 黑名单检测", "DNSBL reputation check"), ip);
    // 汇总:有命中红,全清白绿
    let summary = format!("{}: {}/{}",
        lang.pick("命中列表数", "Listed in"), listed_count, results.len());
    if no_color {
        out.push_str(&summary);
    } else {
        use owo_colors::OwoColorize;
        if listed_count > 0 {
            out.push_str(&summary.red().bold().to_string());
        } else {
            out.push_str(&summary.green().bold().to_string());
        }
    }
    out.push('\n');

    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(vec![lang.pick("黑名单", "Blocklist"), lang.pick("状态", "Status")]);
    for r in results {
        let label = if r.listed { lang.pick("✗ 命中", "✗ Listed") } else { lang.pick("✓ 清白", "✓ Clean") };
        let status = Cell::new(label);
        let status = match (no_color, r.listed) {
            (false, true) => status.fg(Color::Red),
            (false, false) => status.fg(Color::Green),
            _ => status,
        };
        t.add_row(vec![Cell::new(&r.list), status]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

/// Markdown 渲染(pipe 表)
pub fn render_section(results: &[DnsblResult], ip: &str, lang: crate::i18n::Lang) -> String {
    use std::fmt::Write;
    let listed_count = results.iter().filter(|r| r.listed).count();
    let mut out = String::new();
    writeln!(out, "## {} {}\n", lang.pick("DNSBL 黑名单检测", "DNSBL reputation check"), ip).ok();
    writeln!(out, "{}: {}/{}\n", lang.pick("命中列表数", "Listed"), listed_count, results.len()).ok();
    writeln!(out, "| {} | {} |", lang.pick("黑名单", "Blocklist"), lang.pick("状态", "Status")).ok();
    writeln!(out, "|---|---|").ok();
    for r in results {
        let status = if r.listed { lang.pick("✗ 命中", "✗ Listed") } else { lang.pick("✓ 清白", "✓ Clean") };
        writeln!(out, "| {} | {} |", r.list, status).ok();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_ipv4_standard() {
        let ip: Ipv4Addr = "1.2.3.4".parse().unwrap();
        assert_eq!(reverse_ipv4(ip), "4.3.2.1");
    }

    #[test]
    fn reverse_ipv4_all_same() {
        let ip: Ipv4Addr = "10.10.10.10".parse().unwrap();
        assert_eq!(reverse_ipv4(ip), "10.10.10.10");
    }

    #[test]
    fn reverse_ipv4_real() {
        // 8.8.8.8 → 8.8.8.8 (对称)
        let ip: Ipv4Addr = "8.8.8.8".parse().unwrap();
        assert_eq!(reverse_ipv4(ip), "8.8.8.8");
        // 192.168.1.100 → 100.1.168.192
        let ip2: Ipv4Addr = "192.168.1.100".parse().unwrap();
        assert_eq!(reverse_ipv4(ip2), "100.1.168.192");
    }

    #[test]
    fn is_listed_addr_accepts_127() {
        assert!(is_listed_addr("127.0.0.2".parse().unwrap()));
        assert!(is_listed_addr("127.0.0.10".parse().unwrap()));
    }

    #[test]
    fn is_listed_addr_rejects_non_127() {
        // ISP NXDOMAIN 劫持常返回门户 IP(非 127 段)→ 不算命中
        assert!(!is_listed_addr("1.2.3.4".parse().unwrap()));
        assert!(!is_listed_addr("198.51.100.1".parse().unwrap()));
        // IPv6 一律不算命中
        assert!(!is_listed_addr("::1".parse().unwrap()));
    }

    #[test]
    fn dnsbl_result_serializes() {
        let r = DnsblResult { list: "zen.spamhaus.org".into(), listed: false };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("zen.spamhaus.org"));
        assert!(json.contains("false"));
    }

    #[test]
    fn render_terminal_shows_summary() {
        let results = vec![
            DnsblResult { list: "zen.spamhaus.org".into(), listed: false },
            DnsblResult { list: "bl.spamcop.net".into(), listed: true },
        ];
        let out = render_terminal(&results, "1.2.3.4", crate::i18n::Lang::Zh, true);
        assert!(out.contains("1/2"));
        assert!(out.contains("zen.spamhaus.org"));
        assert!(out.contains("命中"));
    }

    #[test]
    fn render_section_markdown() {
        let results = vec![
            DnsblResult { list: "zen.spamhaus.org".into(), listed: false },
        ];
        let out = render_section(&results, "8.8.8.8", crate::i18n::Lang::En);
        assert!(out.contains("8.8.8.8"));
        assert!(out.contains("0/1"));
        assert!(out.contains("zen.spamhaus.org"));
    }

    #[test]
    fn dnsbl_lists_count() {
        assert_eq!(DNSBL_LISTS.len(), 12);
    }
}
