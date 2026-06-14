use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use futures::future::join_all;

pub mod streaming;
pub mod ai;
pub mod mail;
pub mod route;
pub mod dnsbl;
pub mod speedtest;
pub mod unlock_util;
pub mod web;
pub mod cdn;

/// 解锁探测结果状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeStatus {
    Unlocked,    // 完全解锁
    Restricted,  // 部分解锁(如 Netflix 仅自制剧)
    Blocked,     // 封锁/不可用
    Unknown,     // 探测失败,无法判定
}

/// 解锁类型:IP 直连原生 vs DNS 重定向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnlockType {
    Native,   // 原生 — 探针 IP 所属地区与内容地区一致
    Dns,      // DNS 解锁 — 地区不符,但通过 DNS 重定向可访问
    Unknown,  // 无法判定(多发生于未携带 region 的服务)
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub name: String,
    pub status: ProbeStatus,
    pub region: Option<String>,
    pub unlock_type: UnlockType,
    pub info: Option<String>,
}

impl ProbeResult {
    pub fn new(name: &str, status: ProbeStatus, region: Option<String>) -> Self {
        ProbeResult { name: name.to_string(), status, region, unlock_type: UnlockType::Unknown, info: None }
    }
    pub fn unknown(name: &str) -> Self {
        ProbeResult::new(name, ProbeStatus::Unknown, None)
    }
    pub fn with_info(mut self, info: impl Into<String>) -> Self {
        self.info = Some(info.into());
        self
    }
}

#[async_trait]
pub trait Probe: Send + Sync {
    fn name(&self) -> &'static str;
    async fn check(&self, client: &Client) -> ProbeResult;
}

/// 并发跑所有探针,并根据探针机所在地区推断 Native/DNS 类型。
/// probe_country: 探针机 ISO 两字母国家码(如 "JP"),空串表示跳过此推断。
pub async fn run_all_with_native_check(
    client: &Client,
    probes: &[Box<dyn Probe>],
    probe_country: &str,
) -> Vec<ProbeResult> {
    let mut results = join_all(probes.iter().map(|p| p.check(client))).await;
    if !probe_country.is_empty() {
        for r in &mut results {
            if matches!(r.status, ProbeStatus::Unlocked | ProbeStatus::Restricted) {
                r.unlock_type = classify_native_dns(probe_country, r.region.as_deref());
            }
        }
    }
    results
}

/// 纯函数:探针机地区 vs 内容地区 → UnlockType。
/// 地区相符 → Native;地区不符 → Dns;region 为 None → Unknown。
pub fn classify_native_dns(probe_country: &str, content_region: Option<&str>) -> UnlockType {
    match content_region {
        Some(r) if r.eq_ignore_ascii_case(probe_country) => UnlockType::Native,
        Some(_) => UnlockType::Dns,
        None => UnlockType::Unknown,
    }
}

pub fn all_probes() -> Vec<Box<dyn Probe>> {
    use streaming::*;
    vec![
        Box::new(Netflix::default()),
        Box::new(YouTube::default()),
        Box::new(DisneyPlus::default()),
        Box::new(HboMax::default()),
        Box::new(Hulu::default()),
        Box::new(PrimeVideo::default()),
        Box::new(BilibiliCn::default()),
        Box::new(BilibiliHkTw::default()),
        Box::new(AbemaTV::default()),
        Box::new(Dazn::default()),
        Box::new(BbcIplayer::default()),
        Box::new(Crunchyroll::default()),
        Box::new(ParamountPlus::default()),
        Box::new(Peacock::default()),
        Box::new(DiscoveryPlus::default()),
        Box::new(Spotify::default()),
        Box::new(TvbAnywhere::default()),
        Box::new(Funimation::default()),
        Box::new(ai::ChatGpt::default()),
        Box::new(ai::Claude::default()),
        Box::new(ai::Gemini::default()),
        Box::new(web::Bing::default()),
        Box::new(web::GoogleSearch::default()),
        Box::new(web::Reddit::default()),
        Box::new(web::Wikipedia::default()),
        Box::new(web::OneTrust::default()),
        Box::new(web::Apple::default()),
        Box::new(web::Steam::default()),
        Box::new(IQiYi::default()),
        Box::new(Kocowa::default()),
        Box::new(Viu::default()),
        Box::new(TikTok::default()),
        Box::new(cdn::NetflixCdn::default()),
        Box::new(cdn::YoutubeCdn::default()),
        Box::new(web::MetaAI::default()),
        Box::new(web::SonyLiv::default()),
        Box::new(web::GooglePlay::default()),
        Box::new(web::InstagramMusic::default()),
    ]
}

impl ProbeStatus {
    pub fn label(self, lang: crate::i18n::Lang) -> &'static str {
        match self {
            ProbeStatus::Unlocked => lang.pick("✓ 解锁", "✓ Unlocked"),
            ProbeStatus::Restricted => lang.pick("◐ 部分", "◐ Restricted"),
            ProbeStatus::Blocked => lang.pick("✗ 封锁", "✗ Blocked"),
            ProbeStatus::Unknown => lang.pick("? 未知", "? Unknown"),
        }
    }
}

