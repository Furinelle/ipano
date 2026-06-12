//! P12 邮件端口连通性:6 协议 × 多家邮局矩阵。
//!
//! 从本机出口对各邮局的 SMTP/SMTPS/POP3/POP3S/IMAP/IMAPS 端口做 TCP 连通探测,
//! 一眼看出本机出站哪些邮件端口可达(VPS 25 端口常被封)。纯 TCP 连接,不收发邮件。

use std::time::Duration;
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::time::timeout;
use futures::future::join_all;
use comfy_table::{presets::UTF8_FULL, Table};
use crate::i18n::Lang;

/// 协议族:决定用某邮局的哪个主机(smtp./pop./imap.)
#[derive(Clone, Copy)]
enum Fam {
    Smtp,
    Pop,
    Imap,
}

/// 6 个协议列:(列名, 端口, 协议族)
const PROTOCOLS: [(&str, u16, Fam); 6] = [
    ("SMTP", 25, Fam::Smtp),
    ("SMTPS", 465, Fam::Smtp),
    ("POP3", 110, Fam::Pop),
    ("POP3S", 995, Fam::Pop),
    ("IMAP", 143, Fam::Imap),
    ("IMAPS", 993, Fam::Imap),
];

#[derive(Debug, Clone, Serialize)]
pub struct ProtoStatus {
    pub proto: String,
    pub port: u16,
    /// None = 该邮局不提供此协议(不适用);Some = 探测结果
    pub open: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MailResult {
    pub provider: String,
    pub protocols: Vec<ProtoStatus>,
}

/// 邮局主机表:某协议主机为 None 表示该邮局不提供该协议
struct Provider {
    name: &'static str,
    smtp: Option<&'static str>,
    pop: Option<&'static str>,
    imap: Option<&'static str>,
}

fn provider(
    name: &'static str,
    smtp: Option<&'static str>,
    pop: Option<&'static str>,
    imap: Option<&'static str>,
) -> Provider {
    Provider { name, smtp, pop, imap }
}

fn providers() -> Vec<Provider> {
    vec![
        provider("Gmail", Some("smtp.gmail.com"), Some("pop.gmail.com"), Some("imap.gmail.com")),
        provider("Outlook", Some("smtp-mail.outlook.com"), Some("outlook.office365.com"), Some("outlook.office365.com")),
        provider("Office365", Some("smtp.office365.com"), Some("outlook.office365.com"), Some("outlook.office365.com")),
        provider("Yahoo", Some("smtp.mail.yahoo.com"), Some("pop.mail.yahoo.com"), Some("imap.mail.yahoo.com")),
        provider("Apple", Some("smtp.mail.me.com"), None, Some("imap.mail.me.com")),
        provider("QQ", Some("smtp.qq.com"), Some("pop.qq.com"), Some("imap.qq.com")),
        provider("163", Some("smtp.163.com"), Some("pop.163.com"), Some("imap.163.com")),
        provider("Sina", Some("smtp.sina.com"), Some("pop.sina.com"), Some("imap.sina.com")),
        provider("Sohu", Some("smtp.sohu.com"), Some("pop3.sohu.com"), Some("imap.sohu.com")),
        provider("Yandex", Some("smtp.yandex.com"), Some("pop.yandex.com"), Some("imap.yandex.com")),
        provider("Zoho", Some("smtp.zoho.com"), Some("pop.zoho.com"), Some("imap.zoho.com")),
        provider("GMX", Some("mail.gmx.com"), Some("pop.gmx.com"), Some("imap.gmx.com")),
        provider("MailRU", Some("smtp.mail.ru"), Some("pop.mail.ru"), Some("imap.mail.ru")),
        provider("AOL", Some("smtp.aol.com"), Some("pop.aol.com"), Some("imap.aol.com")),
        provider("FastMail", Some("smtp.fastmail.com"), Some("pop.fastmail.com"), Some("imap.fastmail.com")),
    ]
}

/// TCP 连接探测:能在超时内建连即视为端口开放
pub async fn check_port(host: &str, port: u16, timeout_secs: u64) -> bool {
    let addr = format!("{}:{}", host, port);
    matches!(
        timeout(Duration::from_secs(timeout_secs), TcpStream::connect(&addr)).await,
        Ok(Ok(_))
    )
}

/// 并发探测各邮局各协议端口连通性
pub async fn check_all(timeout_secs: u64) -> Vec<MailResult> {
    let futs = providers().into_iter().map(|prov| async move {
        let proto_futs = PROTOCOLS.iter().map(|&(name, port, fam)| {
            let host = match fam {
                Fam::Smtp => prov.smtp,
                Fam::Pop => prov.pop,
                Fam::Imap => prov.imap,
            };
            async move {
                let open = match host {
                    Some(h) => Some(check_port(h, port, timeout_secs).await),
                    None => None,
                };
                ProtoStatus { proto: name.to_string(), port, open }
            }
        });
        let protocols = join_all(proto_futs).await;
        MailResult { provider: prov.name.to_string(), protocols }
    });
    join_all(futs).await
}

fn cell(r: &MailResult, proto: &str) -> &'static str {
    match r.protocols.iter().find(|p| p.proto == proto).and_then(|p| p.open) {
        Some(true) => "✔",
        Some(false) => "✘",
        None => "—",
    }
}

