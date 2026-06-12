use std::time::Duration;
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::time::timeout;
use futures::future::join_all;
use crate::i18n::Lang;

/// SMTP 常用端口:25(中继)、465(SMTPS)、587(提交)
pub const PORTS: [u16; 3] = [25, 465, 587];

#[derive(Debug, Clone, Serialize)]
pub struct PortStatus {
    pub port: u16,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MailResult {
    pub provider: String,
    pub host: String,
    pub ports: Vec<PortStatus>,
}

/// TCP 连接探测:能在超时内建连即视为端口开放
pub async fn check_port(host: &str, port: u16, timeout_secs: u64) -> bool {
    let addr = format!("{}:{}", host, port);
    matches!(
        timeout(Duration::from_secs(timeout_secs), TcpStream::connect(&addr)).await,
        Ok(Ok(_))
    )
}

fn targets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Gmail", "smtp.gmail.com"),
        ("Outlook", "smtp-mail.outlook.com"),
        ("QQ", "smtp.qq.com"),
        ("Yahoo", "smtp.mail.yahoo.com"),
        ("Apple", "smtp.mail.me.com"),
    ]
}

/// 并发探测各邮局各端口连通性
pub async fn check_all(timeout_secs: u64) -> Vec<MailResult> {
    let futs = targets().into_iter().map(|(provider, host)| async move {
        let port_futs = PORTS.iter().map(|&port| async move {
            PortStatus { port, open: check_port(host, port, timeout_secs).await }
        });
        let ports = join_all(port_futs).await;
        MailResult { provider: provider.to_string(), host: host.to_string(), ports }
    });
    join_all(futs).await
}

/// 渲染邮局连通性区(Markdown 表)
pub fn render_section(results: &[MailResult], lang: Lang) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "## {}\n", lang.pick("邮局连通性(SMTP)", "Mail connectivity (SMTP)")).ok();
    writeln!(out, "| {} | 25 | 465 | 587 |", lang.pick("邮局", "Provider")).ok();
    writeln!(out, "|---|---|---|---|").ok();
    for r in results {
        let cell = |port: u16| -> &'static str {
            match r.ports.iter().find(|p| p.port == port) {
                Some(p) if p.open => "✓",
                Some(_) => "✗",
                None => "—",
            }
        };
        writeln!(out, "| {} | {} | {} | {} |", r.provider, cell(25), cell(465), cell(587)).ok();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn open_port_connects() {
        // 本地起监听 → 该端口可连
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(check_port("127.0.0.1", port, 3).await);
    }

    #[tokio::test]
    async fn closed_port_fails() {
        // 端口 1 几乎不会有监听 → 连接失败
        assert!(!check_port("127.0.0.1", 1, 2).await);
    }

    #[test]
    fn render_marks_open_closed() {
        let results = vec![MailResult {
            provider: "Gmail".into(),
            host: "smtp.gmail.com".into(),
            ports: vec![
                PortStatus { port: 25, open: false },
                PortStatus { port: 465, open: true },
                PortStatus { port: 587, open: true },
            ],
        }];
        let s = render_section(&results, Lang::Zh);
        assert!(s.contains("Gmail"));
        assert!(s.contains("邮局连通性"));
        // 25 关、465/587 开
        assert!(s.contains("| Gmail | ✗ | ✓ | ✓ |"));
    }
}
