# 设计:`--speedtest` 重做为三网回国 + 国际测速

- **日期**:2026-06-13
- **状态**:设计已确认,待写实现计划
- **目标版本**:v0.16.0(含破坏性配置变更)
- **对标**:`https://down.wangchao.info/sh/superspeed.sh`(Ookla `speedtest` CLI,三网各城市 server ID)

## 背景与问题

当前 `--speedtest`(P15,v0.15.0)对一批**国际 CDN**(Cachefly / Linode 东京 / Linode 美西 / ThinkBroadband 英国)做纯 HTTP GET 下载,测的是**国际出口带宽**。

但 ipano 对标的 ecs.sh / superspeed.sh,核心诉求是 **VPS 的回国质量**——即从本机出口到国内**三大运营商(电信/联通/移动)**节点的延迟与速率。当前实现方向与诉求相反。

superspeed.sh 用 Ookla 官方 `speedtest` 二进制,对一批 **speedtest.net server ID**(三网各城市,如 3633 上海电信、5396 苏州5G电信、24447 上海5G联通、25637 上海5G移动)测 **ping + 下载 + 上传**。

## 决策记录

| 议题 | 选择 | 理由 / 否决项 |
|---|---|---|
| 测速实现方式 | **纯 Rust 打 speedtest.net 服务器 HTTP 端点** | 保持 ipano 纯 Rust 单 musl 静态二进制设计(无外部二进制依赖)。否决「调用 Ookla CLI」(引入外部二进制 + license + 平台相关,破坏单二进制);否决「三网静态镜像」(找不到清晰标注三网各市的稳定大文件镜像,节点权威性弱)。 |
| 测量指标 | **延迟 + 下载 + 上传**(三项,对齐 superspeed.sh) | 回国质量看延迟+带宽两个维度;上传也测,与参考脚本一致。 |
| 节点范围 | **完整目录,用户自由选择**;含国际(美/日/新) | 不止固定几个;三网各城市 + 香港 + 教育网 + 国际节点全部可选。 |
| host 解析 | **运行时从 VPS 调 search API,按 id 匹配** | host 随 vantage 变化且会轮换,**无法可靠钉死**(实测在美国 vantage 拿不全节点,移动节点全军覆没;VPS 在中国/亚洲 vantage 则能解析到本地节点)。VPS 测速本就联网,多一次 API 调用无负担;真实测速工具(Ookla CLI / speedtest-cli)同样**运行时拉服务器列表**。否决「钉死 host」(物理拿不全 + 怕轮换);否决「Ookla CLI」(外部二进制依赖)。 |
| 默认 `--speedtest` | **国内 6 代表**(回国为核心,国际一个 flag 即可) | 否决「默认带国际节点」(回国是主诉求,默认应聚焦国内)。 |

## 可行性验证(已完成,只读探测)

speedtest.net 端点实测(2026-06-13):

1. **节点解析**:`GET https://www.speedtest.net/api/js/servers?engine=js&search=<城市>&limit=N`,**必须带 `Referer: https://www.speedtest.net/`**(否则 403)。返回 JSON 数组,每项含 `id` / `sponsor` / `name` / `cc` / `host` / `https_functional`。`servers=<ids>` 参数**不过滤**(返回最近节点),故按 `search=城市` 查、再按 `id` 匹配。中文搜索返回空,只能用英文城市名。
2. **延迟**:`GET http://<host>/latency.txt`(10 字节,200)或 `/hello`(44 字节)→ 多次取最小。
3. **下载**:`GET http://<host>/download?nocache=<rand>&size=<N>` → 200,返回精确 N 字节(下载量由 size 控制)。
4. **上传**:`POST http://<host>/upload?nocache=<rand>`,流式 body → 200,接收全部字节。

> host 形如 `4gsuzhou1.speedtest.jsinfo.net:8080`(运营商自有域名,较稳)或 `*.prod.hosts.ooklaserver.net:8080`(Ookla CDN 别名)。**目录钉死用运营商自有 host**(更稳)。

## 架构

测速整体仍在 `src/probe/speedtest.rs`,内部按职责拆分:

