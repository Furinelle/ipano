# IP 质量全字段渲染补全(阶段 A)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `MergedReport`/`SourceData` 已有但渲染层漏印的 IP 质量字段补到 `--raw` 逐源详表、默认报告风险面板、JSON 输出三处,对齐融合怪(ecs.sh)的 IP 质量输出完整度。

**Architecture:** 纯渲染层改动。`aggregate.rs` 的 `merge()` 已合并所有字段、`SourceData` 已 `derive(Serialize)`,故**不动 model 与 merge**,只改 `render/raw.rs`(加 `line!` 行)、`render/terminal.rs`(加分布行 + VT 未检出 + Bogon 标记)、`render/json.rs`(加逐源原始数据数组)。TDD:每处先扩测试断言新字段出现,再加渲染代码。

**Tech Stack:** Rust,comfy-table(终端表),serde_json(JSON)。测试用内置 `#[test]` 断言渲染字符串包含目标字段。

**前置事实(实现者必读):**
- `SourceData`(`src/model.rs:23`)含字段:`trust_score: Option<i64>`、`fraud_score: Option<i64>`、`abuseipdb_score: Option<i64>`、`is_tor/is_hosting/is_crawler/is_mobile/is_residential/is_abuser/is_bogon: Option<bool>`、`browser_dist/os_dist/device_dist: Option<String>`、`blacklist_undetected: Option<u32>`。`SourceData` **不含** `ipqs_score`(仅 `MergedReport` 有)。
- `MergedReport`(`src/aggregate.rs:14`)含上述全部字段且 `merge()` 已填充(`pick!` 或 `majority_bool`)。无需改 merge。
- `--raw` 渲染读 `report.raw`(`Vec<SourceData>`,仅成功源);默认报告/JSON 读 `MergedReport` 顶层合并字段。

---

### Task 1: `--raw` 逐源详表补字段

**Files:**
- Modify: `src/render/raw.rs`(在 `render()` 内现有 `line!` 序列末尾、`out` 返回前追加)
- Test: `src/render/raw.rs`(模块内 `#[cfg(test)] mod tests`)

- [ ] **Step 1: 写失败测试**

在 `src/render/raw.rs` 的 `mod tests` 末尾(最后一个 `}` 之前)追加:

```rust
    #[test]
    fn raw_lists_added_quality_fields() {
        let mut a = SourceData::new("ipapiis");
        a.trust_score = Some(72);
        a.fraud_score = Some(15);
        a.abuseipdb_score = Some(3);
        a.is_tor = Some(false);
        a.is_hosting = Some(true);
        a.is_crawler = Some(false);
        a.is_mobile = Some(false);
        a.is_residential = Some(false);
        a.is_abuser = Some(true);
        a.is_bogon = Some(false);
        a.browser_dist = Some("Chrome 78% 其他 22%".into());
        a.os_dist = Some("Windows 93% 其他 7%".into());
        a.blacklist_undetected = Some(91);
        let report = MergedReport { raw: vec![a], ..Default::default() };
        let s = render(&report);
        assert!(s.contains("信任分"));
        assert!(s.contains("72 [ipapiis]"));
        assert!(s.contains("欺诈分"));
        assert!(s.contains("AbuseIPDB分"));
        assert!(s.contains("是否Tor"));
        assert!(s.contains("是否托管"));
        assert!(s.contains("是否爬虫"));
        assert!(s.contains("是否移动"));
        assert!(s.contains("是否住宅"));
        assert!(s.contains("是否滥用者"));
        assert!(s.contains("Yes [ipapiis]"));
        assert!(s.contains("是否Bogon"));
        assert!(s.contains("浏览器分布"));
        assert!(s.contains("Chrome 78% 其他 22% [ipapiis]"));
        assert!(s.contains("系统分布"));
        assert!(s.contains("VT未检出"));
        assert!(s.contains("91 [ipapiis]"));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib render::raw::tests::raw_lists_added_quality_fields`
Expected: FAIL —— 输出不含「信任分」等新标签,断言 `assert!(s.contains("信任分"))` panic。

- [ ] **Step 3: 加渲染行**

在 `src/render/raw.rs` 中,把现有最后一行 `line!("VT可疑", blacklist_suspicious, |v: &u32| format!("{v}"));`(第 32 行)之后、`out` 之前,追加:

