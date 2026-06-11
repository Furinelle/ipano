use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ipano", version, about = "一站式 IP 全景聚合检测工具")]
pub struct Args {
    /// 要查询的 IP;省略则查本机出口 IP
    pub ip: Option<String>,
    /// 仅 IPv4
    #[arg(short = '4', long)]
    pub four: bool,
    /// 仅 IPv6
    #[arg(short = '6', long)]
    pub six: bool,
    /// 输出 JSON
    #[arg(long)]
    pub json: bool,
    /// 关闭彩色
    #[arg(long)]
    pub no_color: bool,
    /// 单源超时(秒)
    #[arg(long, default_value_t = 8)]
    pub timeout: u64,
}