```
catalog()         -> &'static [SpeedNode]      节点目录(钉死)
parse_spec(&str)  -> Vec<SpeedNode>            CLI/config 选择 → 节点列表
probe_one(node)   -> SpeedResult               单节点 延迟→下载→上传
run_all(nodes)    -> Vec<SpeedResult>          串行跑(避免抢带宽)
render_terminal / render_section / json        渲染
```

### 1. 节点目录(server id + 搜索词,host 运行时解析)

目录不存 host,只存 `{id, name, carrier, search}` + 是否属默认集。host 运行时按 `search` 城市词查 API、按 id 匹配得到。

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Carrier { Telecom, Unicom, Mobile, Edu, Hk, Us, Jp, Sg }

pub struct SpeedNode {
    pub id: u32,             // speedtest.net server id(自选 / 运行时匹配用)
    pub name: &'static str,  // 显示名,如 "电信 江苏苏州5G"
    pub carrier: Carrier,
    pub search: &'static str,// 解析 host 用的城市搜索词(英文),如 "suzhou"
    pub default: bool,       // 是否属 --speedtest 默认 6 代表
}
```

**完整目录**(来源:superspeed.sh 三网全集 + 香港 + 教育/广电 + 国际;`默认` 列 ✓ 为默认 6 代表):

| id | carrier | name | search | 默认 |
|---|---|---|---|---|
| 3633 | Telecom | 电信 上海 | shanghai | |
| 27594 | Telecom | 电信 广州 | guangzhou | |
| 34115 | Telecom | 电信 天津5G | tianjin | |
| 17145 | Telecom | 电信 合肥5G | hefei | |
| 5396 | Telecom | 电信 江苏苏州5G | suzhou | ✓ |
| 5317 | Telecom | 电信 扬州5G | yangzhou | |
| 36663 | Telecom | 电信 镇江5G | zhenjiang | |
| 29071 | Telecom | 电信 成都 | chengdu | |
| 29353 | Telecom | 电信 武汉5G | wuhan | |
| 28225 | Telecom | 电信 长沙5G | changsha | |
| 3973 | Telecom | 电信 兰州 | lanzhou | |
| 34988 | Telecom | 电信 沈阳5G | shenyang | |
| 59386 | Telecom | 电信 浙江杭州 | hangzhou | ✓ |
| 24447 | Unicom | 联通 上海5G | shanghai | ✓ |
| 54625 | Unicom | 联通 南昌 | nanchang | |
| 45170 | Unicom | 联通 无锡 | wuxi | |
| 4884 | Unicom | 联通 福州 | fuzhou | |
| 36646 | Unicom | 联通 郑州5G | zhengzhou | |
| 37235 | Unicom | 联通 沈阳 | shenyang | |
| 43752 | Unicom | 联通 北京 | beijing | ✓ |
| 25637 | Mobile | 移动 上海5G | shanghai | ✓ |
| 6715 | Mobile | 移动 杭州5G | hangzhou | |
| 26404 | Mobile | 移动 合肥5G | hefei | |
| 25858 | Mobile | 移动 北京 | beijing | |
| 4575 | Mobile | 移动 成都 | chengdu | |
| 41910 | Mobile | 移动 郑州5G | zhengzhou | |
| 16171 | Mobile | 移动 福州 | fuzhou | |
| 26940 | Mobile | 移动 银川5G | yinchuan | |
| 53087 | Mobile | 移动 深圳 | shenzhen | |
| 54312 | Mobile | 移动 杭州 | hangzhou | |
| 16145 | Mobile | 移动 兰州 | lanzhou | |
| 29105 | Mobile | 移动 西安 | xi'an | |
| 37639 | Hk | 香港 CMHK Broadband | hong kong | |
| 13538 | Hk | 香港 CSL | hong kong | |
| 32155 | Hk | 香港 CMHK Mobile | hong kong | |
| 30852 | Edu | 教育网 江苏昆山 | kunshan | |
| 35527 | Edu | 广电 四川成都 | chengdu | |
| 43201 | Us | 美国 洛杉矶 | los angeles | |
| 17846 | Us | 美国 圣何塞 | san jose | |
| 48463 | Jp | 日本 东京 | tokyo | |
| 31293 | Sg | 新加坡 | singapore | |

> 默认 6 代表 = 用户点名集:香港(37639)· 联通上海5G(24447)· 联通北京(43752)· 电信苏州5G(5396)· 电信浙江(59386)· 移动上海5G(25637,代表东部移动 —— speedtest.net 无「苏州移动」服务器,superspeed.sh 里也没有)。
>
> 国际节点 id 已用 `search=` 验证(43201 x99.cloud LA · 17846 Sonic SJ · 48463 IPA东京 · 31293 Pacific新加坡),其余三网 id 来自 superspeed.sh,运行时按 search 词解析。

### 2. 选择机制(CLI:`--speedtest` 改为可选带值)

`--speedtest` 由 bool flag 改为 `Option<String>`(clap `num_args(0..=1)` + `default_missing_value`)。值 `SPEC` 语义:

| SPEC | 含义 |
|---|---|
| 空(`--speedtest`) | 默认国内 6 代表(港 CMHK · 沪联通 · 京联通 · 苏电信 · 浙电信 · 沪移动) |
| `all` | 全目录 |
| `cn` | 三网全部(电信+联通+移动) |
| `ct` / `cu` / `cm` | 电信 / 联通 / 移动 全部 |
| `hk` / `edu` | 香港 / 教育网 全部 |
| `intl` | 国际全部 |
| `us` / `jp` / `sg` | 美国 / 日本 / 新加坡 |
| `5396,24447,...` | 按 server id 自选 |
| `list` | 打印完整目录(id / 运营商 / 城市 / host)后退出 |
| 逗号组合 | 任意混合,如 `cn,jp` / `ct,5396` |

`parse_spec` 解析规则:逗号分割 → 每段是分组关键字 / `all` / 数字 id → 去重合并;`list` 单独分支。**未知关键字(非分组、非数字 id)→ 报错并提示有效取值**(fail fast,UX 更清晰)。配置文件 `config.toml` 的 `[speedtest]` 可设默认 SPEC + 追加自定义 Ookla 节点。

### 3. host 解析(运行时)

测速前先把选中节点解析为 host。按 `search` 词分组(多节点共享城市词,如 shanghai 对应 3633/24447/25637),**每个唯一词只调一次** API 并缓存:

`GET https://www.speedtest.net/api/js/servers?engine=js&limit=100&search=<词>`,头带 `Referer: https://www.speedtest.net/` + 浏览器 UA。响应 JSON 数组里**按 id 精确匹配**得 `host`。`limit=100` 尽量让目标 id 浮上来。某 id 未命中(API 不可达 / 该 vantage 不返回该节点)→ 该节点 `ok=false` 标「解析失败」,不影响其余。