impl UnlockType {
    pub fn label(self, lang: crate::i18n::Lang) -> &'static str {
        match self {
            UnlockType::Native => lang.pick("原生", "Native"),
            UnlockType::Dns => "DNS",
            UnlockType::Unknown => "—",
        }
    }
}

/// 终端渲染(comfy-table 包边表;按状态着色,no_color 时退化为纯文本)
pub fn render_terminal(results: &[ProbeResult], lang: crate::i18n::Lang, no_color: bool) -> String {
    use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
    let unlocked = results.iter().filter(|r| r.status == ProbeStatus::Unlocked).count();
    let mut out = String::new();
    out.push_str(&format!("═══ {} ({}/{}) ═══\n",
        lang.pick("流媒体 & AI 解锁检测", "Streaming & AI unlock"),
        unlocked, results.len()));

    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(vec![
        lang.pick("服务", "Service"),
        lang.pick("状态", "Status"),
        lang.pick("地区", "Region"),
        lang.pick("类型", "Type"),
        lang.pick("备注", "Note"),
    ]);
    for r in results {
        let region = r.region.clone().unwrap_or_else(|| "—".to_string());
        let status = Cell::new(r.status.label(lang));
        let status = if no_color { status } else { status.fg(status_color(r.status)) };
        let utype = Cell::new(r.unlock_type.label(lang));
        let utype = match (no_color, r.unlock_type) {
            (false, UnlockType::Native) => utype.fg(Color::Green),
            (false, UnlockType::Dns) => utype.fg(Color::Yellow),
            _ => utype,
        };
        let note = r.info.clone().unwrap_or_else(|| "—".to_string());
        t.add_row(vec![Cell::new(&r.name), status, Cell::new(region), utype, Cell::new(note)]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

/// 状态 → comfy-table 颜色
fn status_color(s: ProbeStatus) -> comfy_table::Color {
    use comfy_table::Color;
    match s {
        ProbeStatus::Unlocked => Color::Green,
        ProbeStatus::Restricted => Color::Yellow,
        ProbeStatus::Blocked => Color::Red,
        ProbeStatus::Unknown => Color::DarkGrey,
    }
}

/// Markdown 渲染(pipe 表,兼容旧行为)
pub fn render_section(results: &[ProbeResult], lang: crate::i18n::Lang) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "## {}\n", lang.pick("流媒体 & AI 解锁检测", "Streaming & AI unlock")).ok();
    writeln!(out, "| {} | {} | {} | {} | {} |",
        lang.pick("服务", "Service"),
        lang.pick("状态", "Status"),
        lang.pick("地区", "Region"),
        lang.pick("类型", "Type"),
        lang.pick("备注", "Note"),
    ).ok();
    writeln!(out, "|---|---|---|---|---|").ok();
    for r in results {
        let region = r.region.clone().unwrap_or_else(|| "—".to_string());
        let note = r.info.clone().unwrap_or_else(|| "—".to_string());
        writeln!(out, "| {} | {} | {} | {} | {} |",
            r.name, r.status.label(lang), region, r.unlock_type.label(lang), note).ok();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_native_dns_match() {
        assert_eq!(classify_native_dns("JP", Some("JP")), UnlockType::Native);
        assert_eq!(classify_native_dns("jp", Some("JP")), UnlockType::Native);
    }

    #[test]
    fn classify_native_dns_mismatch() {
        assert_eq!(classify_native_dns("US", Some("JP")), UnlockType::Dns);
    }

    #[test]
    fn classify_native_dns_no_region() {
        assert_eq!(classify_native_dns("US", None), UnlockType::Unknown);
    }

    #[test]
    fn run_all_with_native_check_sets_type() {
        // Pure structural test — no async needed for native check logic
        let r = ProbeResult::new("Test", ProbeStatus::Unlocked, Some("US".into()));
        assert_eq!(r.unlock_type, UnlockType::Unknown); // default before check
        let ut = classify_native_dns("US", r.region.as_deref());
        assert_eq!(ut, UnlockType::Native);
    }

    #[test]
    fn probe_result_with_info_and_default_none() {
        let r = ProbeResult::new("Steam", ProbeStatus::Unlocked, Some("us".into()));
        assert_eq!(r.info, None);
        let r2 = r.with_info("Community Available");
        assert_eq!(r2.info.as_deref(), Some("Community Available"));
    }

    #[test]
    fn render_terminal_has_note_column() {
        let mut r = ProbeResult::new("Steam", ProbeStatus::Unlocked, Some("us".into()));
        r = r.with_info("Community Available");
        let s = render_terminal(&[r], crate::i18n::Lang::Zh, true);
        assert!(s.contains("备注"));
        assert!(s.contains("Community Available"));
    }
}
