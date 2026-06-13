use std::time::Instant;
use serde::Serialize;
use tokio::time::{timeout, Duration};

/// 运营商 / 区域分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Carrier { Telecom, Unicom, Mobile, Edu, Hk, Us, Jp, Sg }

impl Carrier {
    /// 表格显示名
    pub fn label(self) -> &'static str {
        match self {
            Carrier::Telecom => "电信", Carrier::Unicom => "联通", Carrier::Mobile => "移动",
            Carrier::Edu => "教育/广电", Carrier::Hk => "香港",
            Carrier::Us => "美国", Carrier::Jp => "日本", Carrier::Sg => "新加坡",
        }
    }
    /// 分组关键字 → 该组运营商集合;None 表示不是分组词
    fn group(kw: &str) -> Option<Vec<Carrier>> {
        use Carrier::*;
        Some(match kw {
            "ct" => vec![Telecom], "cu" => vec![Unicom], "cm" => vec![Mobile],
            "hk" => vec![Hk], "edu" => vec![Edu],
            "us" => vec![Us], "jp" => vec![Jp], "sg" => vec![Sg],
            "cn" => vec![Telecom, Unicom, Mobile],
            "intl" => vec![Us, Jp, Sg],
            _ => return None,
        })
    }

    pub fn from_str_lenient(s: &str) -> Carrier {
        match s.to_ascii_lowercase().as_str() {
            "unicom" | "cu" => Carrier::Unicom, "mobile" | "cm" => Carrier::Mobile,
            "edu" => Carrier::Edu, "hk" => Carrier::Hk,
            "us" => Carrier::Us, "jp" => Carrier::Jp, "sg" => Carrier::Sg,
            _ => Carrier::Telecom, // 默认电信
        }
    }
}

/// 测速目标节点。`host=Some` 直接用;`host=None` 运行时按 `search`+`id` 解析。
#[derive(Debug, Clone)]
pub struct SpeedNode {
    pub id: u32,                 // speedtest.net server id;自定义节点用 0
    pub name: String,            // 显示名
    pub carrier: Carrier,
    pub search: String,          // 解析 host 用的城市搜索词(英文)
    pub host: Option<String>,    // 自定义节点直接给 host
    pub default: bool,           // 是否属默认 6 代表
}

/// 选择结果:列目录 / 跑这批节点
pub enum Selection { List, Nodes(Vec<SpeedNode>) }

