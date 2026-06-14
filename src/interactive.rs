use crate::cli::Args;
use crate::i18n::Lang;
use crate::{RunOutcome, SpeedtestSpecSource};
use std::io::{self, Write};
use std::net::IpAddr;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Section {
    Raw,
    Probe,
    Mail,
    Route,
    Dnsbl,
    Speedtest,
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Run(Vec<Section>),
    SetIp,
    Quit,
    Reprompt(String),
}

fn parse_input(raw: &str) -> Action {
    let s = raw.trim();
    if s.is_empty() || s == "1" {
        return Action::Run(vec![]);
    }
    match s {
        "A" | "a" => {
            return Action::Run(vec![
                Section::Raw,
                Section::Probe,
                Section::Mail,
                Section::Route,
                Section::Dnsbl,
            ]);
        }
        "I" | "i" => return Action::SetIp,
        "Q" | "q" => return Action::Quit,
        _ => {}
    }

    let mut sections = Vec::new();
    for seg in s.split(',') {
        let seg = seg.trim();
        let section = match seg {
            "" | "1" => None,
            "2" => Some(Section::Raw),
            "3" => Some(Section::Probe),
            "4" => Some(Section::Mail),
            "5" => Some(Section::Route),
            "6" => Some(Section::Dnsbl),
            "7" => Some(Section::Speedtest),
            _ => return Action::Reprompt(format!("无效输入: {raw}")),
        };
        if let Some(section) = section {
            if !sections.contains(&section) {
                sections.push(section);
            }
        }
    }
    Action::Run(sections)
}

fn apply_sections(args: &mut Args, sections: &[Section]) {
    args.raw = false;
    args.probe = false;
    args.mail = false;
    args.route = false;
    args.dnsbl = false;
    args.speedtest = None;

    for section in sections {
        match section {
            Section::Raw => args.raw = true,
            Section::Probe => args.probe = true,
            Section::Mail => args.mail = true,
            Section::Route => args.route = true,
            Section::Dnsbl => args.dnsbl = true,
            Section::Speedtest => args.speedtest = Some(String::new()),
        }
    }
}

pub(crate) fn should_enter_menu(args: &Args, is_tty: bool) -> bool {
    is_tty
        && args.ip.is_none()
        && !args.json
        && !args.markdown
        && !args.raw
        && !args.probe
        && !args.mail
        && !args.route
        && !args.dnsbl
        && args.speedtest.is_none()
        && !args.all
        && !args.report
}

fn render_menu(target: Option<IpAddr>, lang: Lang) -> String {
    let target = target.map_or_else(
        || lang.pick("本机出口", "Local egress").to_string(),
        |ip| ip.to_string(),
    );
    format!(
        "\n═══ {} ═══\n{}: {}\n\n\
         1. {}\n\
         2. {}\n\
         3. {}\n\
         4. {}\n\
         5. {}\n\
         6. {}\n\
         7. {}\n\
         A. {}\n\
         I. {}\n\
         Q. {}\n\n\
         {}",
        lang.pick("ipano 交互菜单", "ipano interactive menu"),
        lang.pick("目标", "Target"),
        target,
        lang.pick("仅 IP 全景报告", "IP panorama report only"),
        lang.pick("逐源质量详表", "Raw source details"),
        lang.pick("解锁检测 38 项", "Unlock probes (38 checks)"),
        lang.pick("邮局连通", "Mail connectivity"),
        lang.pick("三网回程路由", "CN backhaul routes"),
        lang.pick("DNSBL 黑名单", "DNSBL blocklists"),
        lang.pick("多节点测速(耗流量)", "Multi-node speedtest (uses traffic)"),
        lang.pick("全跑(不含测速)", "Run all except speedtest"),
        lang.pick("修改目标 IP", "Set target IP"),
        lang.pick("退出", "Quit"),
        lang.pick(
            "请输入编号(可逗号多选): ",
            "Select items (comma-separated): "
        ),
    )
}

