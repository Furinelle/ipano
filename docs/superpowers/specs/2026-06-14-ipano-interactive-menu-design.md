# ipano 交互式菜单设计(对标融合怪 ecs.sh)

- 日期:2026-06-14
- 状态:已实现
- 目标版本:v0.20.0

## 背景与目标

融合怪 ecs.sh 运行后弹出数字菜单,用户输入编号选择运行哪些测试项。ipano 当前所有能力靠命令行 flag(`--raw`/`--probe`/`--mail`/`--route`/`--dnsbl`/`--speedtest`/`--all`)驱动,用户需记忆 flag 组合。

本设计给 ipano 增加一个**数字菜单交互界面**:裸跑 `ipano` 进入菜单,选择(可多选)要运行的检测模块,跑完回到菜单可继续选,直到退出。

**非目标(YAGNI):**

- 全屏 TUI(方向键/复选框)——明确不做,数字菜单零依赖、SSH/VPS 最稳
- 菜单内切换语言/输出格式(json/markdown)——交互场景用不到,沿用命令行 flag
- 任何新增第三方 crate

## 形态决策

- **数字菜单**(非全屏 TUI):打印带编号的模块列表,读 stdin 一行,解析编号执行。零新依赖。
- **裸跑进入 + TTY 检测**:`ipano`(完全无参)且 stdin 是交互终端 → 进菜单;否则照旧。
- **菜单内可设目标 IP**:`[I]` 入口输入任意 IP,留空=本机出口;target 在循环内持久保持。

## 入口判断(决策树)

改变了 `ipano` 裸跑的默认行为,故入口判断是最敏感部分。

```
ipano 启动
  ├─ 传了位置参数 IP?                    ──► 直跑(现有行为)
  ├─ 传了任一【功能 flag】?              ──► 直跑(现有行为)
  ├─ 传了 --report?(新增逆向开关)       ──► 直跑查本机
  ├─ stdin 不是 TTY?(管道/cron/重定向)  ──► 直跑查本机(自动化零回归)
  └─ 否则(完全裸跑 + 交互终端)          ──► 进入交互菜单 ★新增
```

flag 分类:

| 类别 | flag | 进菜单? |
|---|---|---|
| 功能 flag | `--json` `--markdown` `--raw` `--probe` `--mail` `--route` `--dnsbl` `--speedtest` `--all` + 位置 IP | 否,直跑 |
| 修饰 flag | `--lang` `--no-color` `--timeout` `--ping0-token` `-4` `-6` | 是,菜单沿用这些设置 |
| 逆向开关 | `--report`(新增) | 否,强制直跑查本机 |

向后兼容:`ipano <ip>`、`ipano --probe`、`ipano | tee log`、cron 任务全部行为不变;只有人在终端敲 `ipano` 回车才进菜单;`ipano --report` 恢复老的裸跑查本机行为。

入口判断抽成纯函数 `should_enter_menu(args, is_tty) -> bool`,可单测。

## 菜单项 + 数据结构

**关键约定:全景报告是基础输出,恒含**(沿用现有 CLI——报告总是先打印)。菜单选的是「报告之外额外跑什么」。

| 输入 | 含义 | 映射到 |
|---|---|---|
| `1` 或回车 | 仅 IP 全景报告 | (基础,无附加) |
| `2` | + 逐源质量详表 | `args.raw` |
| `3` | + 解锁检测 38 项 | `args.probe` |
| `4` | + 邮局连通 | `args.mail` |
| `5` | + 三网回程路由 | `args.route` |
| `6` | + DNSBL 黑名单 | `args.dnsbl` |
| `7` | + 多节点测速(耗流量) | `args.speedtest=Some("")` |
| `A`/`a` | 全跑(2-6 全部,**不含测速**;比命令行 `--all` 多含 raw 详表) | raw+probe+mail+route+dnsbl |
| `I`/`i` | 修改目标 IP | — |
| `Q`/`q` | 退出 | — |

多选逗号分隔:`3,6` = 报告 + 解锁 + DNSBL。

```rust
#[derive(Debug, PartialEq, Clone, Copy)]
enum Section { Raw, Probe, Mail, Route, Dnsbl, Speedtest }
// 全景报告恒含,不入枚举;"1"/回车 = Run(空集) = 仅报告

#[derive(Debug, PartialEq)]
enum Action {
    Run(Vec<Section>),    // 执行选中检测
    SetIp,                // [I]
    Quit,                 // [Q] / EOF(Ctrl-D)
    Reprompt(String),     // 无效或空白 → 显示提示后重来
}

fn parse_input(raw: &str) -> Action       // 核心可测纯函数
fn apply_sections(&mut Args, &[Section])   // Section 集合 → 写回 args 字段
```

菜单**不重写任何业务逻辑**,只做「输入 → args」翻译。

## 交互循环与数据流

