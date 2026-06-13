mod model;
mod fetch;
mod egress;
mod aggregate;
mod sources;
mod render;
mod cli;
mod config;
mod i18n;
mod heuristics;
mod probe;

use std::net::IpAddr;
use clap::Parser;

#[tokio::main]
async fn main() {
    // 先加载配置文件(~/.config/ipano/config.toml),再用 CLI 参数覆盖
    let cfg = config::load();
    let mut args = cli::Args::parse();

    // 优先级:CLI 显式参数 > 配置文件 > 内置默认。
    // lang/timeout 为 Option,故能区分「用户显式传 zh/8」与「未传」。
    let lang_str    = args.lang.clone().or(cfg.lang).unwrap_or_else(|| "zh".into());
    let timeout     = args.timeout.or(cfg.timeout).unwrap_or(8);
    // no_color/ping0_token:CLI flag 或配置任一开启即生效
    let no_color    = args.no_color || cfg.no_color == Some(true);
    let ping0_token = args.ping0_token.clone().or(cfg.ping0_token);

    // always 标志(配置文件中常开的模块)
    if let Some(always) = &cfg.always {
        if always.probe == Some(true) { args.probe = true; }
        if always.mail  == Some(true) { args.mail  = true; }
        if always.route == Some(true) { args.route = true; }
        if always.dnsbl == Some(true) { args.dnsbl = true; }
    }

    // --all 展开:等价于 --probe --mail --route --dnsbl
    if args.all {
        args.probe = true;
        args.mail  = true;
        args.route = true;
        args.dnsbl = true;
    }

    let lang   = i18n::Lang::parse(&lang_str);
    let client = fetch::build_client(timeout);

    let targets: Vec<IpAddr> = match &args.ip {
        Some(s) => match s.parse() {
            Ok(ip) => vec![ip],
            Err(_) => { eprintln!("无效 IP: {}", s); std::process::exit(2); }
        },
        None => {
            let (v4, v6) = egress::detect(&client).await;
            let mut v = Vec::new();
            if !args.six { if let Some(ip) = v4 { v.push(ip); } }
            if !args.four { if let Some(ip) = v6 { v.push(ip); } }
            if v.is_empty() { eprintln!("无法探测本机出口 IP"); std::process::exit(1); }
            v
        }
    };

    // 解锁检测从本机出口发起,只跑一次;先探测本机国家码用于 Native/DNS 区分
    let probe_country = if args.probe {
        egress::detect_country(&client).await.unwrap_or_default()
    } else {
        String::new()
    };
    let probes = if args.probe {
        probe::run_all_with_native_check(&client, &probe::all_probes(), &probe_country).await
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
        probe::route::run_routes(&client, timeout).await
    } else {
        Vec::new()
    };
    // 多节点测速:CLI --speedtest SPEC 优先,其次配置 [speedtest].spec;list 打印目录后退出
    let speedtest = {
        let cli_spec = args.speedtest.clone();
        let cfg_st = cfg.speedtest;
        let spec = cli_spec.or_else(|| cfg_st.as_ref().and_then(|s| s.spec.clone()));
        match spec {
            None => Vec::new(),
            Some(spec) => {
                let mut cat = probe::speedtest::catalog();
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
                        return;
                    }
                    Ok(probe::speedtest::Selection::Nodes(nodes)) => probe::speedtest::run_all(&nodes).await,
                    Err(e) => { eprintln!("--speedtest: {e}"); std::process::exit(2); }
                }
            }
        }
    };

    for ip in targets {
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

        let srcs    = sources::all_sources(ping0_token.clone());
        let results = sources::run_all(&client, ip, &srcs).await;
        let report  = aggregate::merge(ip, results);

        if args.json {
            println!("{}", render::json::to_json(&report, &probes, &mail, &routes, &dnsbl, &speedtest));
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
    }
}
