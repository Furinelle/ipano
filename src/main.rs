mod model;
mod fetch;
mod egress;
mod aggregate;
mod sources;
mod render;
mod cli;
mod i18n;
mod heuristics;
mod probe;

use std::net::IpAddr;
use clap::Parser;

#[tokio::main]
async fn main() {
    let args = cli::Args::parse();
    let lang = i18n::Lang::parse(&args.lang);
    let client = fetch::build_client(args.timeout);

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
        probe::mail::check_all(args.timeout).await
    } else {
        Vec::new()
    };
    // 三网回程路由从本机出口发起(仅 IPv4),与查询 IP 无关,只跑一次;无特权自动降级
    let routes = if args.route {
        probe::route::run_routes(&client, args.timeout).await
    } else {
        Vec::new()
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

        let srcs = sources::all_sources(args.ping0_token.clone());
        let results = sources::run_all(&client, ip, &srcs).await;
        let report = aggregate::merge(ip, results);
        if args.json {
            println!("{}", render::json::to_json(&report, &probes, &mail, &routes, &dnsbl));
        } else {
            if args.markdown {
                print!("{}", render::markdown::to_markdown(&report, lang));
            } else {
                print!("{}", render::terminal::render(&report, args.no_color, lang));
            }
            if !probes.is_empty() {
                let s = if args.markdown {
                    probe::render_section(&probes, lang)
                } else {
                    probe::render_terminal(&probes, lang)
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
                    probe::route::render_terminal(&routes, lang)
                };
                println!("\n{}", s);
            }
            if !dnsbl.is_empty() {
                let s = if args.markdown {
                    probe::dnsbl::render_section(&dnsbl, &ip.to_string(), lang)
                } else {
                    probe::dnsbl::render_terminal(&dnsbl, &ip.to_string(), lang)
                };
                println!("\n{}", s);
            }
        }
    }
}
