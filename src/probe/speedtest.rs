use std::time::Instant;
use serde::{Serialize, Deserialize};
use tokio::time::{timeout, Duration};

/// 单个测速节点(名称 + HTTP 可下载 URL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedNode {
    pub name: String,
    pub url: String,
}

/// 单节点测速结果
#[derive(Debug, Clone, Serialize)]
pub struct SpeedResult {
    pub name: String,
    pub mbps: f64,      // 下载速率(Mbps)
    pub bytes: u64,     // 实测下载字节数
    pub secs: f64,      // 实测耗时(秒)
    pub ok: bool,       // 是否成功(下到数据)
}

/// 默认测速节点:面向 VPS 的全球稳定 HTTP 下载点
pub fn default_nodes() -> Vec<SpeedNode> {
    vec![
        SpeedNode { name: "Cachefly CDN".into(),        url: "http://cachefly.cachefly.net/100mb.test".into() },
        SpeedNode { name: "Linode 东京".into(),         url: "http://speedtest.tokyo2.linode.com/100MB-tokyo2.bin".into() },
        SpeedNode { name: "Linode 美西".into(),         url: "http://speedtest.fremont.linode.com/100MB-fremont.bin".into() },
        SpeedNode { name: "ThinkBroadband 英国".into(), url: "http://ipv4.download.thinkbroadband.com/100MB.zip".into() },
    ]
}

/// 纯函数:由下载字节数与耗时算 Mbps(兆比特/秒)
pub fn calc_mbps(bytes: u64, secs: f64) -> f64 {
    if secs <= 0.0 { return 0.0; }
    (bytes as f64 * 8.0) / secs / 1_000_000.0
}

/// 对单节点做 HTTP 流式下载测速:到达 max_bytes 或 max_secs 即停。
async fn download_one(client: &reqwest::Client, node: &SpeedNode, max_bytes: u64, max_secs: u64) -> SpeedResult {
    let fail = |name: &str| SpeedResult { name: name.into(), mbps: 0.0, bytes: 0, secs: 0.0, ok: false };
    let start = Instant::now();
    let mut resp = match client.get(&node.url).send().await {
        Ok(r) => r,
        Err(_) => return fail(&node.name),
    };
    let deadline = Duration::from_secs(max_secs);
    let mut total: u64 = 0;
    loop {
        let elapsed = start.elapsed();
        if elapsed >= deadline { break; }
        match timeout(deadline - elapsed, resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                total += chunk.len() as u64;
                if total >= max_bytes { break; }
            }
            Ok(Ok(None)) => break,   // 服务端 EOF
            Ok(Err(_)) => break,     // 网络错误
            Err(_) => break,         // 到时
        }
    }
    let secs = start.elapsed().as_secs_f64();
    SpeedResult {
        name: node.name.clone(),
        mbps: calc_mbps(total, secs),
        bytes: total,
        secs,
        ok: total > 0,
    }
}

/// 串行跑所有测速节点(并发会互相抢带宽导致结果失真,故串行)。
pub async fn run_all(nodes: &[SpeedNode]) -> Vec<SpeedResult> {
    // 测速需长连接下载,用独立客户端:浏览器 UA(部分测速点如 Cloudflare
    // 会对非主流 UA 返回 403)+ 较长 total timeout(单节点逻辑上限 10s)
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| crate::fetch::build_client(20));
    let mut out = Vec::with_capacity(nodes.len());
    for n in nodes {
        out.push(download_one(&client, n, 50_000_000, 10).await);
    }
    out
}

/// 人类可读的数据量(MB)
fn fmt_mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// 速率 → comfy-table 颜色(高绿/中黄/低红/失败灰)
fn speed_color(r: &SpeedResult) -> comfy_table::Color {
    use comfy_table::Color;
    if !r.ok { return Color::DarkGrey; }
    match r.mbps {
        m if m >= 100.0 => Color::Green,
        m if m >= 20.0 => Color::Yellow,
        _ => Color::Red,
    }
}