/// 内置节点目录(server id + 城市搜索词;来源 superspeed.sh 三网 + 港 + 教育 + 国际)
pub fn catalog() -> Vec<SpeedNode> {
    use Carrier::*;
    const T: &[(u32, &str, Carrier, &str, bool)] = &[
        (3633,  "电信 上海",        Telecom, "shanghai",    false),
        (27594, "电信 广州",        Telecom, "guangzhou",   false),
        (34115, "电信 天津5G",      Telecom, "tianjin",     false),
        (17145, "电信 合肥5G",      Telecom, "hefei",       false),
        (5396,  "电信 江苏苏州5G",  Telecom, "suzhou",      true),
        (5317,  "电信 扬州5G",      Telecom, "yangzhou",    false),
        (36663, "电信 镇江5G",      Telecom, "zhenjiang",   false),
        (29071, "电信 成都",        Telecom, "chengdu",     false),
        (29353, "电信 武汉5G",      Telecom, "wuhan",       false),
        (28225, "电信 长沙5G",      Telecom, "changsha",    false),
        (3973,  "电信 兰州",        Telecom, "lanzhou",     false),
        (34988, "电信 沈阳5G",      Telecom, "shenyang",    false),
        (59386, "电信 浙江杭州",    Telecom, "hangzhou",    true),
        (24447, "联通 上海5G",      Unicom,  "shanghai",    true),
        (54625, "联通 南昌",        Unicom,  "nanchang",    false),
        (45170, "联通 无锡",        Unicom,  "wuxi",        false),
        (4884,  "联通 福州",        Unicom,  "fuzhou",      false),
        (36646, "联通 郑州5G",      Unicom,  "zhengzhou",   false),
        (37235, "联通 沈阳",        Unicom,  "shenyang",    false),
        (43752, "联通 北京",        Unicom,  "beijing",     true),
        (25637, "移动 上海5G",      Mobile,  "shanghai",    true),
        (6715,  "移动 杭州5G",      Mobile,  "hangzhou",    false),
        (26404, "移动 合肥5G",      Mobile,  "hefei",       false),
        (25858, "移动 北京",        Mobile,  "beijing",     true),
        (4575,  "移动 成都",        Mobile,  "chengdu",     false),
        (41910, "移动 郑州5G",      Mobile,  "zhengzhou",   false),
        (16171, "移动 福州",        Mobile,  "fuzhou",      false),
        (26940, "移动 银川5G",      Mobile,  "yinchuan",    false),
        (53087, "移动 深圳",        Mobile,  "shenzhen",    false),
        (54312, "移动 杭州",        Mobile,  "hangzhou",    false),
        (16145, "移动 兰州",        Mobile,  "lanzhou",     false),
        (29105, "移动 西安",        Mobile,  "xi'an",       false),
        (37639, "香港 CMHK Broadband", Hk,   "hong%20kong", false),
        (13538, "香港 CSL",         Hk,      "hong%20kong", false),
        (32155, "香港 CMHK Mobile", Hk,      "hong%20kong", false),
        (30852, "教育网 江苏昆山",  Edu,     "kunshan",     false),
        (35527, "广电 四川成都",    Edu,     "chengdu",     false),
        (43201, "美国 洛杉矶",      Us,      "los%20angeles", false),
        (17846, "美国 圣何塞",      Us,      "san%20jose",  false),
        (48463, "日本 东京",        Jp,      "tokyo",       false),
        (31293, "新加坡",           Sg,      "singapore",   false),
    ];
    T.iter().map(|&(id, name, carrier, search, default)| SpeedNode {
        id, name: name.into(), carrier, search: search.into(), host: None, default,
    }).collect()
}

/// 解析 SPEC → 选择。逗号分割;每段为分组词 / `all` / 数字 id;`list` 单独;未知报错。
pub fn parse_spec(spec: &str, catalog: &[SpeedNode]) -> Result<Selection, String> {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("list") { return Ok(Selection::List); }
    if spec.is_empty() {
        return Ok(Selection::Nodes(catalog.iter().filter(|n| n.default).cloned().collect()));
    }
    let mut picked: Vec<SpeedNode> = Vec::new();
    let push = |node: &SpeedNode, out: &mut Vec<SpeedNode>| {
        if !out.iter().any(|x| x.id == node.id && x.name == node.name) { out.push(node.clone()); }
    };
    for raw in spec.split(',') {
        let seg = raw.trim();
        if seg.is_empty() { continue; }
        let low = seg.to_ascii_lowercase();
        if low == "all" {
            for n in catalog { push(n, &mut picked); }
        } else if let Some(carriers) = Carrier::group(&low) {
            for n in catalog.iter().filter(|n| carriers.contains(&n.carrier)) { push(n, &mut picked); }
        } else if let Ok(id) = seg.parse::<u32>() {
            match catalog.iter().find(|n| n.id == id) {
                Some(n) => push(n, &mut picked),
                None => return Err(format!("未知节点 id: {id}")),
            }
        } else {
            return Err(format!("未知选择关键字: '{seg}'(有效:all/cn/ct/cu/cm/hk/edu/intl/us/jp/sg/list 或 server id)"));
        }
    }
    Ok(Selection::Nodes(picked))
}

