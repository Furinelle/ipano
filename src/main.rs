#![recursion_limit = "256"]
mod aggregate;
mod cli;
mod config;
mod egress;
mod fetch;
mod heuristics;
mod i18n;
mod interactive;
mod model;
mod probe;
mod render;
mod sources;

use clap::Parser;
use std::io::IsTerminal;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpeedtestSpecSource {
    CliOrConfig,
    CliOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    Rendered,
    Stop,
}

pub(crate) enum TargetError {
    InvalidIp(String),
    EgressUnavailable,
}

impl TargetError {
    fn code(&self) -> i32 {
        match self {
            TargetError::InvalidIp(_) => 2,
            TargetError::EgressUnavailable => 1,
        }
    }

    pub(crate) fn message(&self, lang: i18n::Lang) -> String {
        match self {
            TargetError::InvalidIp(s) => {
                format!("{}: {s}", lang.pick("无效 IP", "Invalid IP"))
            }
            TargetError::EgressUnavailable => lang
                .pick("无法探测本机出口 IP", "Unable to detect local egress IP")
                .to_string(),
        }
    }
}

#[tokio::main]
async fn main() {
    // 先加载配置文件(~/.config/ipano/config.toml),再用 CLI 参数覆盖
    let cfg = config::load();
    let mut args = cli::Args::parse();

    // 优先级:CLI 显式参数 > 配置文件 > 内置默认。
    // lang/timeout 为 Option,故能区分「用户显式传 zh/8」与「未传」。
    let lang_str = args
        .lang
        .clone()
        .or(cfg.lang.clone())
        .unwrap_or_else(|| "zh".into());
    let timeout = args.timeout.or(cfg.timeout).unwrap_or(8);
    // no_color/ping0_token:CLI flag 或配置任一开启即生效
    let no_color = args.no_color || cfg.no_color == Some(true);
    let ping0_token = args.ping0_token.clone().or(cfg.ping0_token.clone());
    let lang = i18n::Lang::parse(&lang_str);
    let client = fetch::build_client(timeout);

    // 先应用 config [always] 与 --all 展开,再判断是否进菜单:
    // 配了 always.* 的用户裸跑应直接跑对应模块(符合「始终开启」语义),不被菜单接管。
    apply_config_always(&mut args, cfg.always.as_ref());
    expand_all(&mut args);

    if interactive::should_enter_menu(&args, std::io::stdin().is_terminal()) {
        interactive::run(
            args,
            &client,
            lang,
            no_color,
            timeout,
            ping0_token,
            cfg.speedtest.as_ref(),
        )
        .await;
        return;
    }

    let targets = match resolve_targets(None, &args, &client).await {
        Ok(targets) => targets,
        Err(err) => {
            eprintln!("{}", err.message(lang));
            std::process::exit(err.code());
        }
    };

    for ip in targets {
        if run_once(
            ip,
            &args,
            &client,
            lang,
            no_color,
            timeout,
            ping0_token.as_deref(),
            cfg.speedtest.as_ref(),
            SpeedtestSpecSource::CliOrConfig,
        )
        .await
            == RunOutcome::Stop
        {
            return;
        }
    }
}

fn apply_config_always(args: &mut cli::Args, always: Option<&config::AlwaysFlags>) {
    if let Some(always) = always {
        if always.probe == Some(true) {
            args.probe = true;
        }
        if always.mail == Some(true) {
            args.mail = true;
        }
        if always.route == Some(true) {
            args.route = true;
        }
        if always.dnsbl == Some(true) {
            args.dnsbl = true;
        }
    }
}

fn expand_all(args: &mut cli::Args) {
    if args.all {
        args.probe = true;
        args.mail = true;
        args.route = true;
        args.dnsbl = true;
    }
}