/// 终端渲染(comfy-table 包边表;速率着色,no_color 时退化纯文本)
pub fn render_terminal(results: &[SpeedResult], lang: crate::i18n::Lang, no_color: bool) -> String {
    use comfy_table::{presets::UTF8_FULL, Cell, Table};
    let mut out = format!("═══ {} ═══\n", lang.pick("多节点下载测速", "Multi-node download speed"));
    out.push_str(&format!("{}\n", lang.pick(
        "从本机出口顺序下载各节点(单节点上限 50MB / 10s),仅供参考",
        "Sequential download per node (cap 50MB / 10s), for reference only",
    )));

    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(vec![
        lang.pick("节点", "Node"),
        lang.pick("下载速率", "Speed"),
        lang.pick("数据量", "Data"),
        lang.pick("耗时", "Time"),
    ]);
    for r in results {
        let speed_txt = if r.ok { format!("{:.1} Mbps", r.mbps) } else { lang.pick("失败", "failed").to_string() };
        let speed = Cell::new(speed_txt);
        let speed = if no_color { speed } else { speed.fg(speed_color(r)) };
        t.add_row(vec![
            Cell::new(&r.name),
            speed,
            Cell::new(fmt_mb(r.bytes)),
            Cell::new(format!("{:.1}s", r.secs)),
        ]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

/// Markdown 渲染(pipe 表)
pub fn render_section(results: &[SpeedResult], lang: crate::i18n::Lang) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "## {}\n", lang.pick("多节点下载测速", "Multi-node download speed")).ok();
    writeln!(out, "| {} | {} | {} | {} |",
        lang.pick("节点", "Node"), lang.pick("下载速率", "Speed"),
        lang.pick("数据量", "Data"), lang.pick("耗时", "Time")).ok();
    writeln!(out, "|---|---|---|---|").ok();
    for r in results {
        let speed_txt = if r.ok { format!("{:.1} Mbps", r.mbps) } else { lang.pick("失败", "failed").to_string() };
        writeln!(out, "| {} | {} | {} | {:.1}s |", r.name, speed_txt, fmt_mb(r.bytes), r.secs).ok();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_mbps_basic() {
        // 100 Mb(12.5 MB)/ 1s = 100 Mbps
        assert!((calc_mbps(12_500_000, 1.0) - 100.0).abs() < 0.01);
    }

    #[test]
    fn calc_mbps_ten_seconds() {
        // 50 MB / 10s = 40 Mbps
        assert!((calc_mbps(50_000_000, 10.0) - 40.0).abs() < 0.01);
    }

    #[test]
    fn calc_mbps_zero_secs_is_zero() {
        assert_eq!(calc_mbps(1_000_000, 0.0), 0.0);
    }

    #[test]
    fn fmt_mb_works() {
        assert_eq!(fmt_mb(50_000_000), "50.0 MB");
        assert_eq!(fmt_mb(0), "0.0 MB");
    }

    #[test]
    fn default_nodes_have_four() {
        let n = default_nodes();
        assert_eq!(n.len(), 4);
        assert!(n.iter().all(|x| x.url.starts_with("http")));
    }

    #[test]
    fn speed_color_thresholds() {
        use comfy_table::Color;
        let mk = |mbps: f64, ok: bool| SpeedResult { name: "x".into(), mbps, bytes: 1, secs: 1.0, ok };
        assert_eq!(speed_color(&mk(150.0, true)), Color::Green);
        assert_eq!(speed_color(&mk(50.0, true)), Color::Yellow);
        assert_eq!(speed_color(&mk(5.0, true)), Color::Red);
        assert_eq!(speed_color(&mk(150.0, false)), Color::DarkGrey);
    }

    #[test]
    fn render_terminal_no_color_plain() {
        let results = vec![
            SpeedResult { name: "Cloudflare".into(), mbps: 123.4, bytes: 50_000_000, secs: 3.2, ok: true },
            SpeedResult { name: "Dead".into(), mbps: 0.0, bytes: 0, secs: 0.0, ok: false },
        ];
        let out = render_terminal(&results, crate::i18n::Lang::Zh, true);
        assert!(out.contains("Cloudflare"));
        assert!(out.contains("123.4 Mbps"));
        assert!(out.contains("失败"));
        assert!(out.contains("50.0 MB"));
    }

    #[test]
    fn render_section_markdown() {
        let results = vec![
            SpeedResult { name: "Linode".into(), mbps: 88.8, bytes: 30_000_000, secs: 2.7, ok: true },
        ];
        let out = render_section(&results, crate::i18n::Lang::En);
        assert!(out.contains("Linode"));
        assert!(out.contains("88.8 Mbps"));
        assert!(out.contains("Speed"));
    }

    #[test]
    fn speed_node_deserializes_from_toml() {
        let toml = r#"
name = "测试点"
url = "http://example.com/100mb.bin"
"#;
        let n: SpeedNode = toml::from_str(toml).unwrap();
        assert_eq!(n.name, "测试点");
        assert_eq!(n.url, "http://example.com/100mb.bin");
    }
}
