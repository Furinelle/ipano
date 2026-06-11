mod model;
mod fetch;
mod egress;
mod aggregate;
mod sources;
mod render;
mod cli;

use std::net::IpAddr;
use clap::Parser;

#[tokio::main]
async fn main() {
    let args = cli::Args::parse();
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

    for ip in targets {
        let srcs = sources::all_sources();
        let results = sources::run_all(&client, ip, &srcs).await;
        let report = aggregate::merge(ip, results);
        if args.json {
            println!("{}", render::json::to_json(&report));
        } else {
            println!("{}", render::terminal::render(&report, args.no_color));
        }
    }
}
