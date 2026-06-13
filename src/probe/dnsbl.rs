use std::net::{IpAddr, Ipv4Addr};
use futures::future::join_all;
use serde::Serialize;
use tokio::time::{timeout, Duration};

/// 检测使用的 DNSBL 列表(211 个 IP 邮件/滥用黑名单)。
/// 来源:fnando/email_data `data/manual/dnsbls.txt`(2026-06-13 快照),
/// 已剔除针对域名的 RHS/URI/DBL 黑名单(对反转 IP 查询无意义)。
/// 运行期每条 4s 超时、全局并发;无响应/NXDOMAIN/被劫持均视为未命中。
static DNSBL_LISTS: &[&str] = &[
    "0spam-n.fusionzero.com",
    "0spam.fusionzero.com",
    "0spamurl.fusionzero.com",
    "access.redhawk.org",
    "all.s5h.net",
    "all.spamrats.com",
    "aspews.ext.sorbs.net",
    "auth.spamrats.com",
    "b.barracudacentral.org",
    "backscatter.spameatingmonkey.net",
    "bad.virusfree.cz",
    "bb.barracudacentral.org",
    "bip.virusfree.cz",
    "bl-h1.rbl.polspam.pl",
    "bl-h2.rbl.polspam.pl",
    "bl-h3.rbl.polspam.pl",
    "bl-h4.rbl.polspam.pl",
    "bl.0spam.org",
    "bl.blocklist.de",
    "bl.drmx.org",
    "bl.fmb.la",
    "bl.ipv6.spameatingmonkey.net",
    "bl.konstant.no",
    "bl.mailspike.net",
    "bl.mav.com.br",
    "bl.nordspam.com",
    "bl.nosolicitado.org",
    "bl.nszones.com",
    "bl.octopusdns.com",
    "bl.rbl.polspam.pl",
    "bl.rbl.scrolloutf1.com",
    "bl.scientificspam.net",
    "bl.score.senderscore.com",
    "bl.spamcop.net",
    "bl.spameatingmonkey.net",
    "bl.suomispam.net",
    "bl.worst.nosolicitado.org",
    "bl6.rbl.polspam.pl",
    "black.dnsbl.brukalai.lt",
    "black.junkemailfilter.com",
    "black.mail.abusix.zone",
    "blackholes.mail-abuse.org",
    "blacklist.netcore.co.in",
    "blacklist.sci.kun.nl",
    "blacklist.woody.ch",
    "block.ascams.com",
    "block.dnsbl.sorbs.net",
    "bogons.cymru.com",
    "bsb.spamlookup.net",
    "cart00ney.surriel.com",
    "cbl.abuseat.org",
    "cbl.anti-spam.org.cn",
    "cdl.anti-spam.org.cn",
    "cidr.bl.mcafee.com",
    "cnkr.rbl.polspam.pl",
    "combined.rbl.msrbl.net",
    "db.wpbl.info",
    "dnsbl-0.uceprotect.net",
    "dnsbl-1.uceprotect.net",
    "dnsbl-2.uceprotect.net",
    "dnsbl-3.uceprotect.net",
    "dnsbl.anticaptcha.net",
    "dnsbl.ascams.com",
    "dnsbl.beetjevreemd.nl",
    "dnsbl.calivent.com.pe",
    "dnsbl.cobion.com",
    "dnsbl.darklist.de",
    "dnsbl.dronebl.org",
    "dnsbl.inps.de",
    "dnsbl.isx.fr",
    "dnsbl.justspam.org",
    "dnsbl.kempt.net",
    "dnsbl.madavi.de",
    "dnsbl.net.ua",
    "dnsbl.rv-soft.info",
    "dnsbl.rymsho.ru",
    "dnsbl.sorbs.net",
    "dnsbl.spfbl.net",
    "dnsbl.tornevall.org",
    "dnsbl.zapbl.net",
    "dnsbl6.anticaptcha.net",
    "dnsblchile.org",
    "dnsrbl.swinog.ch",
    "drone.abuse.ch",
    "dsn.rfc-ignorant.org",
    "dul.dnsbl.sorbs.net",
    "dyn.nszones.com",
    "dyn.rbl.polspam.pl",
    "dyna.spamrats.com",
    "escalations.dnsbl.sorbs.net",
    "exploit.mail.abusix.zone",
    "fnrbl.fast.net",
    "forbidden.icm.edu.pl",
    "free.v4bl.org",
    "gl.suomispam.net",
    "hil.habeas.com",
    "hostkarma.junkemailfilter.com",
    "http.dnsbl.sorbs.net",
    "httpbl.abuse.ch",
    "images.rbl.msrbl.net",
    "ip.v4bl.org",
    "ip4.bl.zenrbl.pl",
    "iprbl.mailcleaner.net",
    "ips.backscatterer.org",
    "ipv6.blacklist.woody.ch",
    "ix.dnsbl.manitu.net",
    "korea.services.net",
    "l1.bbfh.ext.sorbs.net",
    "l2.bbfh.ext.sorbs.net",
    "l3.bbfh.ext.sorbs.net",
    "l4.bbfh.ext.sorbs.net",
    "lblip4.rbl.polspam.pl",
    "lblip6.rbl.polspam.pl",
    "light.dnsbl.brukalai.lt",
    "list.bbfh.org",
    "list.blogspambl.com",
    "mail-abuse.blacklist.jippg.org",
    "misc.dnsbl.sorbs.net",
    "nbl.0spam.org",
    "netbl.spameatingmonkey.net",
    "netblock.pedantic.org",
    "netblockbl.spamgrouper.to",
    "netscan.rbl.blockedservers.com",
    "new.spam.dnsbl.sorbs.net",
    "niprbl.mailcleaner.net",
    "noptr.spamrats.com",
    "nsbl.fmb.la",
    "old.spam.dnsbl.sorbs.net",
    "openproxy.bls.digibase.ca",
    "opm.tornevall.org",
    "orvedb.aupads.org",
    "pbl.spamhaus.org",
    "phishing.rbl.msrbl.net",
    "pofon.foobar.hu",
    "problems.dnsbl.sorbs.net",
    "proxies.dnsbl.sorbs.net",
    "proxyabuse.bls.digibase.ca",
    "psbl.surriel.com",
    "rbl-plus.mail-abuse.org",
    "rbl.0spam.org",
    "rbl.abuse.ro",
    "rbl.blockedservers.com",
    "rbl.dns-servicios.com",
    "rbl.efnet.org",
    "rbl.efnetrbl.org",
    "rbl.fasthosts.co.uk",
    "rbl.interserver.net",
    "rbl.iprange.net",
    "rbl.ircbl.org",
    "rbl.lugh.ch",
    "rbl.metunet.com",
    "rbl.rbldns.ru",
    "rbl.realtimeblacklist.com",
    "rbl.schulte.org",
    "rbl.spamlab.com",
    "rbl.suresupport.com",
    "rbl2.triumf.ca",
    "rblip4.rbl.polspam.pl",
    "rblip6.rbl.polspam.pl",
    "recent.spam.dnsbl.sorbs.net",
    "relays.bl.kundenserver.de",
    "relays.dnsbl.sorbs.net",
    "relays.mail-abuse.org",
    "relays.nether.net",
    "rep.mailspike.net",
    "rsbl.aupads.org",
    "safe.dnsbl.sorbs.net",
    "sbl-xbl.spamhaus.org",
    "sbl.nszones.com",
    "sbl.spamdown.org",
    "sbl.spamhaus.org",
    "short.fmb.la",
    "short.rbl.jp",
    "singular.ttk.pte.hu",
    "smtp.dnsbl.sorbs.net",
    "socks.dnsbl.sorbs.net",
    "spam.dnsbl.anonmails.de",
    "spam.dnsbl.sorbs.net",
    "spam.pedantic.org",
    "spam.rbl.blockedservers.com",
    "spam.rbl.msrbl.net",
    "spam.spamrats.com",
    "spambot.bls.digibase.ca",
    "spamguard.leadmon.net",
    "spamlist.or.kr",
    "spamrbl.imp.ch",
    "spamsources.fabel.dk",
    "st.technovision.dk",
    "superblock.ascams.com",
    "tor.dan.me.uk",
    "tor.efnet.org",
    "torexit.dan.me.uk",
    "truncate.gbudb.net",
    "ubl.unsubscore.com",
    "unsure.nether.net",
    "v4.fullbogons.cymru.com",
    "v6.fullbogons.cymru.com",
    "virbl.bit.nl",
    "virus.rbl.jp",
    "virus.rbl.msrbl.net",
    "vote.drbl.caravan.ru",
    "vote.drbl.gremlin.ru",
    "web.dnsbl.sorbs.net",
    "web.rbl.msrbl.net",
    "work.drbl.caravan.ru",
    "work.drbl.gremlin.ru",
    "wormrbl.imp.ch",
    "xbl.spamhaus.org",
    "z.mailspike.net",
    "zen.spamhaus.org",
    "zombie.dnsbl.sorbs.net",
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
    fn dnsbl_list_is_large_and_unique() {
        assert!(DNSBL_LISTS.len() >= 200, "DNSBL 列表应 >= 200, got {}", DNSBL_LISTS.len());
        let mut v: Vec<&str> = DNSBL_LISTS.to_vec();
        let n = v.len(); v.sort_unstable(); v.dedup();
        assert_eq!(v.len(), n, "DNSBL 列表不应有重复");
    }
}
