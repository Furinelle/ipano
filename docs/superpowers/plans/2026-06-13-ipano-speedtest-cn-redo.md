# 三网回国 + 国际测速重做 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `--speedtest` 从国际 CDN 下载改为对 speedtest.net 三网/国际服务器测「延迟+下载+上传」,全节点目录可选、host 运行时解析。

**Architecture:** 纯 Rust 打 speedtest.net 服务器 HTTP 端点(`/latency.txt` `/download` `/upload`)。内置 ~40 节点目录(server id + 城市搜索词 + 运营商);运行时按城市词调 speedtest.net `search` API、按 id 匹配出 host 再测。`--speedtest[=SPEC]` 选节点(`cn`/`ct`/`cu`/`cm`/`hk`/`edu`/`intl`/`us`/`jp`/`sg`/`all`/id 列表/`list`),默认 6 代表。

**Tech Stack:** Rust, tokio, reqwest, serde, comfy-table, toml。设计见 [`docs/superpowers/specs/2026-06-13-ipano-speedtest-cn-redo-design.md`](../specs/2026-06-13-ipano-speedtest-cn-redo-design.md)。

**约定:** 每个 Task 跑 `cargo test speedtest` 或指定测试名;全绿后 `git add` 相关文件 + commit。本计划重写 `src/probe/speedtest.rs`,旧版整文件替换(旧 `default_nodes`/`SpeedNode{name,url}` 模型废弃)。

---

## File Structure

| 文件 | 职责 | 改动 |
|---|---|---|
| `src/probe/speedtest.rs` | 数据模型(Carrier/SpeedNode/Selection/SpeedResult)、catalog、parse_spec、host 解析、probe_one、run_all、渲染 | 整体重写 |
| `src/cli.rs:38-41` | `--speedtest` bool → `Option<String>` | 改字段 + help |
| `src/config.rs:28-32` | `speedtest: Option<Vec<SpeedNode>>` → `Option<SpeedtestCfg>{spec, custom}` | 改 schema + 测试 |
| `src/main.rs:86-92` | 调度:解析 spec → 节点 → 解析 host → run;`list` 分支 | 改调度块 |
| `src/render/json.rs` | `SpeedResult` 新字段自动序列化(无需改 to_json 签名) | 仅随模型变 |
| `README.md` / `CHANGELOG.md` / `Cargo.toml` | 文档 + 版本 0.16.0 | 改 |

---

## Task 1: 数据模型 + 目录 + parse_spec(纯函数,TDD)

**Files:**
- Modify: `src/probe/speedtest.rs`(顶部数据模型 + catalog + parse_spec,替换旧 `SpeedNode`/`default_nodes`)
- Test: 同文件 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试** — 在 `src/probe/speedtest.rs` 的 `mod tests` 中替换旧的 `default_nodes_have_four`、新增 parse_spec/catalog 测试:

```rust
#[test]
fn catalog_is_sane() {
    let c = catalog();
    assert!(c.len() >= 40, "目录至少 40 个节点, got {}", c.len());
    // 无重复 id(0 是自定义占位,不计)
    let mut ids: Vec<u32> = c.iter().map(|n| n.id).filter(|&i| i != 0).collect();
    let n = ids.len();
    ids.sort_unstable(); ids.dedup();
    assert_eq!(ids.len(), n, "目录 id 不应重复");
    // 默认集恰 6 个
    assert_eq!(c.iter().filter(|n| n.default).count(), 6, "默认集应为 6");
    // 每节点要么有 host 要么有 search 词
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
    // cn 已含 5396,再加 5396 不应重复
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib speedtest 2>&1 | head -30`
Expected: 编译失败(`Selection`/`catalog`/`parse_spec` 未定义,旧 `SpeedNode` 字段不符)

- [ ] **Step 3: 替换数据模型 + 实现 catalog/parse_spec** — 替换 `src/probe/speedtest.rs` 顶部(旧 `SpeedNode` 结构、`default_nodes`)为:

