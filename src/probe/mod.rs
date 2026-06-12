use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use futures::future::join_all;

pub mod streaming;
pub mod ai;

/// 解锁探测结果状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeStatus {
    Unlocked,    // 完全解锁
    Restricted,  // 部分解锁(如 Netflix 仅自制剧)
    Blocked,     // 封锁/不可用
    Unknown,     // 探测失败,无法判定
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub name: String,
    pub status: ProbeStatus,
    pub region: Option<String>,
}

impl ProbeResult {
    pub fn new(name: &str, status: ProbeStatus, region: Option<String>) -> Self {
        ProbeResult { name: name.to_string(), status, region }
    }
    pub fn unknown(name: &str) -> Self {
        ProbeResult::new(name, ProbeStatus::Unknown, None)
    }
}

#[async_trait]
pub trait Probe: Send + Sync {
    fn name(&self) -> &'static str;
    async fn check(&self, client: &Client) -> ProbeResult;
}

/// 并发跑所有探针(从本机出口发起);单探针失败不影响其它。
pub async fn run_all(client: &Client, probes: &[Box<dyn Probe>]) -> Vec<ProbeResult> {
    join_all(probes.iter().map(|p| p.check(client))).await
}

pub fn all_probes() -> Vec<Box<dyn Probe>> {
    vec![
        Box::new(streaming::Netflix::default()),
        Box::new(streaming::YouTube::default()),
        Box::new(ai::ChatGpt::default()),
    ]
}

impl ProbeStatus {
    /// 双语展示文案
    pub fn label(self, lang: crate::i18n::Lang) -> &'static str {
        match self {
            ProbeStatus::Unlocked => lang.pick("✓ 解锁", "✓ Unlocked"),
            ProbeStatus::Restricted => lang.pick("◐ 部分", "◐ Restricted"),
            ProbeStatus::Blocked => lang.pick("✗ 封锁", "✗ Blocked"),
            ProbeStatus::Unknown => lang.pick("? 未知", "? Unknown"),
        }
    }
}

/// 渲染解锁检测区(Markdown 表,终端与 markdown 通用)
pub fn render_section(results: &[ProbeResult], lang: crate::i18n::Lang) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "## {}\n", lang.pick("解锁检测", "Unlock test")).ok();
    writeln!(out, "| {} | {} | {} |", lang.pick("服务", "Service"), lang.pick("状态", "Status"), lang.pick("地区", "Region")).ok();
    writeln!(out, "|---|---|---|").ok();
    for r in results {
        let region = r.region.clone().unwrap_or_else(|| "—".to_string());
        writeln!(out, "| {} | {} | {} |", r.name, r.status.label(lang), region).ok();
    }
    out
}