/// 终端渲染(comfy-table 包边矩阵,与主报告/route 一致)
pub fn render_terminal(results: &[MailResult], lang: Lang) -> String {
    let mut out = String::new();
    out.push_str(&format!("═══ {} ═══\n",
        lang.pick("邮件端口连通性", "Mail port connectivity")));
    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    let mut header = vec![lang.pick("邮局", "Provider").to_string()];
    for (n, _, _) in PROTOCOLS {
        header.push(n.to_string());
    }
    t.set_header(header);
    for r in results {
        let mut row = vec![r.provider.clone()];
        for (n, _, _) in PROTOCOLS {
            row.push(cell(r, n).to_string());
        }
        t.add_row(row);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

/// Markdown 渲染(pipe 表,--markdown 用)
pub fn render_section(results: &[MailResult], lang: Lang) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "## {}\n", lang.pick("邮件端口连通性", "Mail port connectivity")).ok();
    write!(out, "| {} ", lang.pick("邮局", "Provider")).ok();
    for (n, _, _) in PROTOCOLS {
        write!(out, "| {} ", n).ok();
    }
    writeln!(out, "|").ok();
    writeln!(out, "|---|---|---|---|---|---|---|").ok();
    for r in results {
        write!(out, "| {} ", r.provider).ok();
        for (n, _, _) in PROTOCOLS {
            write!(out, "| {} ", cell(r, n)).ok();
        }
        writeln!(out, "|").ok();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn open_port_connects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(check_port("127.0.0.1", port, 3).await);
    }

    #[tokio::test]
    async fn closed_port_fails() {
        assert!(!check_port("127.0.0.1", 1, 2).await);
    }

    fn sample() -> Vec<MailResult> {
        vec![MailResult {
            provider: "Gmail".into(),
            protocols: vec![
                ProtoStatus { proto: "SMTP".into(), port: 25, open: Some(false) },
                ProtoStatus { proto: "SMTPS".into(), port: 465, open: Some(true) },
                ProtoStatus { proto: "POP3".into(), port: 110, open: Some(false) },
                ProtoStatus { proto: "POP3S".into(), port: 995, open: Some(true) },
                ProtoStatus { proto: "IMAP".into(), port: 143, open: Some(false) },
                ProtoStatus { proto: "IMAPS".into(), port: 993, open: Some(true) },
            ],
        }]
    }

    #[test]
    fn render_terminal_has_matrix() {
        let s = render_terminal(&sample(), Lang::Zh);
        assert!(s.contains("邮件端口连通性"));
        assert!(s.contains("Gmail"));
        assert!(s.contains("SMTPS"));
        assert!(s.contains("✔"));
        assert!(s.contains("✘"));
    }

    #[test]
    fn render_section_has_six_protocols() {
        let s = render_section(&sample(), Lang::Zh);
        assert!(s.contains("SMTP"));
        assert!(s.contains("IMAPS"));
        assert!(s.contains("| Gmail "));
    }

    #[test]
    fn na_protocol_renders_dash() {
        let r = MailResult {
            provider: "Apple".into(),
            protocols: vec![ProtoStatus { proto: "POP3".into(), port: 110, open: None }],
        };
        assert_eq!(cell(&r, "POP3"), "—");
        assert_eq!(cell(&r, "IMAP"), "—"); // 不存在的协议也回退 —
    }

    #[test]
    fn providers_cover_major_hosts() {
        let ps = providers();
        let names: Vec<&str> = ps.iter().map(|p| p.name).collect();
        assert!(names.contains(&"Gmail"));
        assert!(names.contains(&"QQ"));
        assert!(names.len() >= 12);
    }
}