pub(crate) async fn run(
    base_args: Args,
    client: &reqwest::Client,
    lang: Lang,
    no_color: bool,
    timeout: u64,
    ping0_token: Option<String>,
    speedtest_cfg: Option<&crate::config::SpeedtestCfg>,
) {
    let mut target: Option<IpAddr> = None;
    loop {
        print!("{}", render_menu(target, lang));
        let _ = io::stdout().flush();

        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("{}: {e}", lang.pick("读取输入失败", "Failed to read input"));
                continue;
            }
        }

        match parse_input(&line) {
            Action::Quit => break,
            Action::Reprompt(msg) => {
                println!("{msg}");
            }
            Action::SetIp => {
                print!(
                    "{}",
                    lang.pick(
                        "请输入目标 IP(留空=本机出口): ",
                        "Enter target IP (blank = local egress): "
                    )
                );
                let _ = io::stdout().flush();

                let mut ip_line = String::new();
                match io::stdin().read_line(&mut ip_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let s = ip_line.trim();
                        if s.is_empty() {
                            target = None;
                        } else {
                            match s.parse() {
                                Ok(ip) => target = Some(ip),
                                Err(_) => eprintln!("{}: {s}", lang.pick("无效 IP", "Invalid IP")),
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{}: {e}", lang.pick("读取输入失败", "Failed to read input"));
                    }
                }
            }
            Action::Run(sections) => {
                let mut args = base_args.clone();
                apply_sections(&mut args, &sections);
                let targets = match crate::resolve_targets(target, &args, client).await {
                    Ok(targets) => targets,
                    Err(e) => {
                        eprintln!("{}", e.message(lang));
                        continue;
                    }
                };

                for ip in targets {
                    let outcome = crate::run_once(
                        ip,
                        &args,
                        client,
                        lang,
                        no_color,
                        timeout,
                        ping0_token.as_deref(),
                        speedtest_cfg,
                        SpeedtestSpecSource::CliOnly,
                    )
                    .await;
                    if outcome == RunOutcome::Stop {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Args;
    use crate::i18n::Lang;
    use std::net::{IpAddr, Ipv4Addr};

    fn base_args() -> Args {
        Args {
            ip: None,
            four: false,
            six: false,
            json: false,
            markdown: false,
            raw: false,
            lang: None,
            all: false,
            probe: false,
            mail: false,
            route: false,
            dnsbl: false,
            speedtest: None,
            ping0_token: None,
            no_color: false,
            timeout: None,
            report: false,
        }
    }

    #[test]
    fn parse_input_runs_report_only_for_enter_or_one() {
        assert_eq!(parse_input(""), Action::Run(vec![]));
        assert_eq!(parse_input("1"), Action::Run(vec![]));
    }

    #[test]
    fn parse_input_maps_single_sections() {
        assert_eq!(parse_input("2"), Action::Run(vec![Section::Raw]));
        assert_eq!(parse_input("3"), Action::Run(vec![Section::Probe]));
        assert_eq!(parse_input("4"), Action::Run(vec![Section::Mail]));
        assert_eq!(parse_input("5"), Action::Run(vec![Section::Route]));
        assert_eq!(parse_input("6"), Action::Run(vec![Section::Dnsbl]));
        assert_eq!(parse_input("7"), Action::Run(vec![Section::Speedtest]));
    }

    #[test]
    fn parse_input_all_setip_quit_and_multiselect() {
        assert_eq!(
            parse_input("A"),
            Action::Run(vec![
                Section::Raw,
                Section::Probe,
                Section::Mail,
                Section::Route,
                Section::Dnsbl,
            ])
        );
        assert_eq!(parse_input("a"), parse_input("A"));
        assert_eq!(parse_input("I"), Action::SetIp);
        assert_eq!(parse_input("i"), Action::SetIp);
        assert_eq!(parse_input("Q"), Action::Quit);
        assert_eq!(parse_input("q"), Action::Quit);
        assert_eq!(
            parse_input(" 3 , 6 "),
            Action::Run(vec![Section::Probe, Section::Dnsbl])
        );
        assert_eq!(
            parse_input("1,3,6"),
            Action::Run(vec![Section::Probe, Section::Dnsbl])
        );
    }

    #[test]
    fn parse_input_reprompts_for_invalid_values() {
        assert!(matches!(parse_input("9"), Action::Reprompt(_)));
        assert!(matches!(parse_input("xyz"), Action::Reprompt(_)));
        assert!(matches!(parse_input("3,xyz"), Action::Reprompt(_)));
    }

    #[test]
    fn apply_sections_sets_only_selected_flags() {
        let mut args = base_args();
        args.probe = true;

        apply_sections(
            &mut args,
            &[Section::Raw, Section::Dnsbl, Section::Speedtest],
        );

        assert!(args.raw);
        assert!(!args.probe);
        assert!(!args.mail);
        assert!(!args.route);
        assert!(args.dnsbl);
        assert_eq!(args.speedtest.as_deref(), Some(""));
    }

    #[test]
    fn should_enter_menu_only_for_tty_without_function_flags() {
        let mut args = base_args();
        assert!(should_enter_menu(&args, true));
        assert!(!should_enter_menu(&args, false));

        args.lang = Some("en".to_string());
        args.no_color = true;
        args.timeout = Some(5);
        args.ping0_token = Some("token".to_string());
        args.four = true;
        assert!(should_enter_menu(&args, true));

        args.ip = Some("1.1.1.1".to_string());
        assert!(!should_enter_menu(&args, true));
    }

    #[test]
    fn should_enter_menu_rejects_each_function_flag_and_report() {
        let mut args = base_args();
        args.json = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.markdown = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.raw = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.probe = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.mail = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.route = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.dnsbl = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.speedtest = Some(String::new());
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.all = true;
        assert!(!should_enter_menu(&args, true));

        let mut args = base_args();
        args.report = true;
        assert!(!should_enter_menu(&args, true));
    }

    #[test]
    fn render_menu_shows_target_and_items_in_zh_and_en() {
        let zh = render_menu(None, Lang::Zh);
        assert!(zh.contains("本机出口"));
        assert!(zh.contains("1."));
        assert!(zh.contains("逐源质量详表"));
        assert!(zh.contains("A."));
        assert!(zh.contains("退出"));

        let en = render_menu(None, Lang::En);
        assert!(en.contains("Local egress"));
        assert!(en.contains("Raw source details"));
        assert!(en.contains("Quit"));

        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let en_with_ip = render_menu(Some(ip), Lang::En);
        assert!(en_with_ip.contains("1.1.1.1"));
    }
}