```rust
    line!("VT未检出", blacklist_undetected, |v: &u32| format!("{v}"));
    line!("信任分", trust_score, |v: &i64| format!("{v}"));
    line!("欺诈分", fraud_score, |v: &i64| format!("{v}"));
    line!("AbuseIPDB分", abuseipdb_score, |v: &i64| format!("{v}"));
    line!("是否Tor", is_tor, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否托管", is_hosting, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否爬虫", is_crawler, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否移动", is_mobile, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否住宅", is_residential, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否滥用者", is_abuser, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("是否Bogon", is_bogon, |v: &bool| if *v {"Yes"} else {"No"}.to_string());
    line!("浏览器分布", browser_dist, |v: &String| v.clone());
    line!("系统分布", os_dist, |v: &String| v.clone());
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib render::raw`
Expected: PASS（含既有 `raw_lists_phase2_fields`、`raw_lists_per_source` 不回归）。

- [ ] **Step 5: 提交**

```bash
git add src/render/raw.rs
git commit -m "feat(render): --raw 逐源详表补 13 字段(信任/欺诈/AbuseIPDB分·Tor/托管/爬虫/移动/住宅/滥用/Bogon·浏览器/系统分布·VT未检出)"
```

---

### Task 2: 默认报告风险面板补字段

**Files:**
- Modify: `src/render/terminal.rs`（`render()` 风险面板 + `risk_flags()`）
- Test: `src/render/terminal.rs`（模块内 `#[cfg(test)] mod tests`）

- [ ] **Step 1: 写失败测试**

在 `src/render/terminal.rs` 的 `mod tests` 末尾追加:

```rust
    #[test]
    fn render_shows_dist_and_undetected_and_bogon() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut cf = SourceData::new("cf");
        cf.browser_dist = Some("Chrome 78% 其他 22%".into());
        cf.os_dist = Some("Windows 93% 其他 7%".into());
        cf.device_dist = Some("桌面 73% 移动 26%".into());
        let mut vt = SourceData::new("vt");
        vt.blacklist_malicious = Some(0);
        vt.blacklist_undetected = Some(91);
        let mut ipreg = SourceData::new("ipreg");
        ipreg.is_bogon = Some(true);
        let report = crate::aggregate::merge(ip, vec![
            ("cf".into(), Ok(cf)), ("vt".into(), Ok(vt)), ("ipreg".into(), Ok(ipreg)),
        ]);
        let s = render(&report, true, Lang::Zh);
        assert!(s.contains("浏览器分布"));
        assert!(s.contains("系统分布"));
        assert!(s.contains("设备分布"));
        assert!(s.contains("未检出"));
        assert!(s.contains("Bogon"));
    }
```

> 注:测试用 `Lang::Zh`。若该模块测试已 `use` 了 `Lang`/`SourceData`,沿用;否则在测试函数内用全路径 `crate::i18n::Lang::Zh` 与 `crate::model::SourceData::new`。先看文件顶部既有测试的写法对齐。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib render::terminal::tests::render_shows_dist_and_undetected_and_bogon`
Expected: FAIL —— 不含「浏览器分布」「未检出」等。

- [ ] **Step 3: 加渲染**

(3a) 在 `src/render/terminal.rs` 的人机流量行（现第 51-53 行 `if let (Some(h), Some(b)) = ...` 块）之后,追加分布行:

```rust
        if let Some(s) = &r.browser_dist { rt.add_row(vec!["浏览器分布(CF Radar)".to_string(), s.clone()]); }
        if let Some(s) = &r.os_dist { rt.add_row(vec!["系统分布(CF Radar)".to_string(), s.clone()]); }
        if let Some(s) = &r.device_dist { rt.add_row(vec!["设备分布(CF Radar)".to_string(), s.clone()]); }
```

(3b) 把现有 VT 黑名单格式串（现第 45-49 行）改为含未检出:

```rust
            rt.add_row(vec!["VT 黑名单".to_string(), format!(
                "{} 恶意 / {} 可疑 / {} 无害 / {} 未检出",
                r.blacklist_malicious.unwrap_or(0),
                r.blacklist_suspicious.unwrap_or(0),
                r.blacklist_harmless.unwrap_or(0),
                r.blacklist_undetected.unwrap_or(0))]);
```

(3c) 在 `risk_flags()`（现第 100 行 `if r.is_cloud == Some(true) { f.push("云"); }` 一带）追加 Bogon 标记,放在 `is_anonymous` 判断之后:

```rust
    if r.is_bogon == Some(true) { f.push("Bogon"); }
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib render::terminal`
Expected: PASS（既有 `render_shows_blacklist_and_traffic` 等不回归；注意它若断言旧 VT 串「无害」仍包含,改后仍含,不受影响）。

- [ ] **Step 5: 提交**

```bash
git add src/render/terminal.rs
git commit -m "feat(render): 默认报告风险面板补浏览器/系统/设备分布行、VT未检出数、Bogon 标记"
```

---

### Task 3: JSON 暴露逐源原始数据

**Files:**
- Modify: `src/render/json.rs`（`to_json()` 的 `json!` 对象加键）
- Test: `src/render/json.rs`（模块内 `#[cfg(test)] mod tests`）