/// 从 speedtest.net search API 的 JSON 数组里按 id 取 host(纯函数)
fn pick_host_from_json(body: &str, id: u32) -> Option<String> {
    let arr: serde_json::Value = serde_json::from_str(body).ok()?;
    for item in arr.as_array()? {
        if item.get("id").and_then(|v| v.as_str()) == Some(&id.to_string()) {
            return item.get("host").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
    }
    None
}

/// 运行时把选中节点解析为 host。按 search 词分组,每词只查一次 API 并缓存。
/// 返回与输入等长的 Vec<Option<String>>(host),解析失败为 None。
async fn resolve_hosts(client: &reqwest::Client, nodes: &[SpeedNode]) -> Vec<Option<String>> {
    use std::collections::HashMap;
    let mut cache: HashMap<String, String> = HashMap::new();
    let mut out = Vec::with_capacity(nodes.len());
    for n in nodes {
        if let Some(h) = &n.host { out.push(Some(h.clone())); continue; }
        let body = match cache.get(&n.search) {
            Some(b) => b.clone(),
            None => {
                let url = format!(
                    "https://www.speedtest.net/api/js/servers?engine=js&limit=100&search={}",
                    n.search);
                let body = match client.get(&url)
                    .header("Referer", "https://www.speedtest.net/")
                    .send().await
                {
                    Ok(r) => r.text().await.unwrap_or_default(),
                    Err(_) => String::new(),
                };
                cache.insert(n.search.clone(), body.clone());
                body
            }
        };
        out.push(pick_host_from_json(&body, n.id));
    }
    out
}

/// 单节点测速结果
#[derive(Debug, Clone, Serialize)]
pub struct SpeedResult {
    pub name: String,
    pub carrier: Carrier,
    pub latency_ms: Option<f64>,
    pub down_mbps: f64, pub down_bytes: u64, pub down_secs: f64,
    pub up_mbps: f64,   pub up_bytes: u64,   pub up_secs: f64,
    pub ok: bool,
}

/// 纯函数:由字节数与耗时算 Mbps
pub fn calc_mbps(bytes: u64, secs: f64) -> f64 {
    if secs <= 0.0 { return 0.0; }
    (bytes as f64 * 8.0) / secs / 1_000_000.0
}

/// 延迟:GET http://host/latency.txt ×4,取最小 ms
async fn measure_latency(client: &reqwest::Client, host: &str) -> Option<f64> {
    let url = format!("http://{host}/latency.txt");
    let mut best: Option<f64> = None;
    for _ in 0..4 {
        let start = Instant::now();
        match timeout(Duration::from_secs(4), client.get(&url).send()).await {
            Ok(Ok(resp)) if resp.status().is_success() => {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                best = Some(best.map_or(ms, |b: f64| b.min(ms)));
            }
            _ => {}
        }
    }
    best
}

/// 下载:GET /download?size=N,chunked + deadline
async fn measure_download(client: &reqwest::Client, host: &str, max_secs: u64) -> (f64, u64, f64) {
    let url = format!("http://{host}/download?nocache={}&size=100000000", rand_tag());
    let start = Instant::now();
    let mut total = 0u64;
    let deadline = Duration::from_secs(max_secs);
    if let Ok(mut resp) = client.get(&url).send().await {
        loop {
            let elapsed = start.elapsed();
            if elapsed >= deadline { break; }
            match timeout(deadline - elapsed, resp.chunk()).await {
                Ok(Ok(Some(c))) => { total += c.len() as u64; }
                _ => break,
            }
        }
    }
    let secs = start.elapsed().as_secs_f64();
    (calc_mbps(total, secs), total, secs)
}

/// 上传:POST /upload,流式发送零缓冲到 deadline,原子计数器统计已发字节
async fn measure_upload(client: &reqwest::Client, host: &str, max_secs: u64) -> (f64, u64, f64) {
    use futures::stream;
    use futures::StreamExt;
    use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
    let url = format!("http://{host}/upload?nocache={}", rand_tag());
    let counter = Arc::new(AtomicU64::new(0));
    let c2 = counter.clone();
    // 构造有界 TryStream(2000 × 1MB 块),wrap_stream 需 reqwest feature = "stream"
    let stream = stream::repeat_with(move || {
        c2.fetch_add(1_000_000, Ordering::Relaxed);
        Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(vec![0u8; 1_000_000]))
    }).take(2000);
    let body = reqwest::Body::wrap_stream(stream);
    let start = Instant::now();
    let _ = timeout(
        Duration::from_secs(max_secs),
        client.post(&url).header("Content-Type", "application/octet-stream").body(body).send()
    ).await;
    let secs = start.elapsed().as_secs_f64();
    let sent = counter.load(Ordering::Relaxed);
    (calc_mbps(sent, secs), sent, secs)
}