```
裸跑 + TTY → main → interactive::run(base_args, client, lang, no_color)
   ┌───────────────────────── loop ─────────────────────────┐
   │ 1. 打印 render_menu(target, lang)            → stdout    │
   │ 2. 读 stdin 一行                                          │
   │ 3. parse_input(line) → Action:                           │
   │     ├ Quit / EOF(Ctrl-D) ──────────► break(退出)         │
   │     ├ Reprompt(msg) ──► 打印 msg ──► continue            │
   │     ├ SetIp ──► 提示输入 ──► 更新 target ──► continue    │
   │     └ Run(sections):                                     │
   │         a. args = base_args.clone()   ★每轮干净副本      │
   │         b. apply_sections(&mut args, &sections)          │
   │         c. resolve_targets(target,&args,client)→Vec<IP>  │
   │         d. for ip: run_once(ip,&args,…)       → stdout   │
   │         e. 回到循环顶部(可继续选)                        │
   └──────────────────────────────────────────────────────────┘
```

三个关键正确性点:

1. **`base_args.clone()` 每轮重置**——否则连续选 `[3]`、`[6]` 会累积成 probe+dnsbl。base_args 只保留修饰 flag,模块 flag 每轮从干净副本重建。
2. **target 状态保持在循环里**——`target: Option<IpAddr>`(None=本机出口),`[I]` 修改后持久,直到再次修改。
3. **运行后回菜单**(融合怪行为)——跑完一组检测回菜单可继续,`Q`/Ctrl-D 才退出。

### 复用现有逻辑的重构

为让菜单与命令行直跑共用同一渲染路径,把 main.rs 现有 `for ip in targets { … }` 内联的渲染调度抽成共享函数:

- `run_once(ip, &args, &client, lang, no_color)`:对单个 IP 跑全套(报告恒出 + 按 args flag 追加 raw/probe/mail/route/dnsbl/speedtest)。main 直跑与 interactive 均调用它。
- `resolve_targets(target, &args, &client) -> Vec<IpAddr>`:None 时走 `egress::detect`(遵守 `-4/-6`),Some 时单个 IP。main 与 interactive 共用。

这两个抽取是本设计在既有代码上的定向改进——把渲染调度收敛到一处,菜单和命令行成为同一逻辑的两个前端。

## 错误处理

| 情况 | 处理 |
|---|---|
| 无效菜单输入(`9`/`xyz`) | `Reprompt`,回菜单,不崩溃 |
| `[I]` 后输入无效 IP | 打印「无效 IP」,target 保持不变,回菜单 |
| EOF / Ctrl-D | 视同 `Quit`,优雅退出 |
| Ctrl-C(SIGINT) | 不特殊处理,标准信号终止 |
| 本机出口探测失败 | 打印错误,**回菜单**(用户可改用 `[I]` 指定 IP) |
| 某检测模块内部失败 | 各模块已自带降级,菜单循环不受影响 |

原则:**菜单循环永不因单次输入/单个模块失败而崩溃**,最坏回到菜单重来。

## 测试策略

**纯函数单测(核心):**

- `parse_input()`:`""`/`"1"`/`"3"`/`"1,3,6"`/`"A"`/`"a"`/`"I"`/`"Q"`/`"9"`/`"xyz"`/含空格 `" 3 , 6 "` → 对应 `Action`
- `should_enter_menu(args, is_tty)`:裸跑+tty→true;裸跑+非tty→false;带 ip→false;`--probe`→false;`--report`→false;`--lang en`+tty→true
- `render_menu(target, lang)`:含各菜单项标签;target=None 显示「本机出口」,Some 显示 IP;zh/en 双语
- `apply_sections()`:Section 集合正确写入对应 args 字段

**不写自动化测试(靠 VPS 手动实跑,符合项目惯例):**

- stdin 读取 / 循环驱动 / `run_once` / `resolve_targets`(涉及真实网络 + 终端交互)

## 组件清单(预计)

| 文件 | 改动 |
|---|---|
| `src/interactive.rs` | 新增:`Section`/`Action`/`parse_input`/`apply_sections`/`render_menu`/`should_enter_menu`/`run`(循环驱动) |
| `src/cli.rs` | 新增 `--report` flag |
| `src/main.rs` | 抽出 `run_once`/`resolve_targets`;入口加 `should_enter_menu` 判断分支;`mod interactive;` |

## i18n

菜单文本走现有 `i18n::Lang::pick(zh, en)`,zh/en 双语,沿用命令行 `--lang`/config 设置。

## 版本与发布

- 版本号:v0.19.0 → v0.20.0(新增用户可见交互特性,minor bump)
- README:新增「交互菜单」章节 + 截图/示例;说明裸跑行为变化与 `--report` 逆向开关
- CHANGELOG:记录入口行为变化(裸跑+TTY 进菜单)、`--report` 新增、菜单能力