- [ ] **Step 1: 写失败测试**

在 `src/render/json.rs` 的 `mod tests` 末尾追加:

```rust
    #[test]
    fn json_contains_per_source_raw() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut a = SourceData::new("ipapiis");
        a.usage_type = Some("hosting".into());
        a.is_tor = Some(false);
        a.browser_dist = Some("Chrome 78%".into());
        let report = merge(ip, vec![("ipapiis".to_string(), Ok(a))]);
        let s = to_json(&report, &[], &[], &[], &[], &[]);
        assert!(s.contains("sources_data"));
        assert!(s.contains("\"source_id\""));
        assert!(s.contains("\"ipapiis\""));
        assert!(s.contains("\"browser_dist\""));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib render::json::tests::json_contains_per_source_raw`
Expected: FAIL —— JSON 不含 `sources_data`。

- [ ] **Step 3: 加 JSON 键**

在 `src/render/json.rs` 的 `json!({ ... })` 对象里,`"sources": sources,`（现第 61 行）之后追加一行:

```rust
        "sources_data": r.raw,
```

> `r.raw` 是 `Vec<SourceData>`,`SourceData` 已 `derive(Serialize)`（`src/model.rs:22`），`serde_json::json!` 宏直接接受。`sources`（状态:id/ok/error,含失败源）与 `sources_data`（成功源完整原始字段）并存,各司其职。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib render::json`
Expected: PASS（既有 4 个 json 测试不回归）。

- [ ] **Step 5: 提交**

```bash
git add src/render/json.rs
git commit -m "feat(render): JSON 增 sources_data 数组逐源输出完整 SourceData 原始字段"
```

---

### Task 4: 全量回归 + 文档

**Files:**
- Modify: `CHANGELOG.md`（顶部加条目）

- [ ] **Step 1: 全量测试**

Run: `cargo test`
Expected: PASS,数量 ≥ 234 +3（本计划新增 3 测试 = 237)，无失败。

- [ ] **Step 2: 真机抽验渲染（可选,本地有源即可）**

Run: `cargo build --release && ./target/release/ipano 1.1.1.1 --raw | grep -E "是否Tor|浏览器分布|VT未检出|信任分"`
Expected: 至少能看到这些新标签行（取决于哪些免费源对 1.1.1.1 返回了对应字段;无值的行不打印属正常）。

- [ ] **Step 3: 加 CHANGELOG 条目**

在 `CHANGELOG.md` 顶部（最新版块之上)插入:

```markdown
## [未发布] v0.19.0 进行中

### 阶段 A — IP 质量全字段渲染补全
- `--raw` 逐源详表补 13 字段:信任/欺诈/AbuseIPDB 分、是否 Tor/托管/爬虫/移动/住宅/滥用者/Bogon、浏览器/系统分布、VT 未检出数。
- 默认报告风险面板补:浏览器/系统/设备分布行、VT 黑名单未检出计数、Bogon 标记。
- JSON 新增 `sources_data` 数组,逐源输出完整 `SourceData` 原始字段。
- model 与 merge 无改动(字段早已合并,本阶段纯渲染补齐)。
```

> 版本号 `Cargo.toml` **本阶段不 bump**:v0.19.0 含阶段 A/B/C,待全部完成再统一发布。CHANGELOG 用「未发布 v0.19.0 进行中」标记累积。

- [ ] **Step 4: 提交**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): 记录 v0.19.0 阶段 A 渲染补全"
```

---

## Self-Review

- **Spec 覆盖**:spec item 1 三处输出(--raw / 默认 / JSON)分别由 Task 1 / 2 / 3 覆盖。spec 提「需动 aggregate.rs merge」一句**作废**——实读 `aggregate.rs:94-113` 确认字段已全部合并,本计划不动 merge(plan 优先于 spec)。
- **占位扫描**:无 TBD/TODO;每步含完整代码块与确切命令。
- **类型一致**:`line!` 闭包参数类型与 `SourceData` 字段类型对齐(i64/bool/String/u32);`r.raw: Vec<SourceData>` 与 `Serialize` 一致;测试用 `merge()`/`SourceData::new` 与现有测试同款。
- **范围**:仅渲染层,零网络,自包含可独立交付与回归;不含 item 2 探针(另开阶段 B/C plan)。