fn rand_tag() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

/// 单节点:延迟 → 下载 → 上传
async fn probe_one(client: &reqwest::Client, node: &SpeedNode, host: &str) -> SpeedResult {
    let latency_ms = measure_latency(client, host).await;
    let (down_mbps, down_bytes, down_secs) = measure_download(client, host, 10).await;
    let (up_mbps, up_bytes, up_secs) = measure_upload(client, host, 8).await;
    SpeedResult {
        name: node.name.clone(), carrier: node.carrier,
        latency_ms, down_mbps, down_bytes, down_secs, up_mbps, up_bytes, up_secs,
        ok: down_bytes > 0,
    }
}

/// 解析失败的占位结果
fn unresolved(node: &SpeedNode) -> SpeedResult {
    SpeedResult {
        name: node.name.clone(), carrier: node.carrier, latency_ms: None,
        down_mbps: 0.0, down_bytes: 0, down_secs: 0.0, up_mbps: 0.0, up_bytes: 0, up_secs: 0.0,
        ok: false,
    }
}

/// 串行跑所有节点(并发抢带宽失真)
pub async fn run_all(nodes: &[SpeedNode]) -> Vec<SpeedResult> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| crate::fetch::build_client(30));
    let hosts = resolve_hosts(&client, nodes).await;
    let mut out = Vec::with_capacity(nodes.len());
    for (n, host) in nodes.iter().zip(hosts) {
        match host {
            Some(h) => out.push(probe_one(&client, n, &h).await),
            None => out.push(unresolved(n)),
        }
    }
    out
}

#[allow(dead_code)]
fn fmt_mb(bytes: u64) -> String { format!("{:.1} MB", bytes as f64 / 1_000_000.0) }

fn speed_color(mbps: f64, ok: bool) -> comfy_table::Color {
    use comfy_table::Color;
    if !ok { return Color::DarkGrey; }
    match mbps { m if m >= 100.0 => Color::Green, m if m >= 20.0 => Color::Yellow, _ => Color::Red }
}
fn latency_color(ms: f64) -> comfy_table::Color {
    use comfy_table::Color;
    match ms { x if x <= 50.0 => Color::Green, x if x <= 150.0 => Color::Yellow, _ => Color::Red }
}
fn fmt_speed(mbps: f64, ok: bool, lang: crate::i18n::Lang) -> String {
    if ok { format!("{mbps:.1} Mbps") } else { lang.pick("失败", "failed").to_string() }
}
fn fmt_latency(ms: Option<f64>) -> String { ms.map_or("-".to_string(), |x| format!("{x:.0} ms")) }

pub fn render_terminal(results: &[SpeedResult], lang: crate::i18n::Lang, no_color: bool) -> String {
    use comfy_table::{presets::UTF8_FULL, Cell, Table};
    let mut out = format!("═══ {} ═══\n", lang.pick("多节点测速(回国/国际)", "Multi-node speed (CN backhaul / intl)"));
    out.push_str(&format!("{}\n", lang.pick(
        "对 speedtest.net 三网/国际节点测延迟+下载+上传(单连接,仅供参考)",
        "Latency + download + upload vs speedtest.net nodes (single-conn, for reference)")));
    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(vec![
        lang.pick("节点", "Node"), lang.pick("运营商", "Carrier"),
        lang.pick("延迟", "Latency"), lang.pick("下载", "Download"), lang.pick("上传", "Upload"),
    ]);
    for r in results {
        let down = { let c = Cell::new(fmt_speed(r.down_mbps, r.ok, lang));
            if no_color { c } else { c.fg(speed_color(r.down_mbps, r.ok)) } };
        let up = { let c = Cell::new(fmt_speed(r.up_mbps, r.ok && r.up_bytes > 0, lang));
            if no_color { c } else { c.fg(speed_color(r.up_mbps, r.ok && r.up_bytes > 0)) } };
        let lat = { let c = Cell::new(fmt_latency(r.latency_ms));
            match (no_color, r.latency_ms) { (false, Some(ms)) => c.fg(latency_color(ms)), _ => c } };
        t.add_row(vec![Cell::new(&r.name), Cell::new(r.carrier.label()), lat, down, up]);
    }
    out.push_str(&t.to_string()); out.push('\n'); out
}