pub(crate) async fn resolve_targets(
    target: Option<IpAddr>,
    args: &cli::Args,
    client: &reqwest::Client,
) -> Result<Vec<IpAddr>, TargetError> {
    if let Some(ip) = target {
        return Ok(vec![ip]);
    }

    if let Some(s) = &args.ip {
        return s
            .parse()
            .map(|ip| vec![ip])
            .map_err(|_| TargetError::InvalidIp(s.clone()));
    }

    let (v4, v6) = egress::detect(client).await;
    let mut targets = Vec::new();
    if !args.six {
        if let Some(ip) = v4 {
            targets.push(ip);
        }
    }
    if !args.four {
        if let Some(ip) = v6 {
            targets.push(ip);
        }
    }
    if targets.is_empty() {
        Err(TargetError::EgressUnavailable)
    } else {
        Ok(targets)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_once(
    ip: IpAddr,
    args: &cli::Args,
    client: &reqwest::Client,
    lang: i18n::Lang,
    no_color: bool,
    timeout: u64,
    ping0_token: Option<&str>,
    speedtest_cfg: Option<&config::SpeedtestCfg>,
    speedtest_spec_source: SpeedtestSpecSource,
) -> RunOutcome {
    // 解锁检测从本机出口发起,只跑一次;先探测本机国家码用于 Native/DNS 区分
    let probe_country = if args.probe {
        egress::detect_country(client).await.unwrap_or_default()
    } else {
        String::new()
    };
    let probes = if args.probe {
        probe::run_all_with_native_check(client, &probe::all_probes(), &probe_country).await
    } else {
        Vec::new()
    };
    let mail = if args.mail {
        probe::mail::check_all(timeout).await
    } else {
        Vec::new()
    };
    // 三网回程路由从本机出口发起(仅 IPv4),只跑一次;无特权自动降级
    let routes = if args.route {
        probe::route::run_routes(client, timeout).await
    } else {
        Vec::new()
    };
    // 多节点测速:CLI --speedtest SPEC 优先,其次配置 [speedtest].spec;list 打印目录后退出
    let speedtest = {
        let cli_spec = args.speedtest.clone();
        let cfg_spec = match speedtest_spec_source {
            SpeedtestSpecSource::CliOrConfig => speedtest_cfg.and_then(|s| s.spec.clone()),
            SpeedtestSpecSource::CliOnly => None,
        };
        let spec = cli_spec.or(cfg_spec);
        match spec {
            None => Vec::new(),
            Some(spec) => {
                let mut cat = probe::speedtest::catalog();
                if let Some(customs) = speedtest_cfg.and_then(|s| s.custom.as_ref()) {
                    for cu in customs {
                        cat.push(probe::speedtest::SpeedNode {
                            id: 0,
                            name: cu.name.clone(),
                            carrier: probe::speedtest::Carrier::from_str_lenient(&cu.carrier),
                            search: String::new(),
                            host: Some(cu.host.clone()),
                            default: false,
                        });
                    }
                }
                match probe::speedtest::parse_spec(&spec, &cat) {
                    Ok(probe::speedtest::Selection::List) => {
                        print!("{}", probe::speedtest::render_catalog(&cat, lang));
                        return RunOutcome::Stop;
                    }
                    Ok(probe::speedtest::Selection::Nodes(nodes)) => {
                        probe::speedtest::run_all(&nodes).await
                    }
                    Err(e) => {
                        eprintln!("--speedtest: {e}");
                        std::process::exit(2);
                    }
                }
            }
        }
    };

    // DNSBL 检测:针对当前查询 IP(仅 IPv4)
    let dnsbl = if args.dnsbl {
        if let IpAddr::V4(v4) = ip {
            probe::dnsbl::check_all(v4).await
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let srcs = sources::all_sources(ping0_token.map(str::to_string));
    let results = sources::run_all(client, ip, &srcs).await;
    let report = aggregate::merge(ip, results);

    if args.json {
        println!(
            "{}",
            render::json::to_json(&report, &probes, &mail, &routes, &dnsbl, &speedtest)
        );
    } else {
        if args.markdown {
            print!("{}", render::markdown::to_markdown(&report, lang));
        } else {
            print!("{}", render::terminal::render(&report, no_color, lang));
        }
        if args.raw {
            print!("\n{}", render::raw::render(&report));
        }
        if !probes.is_empty() {
            let s = if args.markdown {
                probe::render_section(&probes, lang)
            } else {
                probe::render_terminal(&probes, lang, no_color)
            };
            println!("\n{}", s);
        }
        if !mail.is_empty() {
            let s = if args.markdown {
                probe::mail::render_section(&mail, lang)
            } else {
                probe::mail::render_terminal(&mail, lang)
            };
            println!("\n{}", s);
        }
        if !routes.is_empty() {
            let s = if args.markdown {
                probe::route::render_section(&routes, lang)
            } else {
                probe::route::render_terminal(&routes, lang, no_color)
            };
            println!("\n{}", s);
        }
        if !dnsbl.is_empty() {
            let s = if args.markdown {
                probe::dnsbl::render_section(&dnsbl, &ip.to_string(), lang)
            } else {
                probe::dnsbl::render_terminal(&dnsbl, &ip.to_string(), lang, no_color)
            };
            println!("\n{}", s);
        }
        if !speedtest.is_empty() {
            let s = if args.markdown {
                probe::speedtest::render_section(&speedtest, lang)
            } else {
                probe::speedtest::render_terminal(&speedtest, lang, no_color)
            };
            println!("\n{}", s);
        }
    }

    RunOutcome::Rendered
}