> 解析后 host 可能形如 `*.prod.hosts.ooklaserver.net:8080`(Ookla CDN 别名)或运营商自有域名,直接拿来拼测速 URL。

### 4. 单节点探测(`probe_one`)

每节点串行(并发抢带宽失真),节点内依次(`host` 来自上一步):

1. **延迟**:`GET http://host/latency.txt` ×4,取最小 ms;失败则 `latency_ms = None`。
2. **下载**:`GET http://host/download?nocache=<rand>&size=100000000`,沿用现有 `download_one` 的 chunked + deadline 读法(上限 ~10s);`down_mbps = calc_mbps(bytes, secs)`。
3. **上传**:`POST http://host/upload?nocache=<rand>`,流式发送预生成缓冲(reqwest `Body::wrap_stream` 或定长 body + deadline 上限 ~8s),计已发字节 / 耗时 → `up_mbps`。

复用专用 reqwest client:浏览器 UA + 较长 total timeout。

### 5. 结果模型

```rust
pub struct SpeedResult {
    pub name: String,
    pub carrier: Carrier,
    pub latency_ms: Option<f64>,
    pub down_mbps: f64, pub down_bytes: u64, pub down_secs: f64,
    pub up_mbps: f64,   pub up_bytes: u64,   pub up_secs: f64,
    pub ok: bool,   // 下载拿到数据即 true
}
```

### 6. 渲染