```rust
use std::time::Instant;
use serde::{Serialize, Deserialize};
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
    // (id, name, carrier, search, default)
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
        (25858, "移动 北京",        Mobile,  "beijing",     false),
        (4575,  "移动 成都",        Mobile,  "chengdu",     false),
        (41910, "移动 郑州5G",      Mobile,  "zhengzhou",   false),
        (16171, "移动 福州",        Mobile,  "fuzhou",      false),
        (26940, "移动 银川5G",      Mobile,  "yinchuan",    false),
        (53087, "移动 深圳",        Mobile,  "shenzhen",    false),
        (54312, "移动 杭州",        Mobile,  "hangzhou",    false),
        (16145, "移动 兰州",        Mobile,  "lanzhou",     false),
        (29105, "移动 西安",        Mobile,  "xi'an",       false),
        (37639, "香港 CMHK Broadband", Hk,   "hong kong",   false),
        (13538, "香港 CSL",         Hk,      "hong kong",   false),
        (32155, "香港 CMHK Mobile", Hk,      "hong kong",   false),
        (30852, "教育网 江苏昆山",  Edu,     "kunshan",     false),
        (35527, "广电 四川成都",    Edu,     "chengdu",     false),
        (43201, "美国 洛杉矶",      Us,      "los angeles", false),
        (17846, "美国 圣何塞",      Us,      "san jose",    false),
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
    let mut push = |node: &SpeedNode, out: &mut Vec<SpeedNode>| {
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
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib speedtest::tests::parse_spec 2>&1 | tail -15 && cargo test --lib speedtest::tests::catalog_is_sane 2>&1 | tail -5`
Expected: 全 PASS。(此时旧的 `SpeedResult`/渲染/`run_all` 仍引用旧 `SpeedNode` 字段,会编译错 —— 由 Task 3/5 修复;只跑这两个纯测试可借助 `--lib` 但若整 crate 编译不过需先做 Task 3。**实际执行顺序:Task 1 写代码 → Task 3 接着改 SpeedResult/run_all 再统一编译**。)

- [ ] **Step 5: 暂不 commit**(等 Task 3 让 crate 可编译后一起提交)

---

## Task 2: host 解析(JSON 匹配纯函数 + 异步取数)

**Files:**
- Modify: `src/probe/speedtest.rs`
- Test: 同文件