pub fn render_section(results: &[SpeedResult], lang: crate::i18n::Lang) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "## {}\n", lang.pick("多节点测速(回国/国际)", "Multi-node speed (CN backhaul / intl)")).ok();
    writeln!(out, "| {} | {} | {} | {} | {} |",
        lang.pick("节点","Node"), lang.pick("运营商","Carrier"),
        lang.pick("延迟","Latency"), lang.pick("下载","Download"), lang.pick("上传","Upload")).ok();
    writeln!(out, "|---|---|---|---|---|").ok();
    for r in results {
        writeln!(out, "| {} | {} | {} | {} | {} |",
            r.name, r.carrier.label(), fmt_latency(r.latency_ms),
            fmt_speed(r.down_mbps, r.ok, lang), fmt_speed(r.up_mbps, r.ok && r.up_bytes > 0, lang)).ok();
    }
    out
}

/// `--speedtest=list`:打印完整目录(id/运营商/名称)
pub fn render_catalog(catalog: &[SpeedNode], lang: crate::i18n::Lang) -> String {
    use comfy_table::{presets::UTF8_FULL, Table};
    let mut out = format!("═══ {} ═══\n", lang.pick("测速节点目录", "Speedtest node catalog"));
    out.push_str(&format!("{}\n", lang.pick(
        "用 --speedtest=<分组/id,逗号分隔> 选择,如 --speedtest=cn,jp",
        "Select via --speedtest=<group/id,comma>, e.g. --speedtest=cn,jp")));
    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(vec![lang.pick("ID","ID"), lang.pick("运营商","Carrier"), lang.pick("节点","Node")]);
    for n in catalog.iter().filter(|n| n.id != 0) {
        t.add_row(vec![n.id.to_string(), n.carrier.label().to_string(), n.name.clone()]);
    }
    out.push_str(&t.to_string()); out.push('\n'); out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_mbps_basic() {
        assert!((calc_mbps(12_500_000, 1.0) - 100.0).abs() < 0.01);
    }

    #[test]
    fn calc_mbps_ten_seconds() {
        assert!((calc_mbps(50_000_000, 10.0) - 40.0).abs() < 0.01);
    }

    #[test]
    fn calc_mbps_zero_secs_is_zero() {
        assert_eq!(calc_mbps(1_000_000, 0.0), 0.0);
    }

    #[test]
    fn catalog_is_sane() {
        let c = catalog();
        assert!(c.len() >= 40, "目录至少 40 个节点, got {}", c.len());
        let mut ids: Vec<u32> = c.iter().map(|n| n.id).filter(|&i| i != 0).collect();
        let n = ids.len();
        ids.sort_unstable(); ids.dedup();
        assert_eq!(ids.len(), n, "目录 id 不应重复");
        assert_eq!(c.iter().filter(|n| n.default).count(), 6, "默认集应为 6");
        assert!(c.iter().all(|n| n.host.is_some() || !n.search.is_empty()));
    }

    #[test]
    fn parse_spec_empty_is_default_six() {
        let c = catalog();
        let sel = parse_spec("", &c).unwrap();
        match sel { Selection::Nodes(v) => assert_eq!(v.len(), 6), _ => panic!("应为 Nodes") }
    }

    #[test]
    fn parse_spec_groups() {
        let c = catalog();
        let n = |s: &str| match parse_spec(s, &c).unwrap() { Selection::Nodes(v) => v.len(), _ => 0 };
        assert!(n("all") >= 40);
        let cn = n("cn"); let ct = n("ct"); let cu = n("cu"); let cm = n("cm");
        assert_eq!(cn, ct + cu + cm, "cn 应等于三网之和");
        assert!(n("hk") >= 3);
        assert_eq!(n("intl"), n("us") + n("jp") + n("sg"));
    }

    #[test]
    fn parse_spec_ids_and_mixed_dedup() {
        let c = catalog();
        match parse_spec("5396,24447", &c).unwrap() {
            Selection::Nodes(v) => { assert_eq!(v.len(), 2); assert!(v.iter().any(|n| n.id == 5396)); }
            _ => panic!(),
        }
        match parse_spec("cn,5396", &c).unwrap() {
            Selection::Nodes(v) => {
                let cn = match parse_spec("cn", &c).unwrap() { Selection::Nodes(x) => x.len(), _ => 0 };
                assert_eq!(v.len(), cn);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_spec_list_and_unknown() {
        let c = catalog();
        assert!(matches!(parse_spec("list", &c).unwrap(), Selection::List));
        assert!(parse_spec("mars", &c).is_err(), "未知关键字应报错");
        assert!(parse_spec("99999999", &c).is_err(), "未知 id 应报错");
    }

    #[test]
    fn pick_host_matches_id() {
        let json = r#"[
          {"id":"5396","host":"a.example.net:8080","sponsor":"CT"},
          {"id":"16204","host":"b.example.com:8080","sponsor":"X"}
        ]"#;
        assert_eq!(pick_host_from_json(json, 5396).as_deref(), Some("a.example.net:8080"));
        assert_eq!(pick_host_from_json(json, 16204).as_deref(), Some("b.example.com:8080"));
        assert_eq!(pick_host_from_json(json, 99999), None);
        assert_eq!(pick_host_from_json("not json", 1), None);
    }

    #[test]
    fn speed_result_fields() {
        let r = SpeedResult {
            name: "电信 苏州5G".into(), carrier: Carrier::Telecom,
            latency_ms: Some(12.3),
            down_mbps: 95.0, down_bytes: 50_000_000, down_secs: 4.2,
            up_mbps: 20.0,   up_bytes: 10_000_000,   up_secs: 4.0,
            ok: true,
        };
        assert_eq!(r.carrier.label(), "电信");
        assert!(r.ok && r.latency_ms.unwrap() > 0.0);
    }

    #[test]
    fn render_terminal_has_all_columns() {
        let results = vec![SpeedResult {
            name: "电信 苏州5G".into(), carrier: Carrier::Telecom, latency_ms: Some(11.0),
            down_mbps: 123.4, down_bytes: 50_000_000, down_secs: 3.2,
            up_mbps: 45.6, up_bytes: 20_000_000, up_secs: 3.5, ok: true,
        }, SpeedResult {
            name: "移动 上海5G".into(), carrier: Carrier::Mobile, latency_ms: None,
            down_mbps: 0.0, down_bytes: 0, down_secs: 0.0, up_mbps: 0.0, up_bytes: 0, up_secs: 0.0, ok: false,
        }];
        let out = render_terminal(&results, crate::i18n::Lang::Zh, true);
        assert!(out.contains("电信 苏州5G") && out.contains("123.4"));
        assert!(out.contains("45.6"));
        assert!(out.contains("11"));
        assert!(out.contains("移动") && out.contains("失败"));
    }

    #[test]
    fn render_section_markdown_has_headers() {
        let results = vec![SpeedResult {
            name: "日本 东京".into(), carrier: Carrier::Jp, latency_ms: Some(2.0),
            down_mbps: 88.8, down_bytes: 30_000_000, down_secs: 2.7,
            up_mbps: 30.0, up_bytes: 10_000_000, up_secs: 2.0, ok: true,
        }];
        let out = render_section(&results, crate::i18n::Lang::En);
        assert!(out.contains("Latency") && out.contains("Download") && out.contains("Upload"));
        assert!(out.contains("88.8"));
    }

    #[test]
    fn fmt_mb_works() {
        assert_eq!(fmt_mb(50_000_000), "50.0 MB");
        assert_eq!(fmt_mb(0), "0.0 MB");
    }
}