- **终端**:comfy-table,列 `节点 | 运营商 | 延迟 | 下载 | 上传`。下载/上传按速率着色(高绿/中黄/低红/失败灰),延迟按 ms 着色(低绿/中黄/高红)。`no_color` 退纯文本。
- **Markdown**(`render_section`):同列 pipe 表。
- **JSON**(`src/render/json.rs`):`speedtest[]` 数组,每项 `{name, carrier, latency_ms, down_mbps, down_bytes, down_secs, up_mbps, up_bytes, up_secs, ok}`。

### 7. 配置 schema(破坏性变更)

v0.15 的 `[[speedtest_node]] { name, url }` 原始 URL 模型**移除**,改:

```toml
[speedtest]
spec = "cn"                      # 默认选择(同 CLI SPEC 语法)
# 追加目录外的自定义 Ookla 节点
[[speedtest.custom]]
name = "自建测速点"
carrier = "telecom"
host = "speedtest.example.com:8080"
```

CLI `--speedtest <SPEC>` 覆盖配置 `spec`(沿用现有「CLI 优先」约定)。

### 8. 错误处理与降级

- 节点 host 失效 / 超时 / 非 200 → 该节点 `ok=false`,渲染「失败」,不影响其余节点。
- 上传不被服务器接受(部分节点禁 upload)→ `up_mbps=0`,延迟+下载仍出。
- 全部节点失败 → 正常输出空结果表 + 提示,不 panic。

## 测试

`src/probe/speedtest.rs` 内 `#[cfg(test)]`(纯函数,无网络):

- `calc_mbps_*`(保留现有)
- `parse_spec`:空→默认 6;`all`→全量;`cn`/`ct`/`cu`/`cm`/`hk`/`intl`/`us`/`jp`/`sg` 分组正确;`5396,24447` ID 命中;`cn,jp` 混合去重;未知关键字忽略或报错(择一,实现计划定);`list` 单独识别。
- `catalog` 非空、无重复 id、每个 host 含 `:` 端口。
- 默认集恰为 6 个、carrier 覆盖 港/沪联通/京联通/苏电信/浙电信/沪移动。
- `render_terminal` no_color 纯文本含节点名/延迟/下载/上传/失败字样。
- `render_section` markdown 含表头。
- JSON 形状(`src/render/json.rs` 既有测试模式)。

## 诚实标注(README)

- **单连接单 TCP 流**测速(非 Ookla 多连接并发),高带宽链路会**低估**,结果**仅供参考**。
- 节点 host **钉死**;Ookla 重新分配 host 时该节点失败 → 需更新目录表(维护期用 `search=城市` API 重新解析)。
- 测速方向为**本机出口 → 目标节点**(回国 / 出国方向),非对端到本机。
- 仅 IPv4 出口经由系统路由;不强制绑定 IP 版本。

## 改动文件

| 文件 | 改动 |
|---|---|
| `src/probe/speedtest.rs` | 主体重写:Carrier/SpeedNode/catalog/parse_spec/probe_one(延迟+下载+上传)/SpeedResult/渲染 |
| `src/cli.rs` | `--speedtest` 由 bool → `Option<String>`(可选带值);help 文档 |
| `src/config.rs` | 移除旧 `[[speedtest_node]]`,加 `[speedtest] spec` + `[[speedtest.custom]]` |
| `src/main.rs` | 调度:解析 SPEC → 节点;`list` 分支打印目录后退出;CLI 优先覆盖配置 |
| `src/render/json.rs` | `speedtest[]` 字段扩展(延迟+上传) |
| `README.md` | 功能说明 + 路线图 + 诚实标注更新 |
| `CHANGELOG.md` | v0.16.0 条目 + 破坏性配置变更说明 |

## 范围边界(YAGNI)

- **不做**多连接并发测速(单连接,简单一致)。
- **不做**自动选最优节点 / 距离排序(用户按目录显式选)。
- **不做** host 缓存落盘(每次运行重新解析;`search` 词级内存缓存即可)。
- **不做**保留旧 `{name,url}` 原始 URL 配置兼容(v0.15 刚发,作者唯一,可破坏)。
