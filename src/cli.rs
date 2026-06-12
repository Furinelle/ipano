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
    /// 输出 Markdown(含各源对比表 + 启发式结论)
    #[arg(long)]
    pub markdown: bool,
    /// 输出语言:zh(默认)或 en
    #[arg(long, default_value = "zh")]
    pub lang: String,
    /// 启用解锁检测(主动探测 Netflix/YouTube/ChatGPT,从本机出口发起)
    #[arg(long)]
    pub probe: bool,
    /// 启用邮局连通性检测(TCP 连 SMTP 25/465/587 到主流邮局)
    #[arg(long)]
    pub mail: bool,
    /// ping0 token(浏览器解 Turnstile 验证码后从 cookie 复制,60 秒内有效);
    /// 不提供则 ping0 源自动降级。也可用环境变量 IPANO_PING0_TOKEN
    #[arg(long)]
    pub ping0_token: Option<String>,
    /// 关闭彩色
    #[arg(long)]
    pub no_color: bool,
    /// 单源超时(秒)
    #[arg(long, default_value_t = 8)]
    pub timeout: u64,
}