- [ ] **Step 1: 写失败测试** — 解析 search API JSON、按 id 取 host:

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib speedtest::tests::pick_host 2>&1 | tail -10`
Expected: FAIL(`pick_host_from_json` 未定义)

- [ ] **Step 3: 实现解析 + 异步取数** — 在 `src/probe/speedtest.rs` 加:

```rust
/// 从 speedtest.net search API 的 JSON 数组里按 id 取 host(纯函数)
fn pick_host_from_json(body: &str, id: u32) -> Option<String> {
    let arr: serde_json::Value = serde_json::from_str(body).ok()?;
    for item in arr.as_array()? {
        // id 字段是字符串
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
    let mut cache: HashMap<String, String> = HashMap::new(); // search词 → 该词 API 原始 JSON
    let mut out = Vec::with_capacity(nodes.len());
    for n in nodes {
        if let Some(h) = &n.host { out.push(Some(h.clone())); continue; } // 自定义节点直接用
        let body = match cache.get(&n.search) {
            Some(b) => b.clone(),
            None => {
                let url = format!(
                    "https://www.speedtest.net/api/js/servers?engine=js&limit=100&search={}",
                    urlencoding::encode(&n.search));
                let body = client.get(&url)
                    .header("Referer", "https://www.speedtest.net/")
                    .send().await.ok()
                    .and_then(|r| futures::executor::block_on(async { r.text().await.ok() }));
                // 注:实现时用 .await 而非 block_on,见下方说明
                let body = body.unwrap_or_default();
                cache.insert(n.search.clone(), body.clone());
                body
            }
        };
        out.push(pick_host_from_json(&body, n.id));
    }
    out
}
```

> **实现说明(避免占位陷阱):** 上面 `resolve_hosts` 内的取 body 应直接用 `.await`,不要 `block_on`。正确写法:
> ```rust
> let body = match client.get(&url).header("Referer","https://www.speedtest.net/").send().await {
>     Ok(r) => r.text().await.unwrap_or_default(),
>     Err(_) => String::new(),
> };
> ```
> `urlencoding` 若未在依赖中,改用 `n.search.replace(' ', "%20")`(城市词仅含空格)。本计划 Task 4 Step 0 处理依赖。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib speedtest::tests::pick_host 2>&1 | tail -8`
Expected: PASS(`pick_host_from_json` 纯函数可单独编译测;若 crate 因 Task 3 未完成而编译不过,合并到 Task 3 后统一验证)

- [ ] **Step 5: 暂不 commit**

---

## Task 3: SpeedResult 模型 + probe_one + run_all(让 crate 重新可编译)

**Files:**
- Modify: `src/probe/speedtest.rs`(替换旧 `SpeedResult`、`download_one`、`run_all`)
- Test: 同文件

- [ ] **Step 1: 写失败测试** — calc 保留,新增 SpeedResult 构造与默认:

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib speedtest 2>&1 | head -20`
Expected: 编译错(旧 `SpeedResult` 字段不符)

- [ ] **Step 3: 替换 SpeedResult + 探测逻辑** — 替换 `src/probe/speedtest.rs` 中旧的 `SpeedResult`/`download_one`/`run_all`:

```rust
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

/// 上传:POST /upload,流式发送零缓冲到 deadline
async fn measure_upload(client: &reqwest::Client, host: &str, max_secs: u64) -> (f64, u64, f64) {
    use futures::stream;
    let url = format!("http://{host}/upload?nocache={}", rand_tag());
    // 1MB 块的无限流,靠 total timeout 截断
    let chunk = bytes::Bytes::from(vec![0u8; 1_000_000]);
    let body = reqwest::Body::wrap_stream(stream::repeat_with(move || {
        Ok::<_, std::io::Error>(chunk.clone())
    }).take(max_secs as usize * 200)); // 上限远大于 deadline 实际能发的量
    let start = Instant::now();
    let sent = match timeout(Duration::from_secs(max_secs),
        client.post(&url).header("Content-Type", "application/octet-stream").body(body).send()).await
    {
        Ok(Ok(_)) | Err(_) => 0, // 简化:见下方说明用计数包装
        Ok(Err(_)) => 0,
    };
    let secs = start.elapsed().as_secs_f64();
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
```

> **上传计字节(避免占位):** 上面 `measure_upload` 的 `sent` 需真实统计已发字节。用 `stream` + 原子计数器包装每个 chunk:
> ```rust
> use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
> let counter = Arc::new(AtomicU64::new(0));
> let c2 = counter.clone();
> let body = reqwest::Body::wrap_stream(
>     stream::repeat_with(move || { c2.fetch_add(1_000_000, Ordering::Relaxed);
>         Ok::<_, std::io::Error>(bytes::Bytes::from(vec![0u8; 1_000_000])) })
>     .take(2000));
> // ... send with timeout ...
> let sent = counter.load(Ordering::Relaxed);
> ```
> 注:reqwest 把 body 全部交给传输层后 send 才返回;timeout 截断时 send future 被 drop,已 `fetch_add` 的计数即近似已发量。若服务器拒绝 upload,`sent` 仍 >0 但 `up_mbps` 反映很小 —— 可接受。

- [ ] **Step 4: 跑全部 speedtest 测试**

Run: `cargo test --lib speedtest 2>&1 | tail -20`
Expected: Task 1/2/3 所有纯函数测试 PASS,crate 编译通过(渲染/json 暂可能引用旧字段 → 若报错进 Task 5/6 修)

- [ ] **Step 5: 依赖检查** — 确认 `Cargo.toml` 有 `futures`、`bytes`、`serde_json`、`urlencoding`(或改用 `replace`)。缺则 `cargo add futures bytes`(`serde_json` 通常已有)。

```bash
grep -E 'futures|bytes|serde_json|urlencoding' Cargo.toml
```

- [ ] **Step 6: Commit(模型+探测+解析三件一起)**

```bash
git add src/probe/speedtest.rs Cargo.toml Cargo.lock
git commit -m "feat(speedtest): 三网/国际节点目录 + 运行时 host 解析 + 延迟/下载/上传探测"
```

---

## Task 4: 渲染(终端 + markdown,新列 延迟/下载/上传,TDD)

**Files:**
- Modify: `src/probe/speedtest.rs`(`render_terminal`/`render_section`/`speed_color`/`fmt_mb`)
- Test: 同文件

- [ ] **Step 1: 写失败测试** — 替换旧 `render_terminal_no_color_plain`/`render_section_markdown`:

```rust
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
    assert!(out.contains("45.6"));         // 上传
    assert!(out.contains("11")); // 延迟
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib speedtest::tests::render 2>&1 | tail -12`
Expected: FAIL(旧渲染缺延迟/上传列)

- [ ] **Step 3: 替换渲染函数** — 替换 `src/probe/speedtest.rs` 的 `fmt_mb`/`speed_color`/`render_terminal`/`render_section`:

```rust
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
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib speedtest 2>&1 | tail -15`
Expected: 全 PASS

- [ ] **Step 5: Commit**

```bash
git add src/probe/speedtest.rs
git commit -m "feat(speedtest): 渲染加运营商/延迟/上传列,终端+markdown"
```

---

## Task 5: CLI `--speedtest` 改可选带值

**Files:**
- Modify: `src/cli.rs:38-41`

- [ ] **Step 1: 改字段** — 替换 `src/cli.rs` 的 speedtest 字段:

```rust
    /// 多节点测速(对 speedtest.net 三网/国际节点测 延迟+下载+上传,从本机出口发起);
    /// 不带值=默认 6 代表;可选 SPEC: all/cn/ct/cu/cm/hk/edu/intl/us/jp/sg/list 或 server id 列表(逗号分隔)。
    /// 会消耗较多流量,故不含在 --all 内
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    pub speedtest: Option<String>,
```

- [ ] **Step 2: 编译检查(main.rs 会报错,Task 7 修)**

Run: `cargo build 2>&1 | grep -E 'speedtest|error\[' | head`
Expected: `main.rs` 内 `if args.speedtest` 类型不符报错 —— 预期,Task 7 修复。

- [ ] **Step 3: 暂不 commit**(与 Task 6/7 一起)

---

## Task 6: 配置 schema(`[speedtest] spec` + `[[speedtest.custom]]`)

**Files:**
- Modify: `src/config.rs:28-32`(+ 测试)

- [ ] **Step 1: 写失败测试** — 在 `config.rs` 的 `mod tests` 加:

```rust
#[test]
fn config_parses_speedtest() {
    let src = r#"
[speedtest]
spec = "cn"
[[speedtest.custom]]
name = "自建"
carrier = "telecom"
host = "speedtest.example.com:8080"
"#;
    let c: Config = toml::from_str(src).unwrap();
    let st = c.speedtest.unwrap();
    assert_eq!(st.spec.as_deref(), Some("cn"));
    assert_eq!(st.custom.unwrap()[0].host, "speedtest.example.com:8080");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --lib config::tests::config_parses_speedtest 2>&1 | tail -10`
Expected: FAIL(`SpeedtestCfg` 未定义 / 旧字段不符)

- [ ] **Step 3: 改 schema** — 替换 `src/config.rs:28-32` 的字段 + 新增结构:

```rust
    /// 测速配置;例:
    /// [speedtest]
    /// spec = "cn"          # 默认选择(同 --speedtest SPEC 语法)
    /// [[speedtest.custom]] # 追加目录外的 Ookla 节点
    /// name = "自建"
    /// carrier = "telecom"
    /// host = "speedtest.example.com:8080"
    pub speedtest: Option<SpeedtestCfg>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SpeedtestCfg {
    pub spec: Option<String>,
    pub custom: Option<Vec<CustomNode>>,
}

#[derive(Debug, Deserialize)]
pub struct CustomNode {
    pub name: String,
    pub carrier: String,   // telecom/unicom/mobile/edu/hk/us/jp/sg
    pub host: String,
```

> 注:闭合大括号 —— 原 `Config` 结构体的 `}` 保留在 `speedtest` 字段后,`SpeedtestCfg`/`CustomNode` 紧随其后。`CustomNode` 的 `}` 也要补上。

并在 `speedtest.rs` 加 carrier 字符串解析(供 main 转换 custom 节点):

```rust
impl Carrier {
    pub fn from_str_lenient(s: &str) -> Carrier {
        match s.to_ascii_lowercase().as_str() {
            "unicom" | "cu" => Carrier::Unicom, "mobile" | "cm" => Carrier::Mobile,
            "edu" => Carrier::Edu, "hk" => Carrier::Hk,
            "us" => Carrier::Us, "jp" => Carrier::Jp, "sg" => Carrier::Sg,
            _ => Carrier::Telecom, // 默认电信
        }
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --lib config::tests 2>&1 | tail -12`
Expected: 全 PASS(`config::tests` 全部)

- [ ] **Step 5: 暂不 commit**(与 Task 7 一起)

---

## Task 7: main.rs 调度 + `list` 分支 + CLI/config 优先级

**Files:**
- Modify: `src/main.rs:86-92`

- [ ] **Step 1: 改调度块** — 替换 `src/main.rs:86-92`:

```rust
    // 多节点测速:CLI --speedtest SPEC 优先,其次配置 [speedtest].spec;list 打印目录后退出
    let speedtest = {
        let cli_spec = args.speedtest.clone();
        let cfg_st = cfg.speedtest;
        let spec = cli_spec.or_else(|| cfg_st.as_ref().and_then(|s| s.spec.clone()));
        match spec {
            None => Vec::new(), // 既没传 --speedtest 也没配 spec → 不跑
            Some(spec) => {
                let mut cat = probe::speedtest::catalog();
                // 追加配置里的自定义节点
                if let Some(customs) = cfg_st.and_then(|s| s.custom) {
                    for cu in customs {
                        cat.push(probe::speedtest::SpeedNode {
                            id: 0, name: cu.name,
                            carrier: probe::speedtest::Carrier::from_str_lenient(&cu.carrier),
                            search: String::new(), host: Some(cu.host), default: false,
                        });
                    }
                }
                match probe::speedtest::parse_spec(&spec, &cat) {
                    Ok(probe::speedtest::Selection::List) => {
                        print!("{}", probe::speedtest::render_catalog(&cat, lang));
                        return Ok(());
                    }
                    Ok(probe::speedtest::Selection::Nodes(nodes)) => probe::speedtest::run_all(&nodes).await,
                    Err(e) => { eprintln!("--speedtest: {e}"); std::process::exit(2); }
                }
            }
        }
    };
```

> 注:`main` 返回类型须支持 `return Ok(())`(现有 `main` 返回 `anyhow::Result<()>` 或类似 —— 实现时确认;若 `main` 非 Result,改 `return;` 并调整)。`lang` 变量在此作用域已存在(渲染用)。

- [ ] **Step 2: 加目录渲染函数** — 在 `src/probe/speedtest.rs` 加:

```rust
/// `--speedtest=list`:打印完整目录(id/运营商/名称/搜索词)
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
```

- [ ] **Step 3: 全量编译 + 测试**

Run: `cargo build 2>&1 | tail -5 && cargo test 2>&1 | tail -15`
Expected: 编译通过,所有测试 PASS

- [ ] **Step 4: 手动冒烟(list 不需网络)**

Run: `cargo run -- --speedtest=list 1.1.1.1 2>&1 | head -20`
Expected: 打印节点目录表(含电信/联通/移动/香港/美/日/新 各节点);未知关键字测:`cargo run -- --speedtest=mars 1.1.1.1; echo $?` → 退出码 2 + 错误提示。

- [ ] **Step 5: Commit(CLI+config+main 一起)**

```bash
git add src/cli.rs src/config.rs src/main.rs src/probe/speedtest.rs
git commit -m "feat(speedtest): --speedtest 可选 SPEC + list 目录 + 配置 spec/custom"
```

---

## Task 8: 文档 + 版本 0.16.0

**Files:**
- Modify: `README.md`(功能说明 + 路线图 P15 行 + 诚实标注)、`CHANGELOG.md`、`Cargo.toml`(version)

- [ ] **Step 1: 改 Cargo.toml 版本** — `version = "0.15.0"` → `version = "0.16.0"`

- [ ] **Step 2: CHANGELOG.md 加条目**(顶部):

```markdown
## [0.16.0] - 2026-06-13

### Changed (破坏性)
- **`--speedtest` 重做为三网回国 + 国际测速**:不再下载国际 CDN,改为对 speedtest.net 三网(电信/联通/移动)+ 香港 + 教育/广电 + 国际(美/日/新)节点测 **延迟 + 下载 + 上传**。host 运行时按 server id 解析(适配各 vantage)。
  - 选择:`--speedtest`(默认 6 代表)/ `=cn`/`ct`/`cu`/`cm`/`hk`/`edu`/`intl`/`us`/`jp`/`sg`/`all` / server id 列表 / `=list` 看目录,逗号可组合。
  - **配置变更**:旧 `[[speedtest]] {name,url}` 移除,改 `[speedtest] spec = "..."` + `[[speedtest.custom]] {name,carrier,host}`。
- 单连接单流测速,结果仅供参考。
```

- [ ] **Step 3: README 更新** — `--speedtest` 段落、路线图 P15 行(改述为「三网回国+国际测速」)、`--speedtest=list` 用法、诚实标注(单连接/运行时解析/方向)。具体替换 README 中含 `Cachefly`/`P15`/`多节点下载测速` 的描述为新版描述。

```bash
grep -n "speedtest\|Cachefly\|P15\|多节点" README.md
```
逐处把「顺序下载 Cachefly/Linode…」替换为「对 speedtest.net 三网/国际节点测延迟+下载+上传;`--speedtest=list` 看全部可选节点」。

- [ ] **Step 4: 验证文档无残留旧描述**

Run: `grep -n "Cachefly\|Linode\|ThinkBroadband" README.md CHANGELOG.md`
Expected: README 中无残留(CHANGELOG 历史条目保留旧描述属正常)

- [ ] **Step 5: 最终全量验证**

Run: `cargo build --release 2>&1 | tail -3 && cargo test 2>&1 | tail -8`
Expected: release 编译通过,测试全绿

- [ ] **Step 6: Commit**

```bash
git add README.md CHANGELOG.md Cargo.toml Cargo.lock
git commit -m "docs(speedtest): README/CHANGELOG 同步三网测速重做(v0.16.0)"
```

---

## Task 9: 联网冒烟测试(人工 / 可选)

> 真实测速需网络且结果随 vantage 变化,不进自动化测试。建议人工跑一次确认端到端。

- [ ] **Step 1: 默认集**

Run: `cargo run -- --speedtest 1.1.1.1 2>&1 | tail -20`
Expected: 6 行结果表;若本机在海外,国内移动/联通节点可能解析失败(显示「失败」)属预期 —— 在中国/亚洲 VPS 上跑应正常。

- [ ] **Step 2: 分组 + 国际**

Run: `cargo run -- --speedtest=jp,sg 1.1.1.1 2>&1 | tail -10`
Expected: 日本东京 + 新加坡 两节点延迟/下载/上传(海外 vantage 下国际节点应能解析成功)

- [ ] **Step 3: JSON**

Run: `cargo run -- --speedtest=jp --json 1.1.1.1 2>&1 | python3 -m json.tool | grep -A12 speedtest`
Expected: `speedtest[]` 含 `name/carrier/latency_ms/down_mbps/up_mbps/...` 字段

---

## Self-Review(写完计划核对 spec)

- **Spec 覆盖**:测速实现(Task 3)✓ / 三指标(Task 3)✓ / 全目录可选(Task 1)✓ / 国际节点(Task 1)✓ / 运行时解析(Task 2)✓ / list(Task 7)✓ / 配置变更(Task 6)✓ / 渲染三列(Task 4)✓ / JSON(Task 3 Serialize 自动 + Task 9 验证)✓ / 诚实标注(Task 8)✓ / 默认 6(Task 1)✓。无遗漏。
- **占位符**:`measure_upload` 的字节计数、`resolve_hosts` 的 `.await` 写法已用「实现说明」给出确切替代,非 TODO。`main` 返回类型在 Task 7 注明需确认。
- **类型一致**:`SpeedNode{id,name,carrier,search,host,default}` 跨 Task 1/3/6/7 一致;`Selection{List,Nodes}` Task 1/7 一致;`SpeedResult` 字段 Task 3/4 一致;`Carrier::label/group/from_str_lenient` 定义于 Task 1/6,使用于 4/7。
