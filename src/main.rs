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

    // 解锁检测/邮局连通性从本机出口发起,与查询 IP 无关,只跑一次
    let probes = if args.probe {
        probe::run_all(&client, &probe::all_probes()).await
    } else {
        Vec::new()
    };
    let mail = if args.mail {
        probe::mail::check_all(args.timeout).await
    } else {
        Vec::new()
    };

    for ip in targets {
        let srcs = sources::all_sources(args.ping0_token.clone());
        let results = sources::run_all(&client, ip, &srcs).await;
        let report = aggregate::merge(ip, results);
        if args.json {
            println!("{}", render::json::to_json(&report, &probes, &mail));
        } else {
            if args.markdown {
                print!("{}", render::markdown::to_markdown(&report, lang));
            } else {
                print!("{}", render::terminal::render(&report, args.no_color, lang));
            }
            if !probes.is_empty() { println!("\n{}", probe::render_section(&probes, lang)); }
            if !mail.is_empty() { println!("\n{}", probe::mail::render_section(&mail, lang)); }
        }
    }
}
