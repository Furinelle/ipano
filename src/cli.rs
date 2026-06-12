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
    /// 输出语言:zh(默认)或 en;未指定时回落到配置文件,再回落到 zh
    #[arg(long)]
    pub lang: Option<String>,
    /// 一键全跑:等价于同时传 --probe --mail --route --dnsbl
    #[arg(long, short = 'A')]
    pub all: bool,
    /// 启用解锁检测(主动探测 Netflix/YouTube/ChatGPT,从本机出口发起)
    #[arg(long)]
    pub probe: bool,
    /// 启用邮局连通性检测(TCP 连 SMTP 25/465/587 到主流邮局)
    #[arg(long)]
    pub mail: bool,
    /// 启用三网回程路由(原生 traceroute 到 电信/联通/移动 参考节点;需 root/cap_net_raw,无特权自动降级)
    #[arg(long)]
    pub route: bool,
    /// 启用 DNSBL 黑名单检测(12 个主流邮件/滥用黑名单,仅 IPv4)
    #[arg(long)]
    pub dnsbl: bool,
    /// 启用多节点下载测速(顺序下载 Cloudflare/Cachefly/Linode 东京/ThinkBroadband;
    /// 会消耗较多流量,故不含在 --all 内,需单独开启)
    #[arg(long)]
    pub speedtest: bool,
    /// ping0 token(浏览器解 Turnstile 验证码后从 cookie 复制,60 秒内有效);
    /// 不提供则 ping0 源自动降级。也可用环境变量 IPANO_PING0_TOKEN
    #[arg(long)]
    pub ping0_token: Option<String>,
    /// 关闭彩色
    #[arg(long)]
    pub no_color: bool,
    /// 单源超时(秒);未指定时回落到配置文件,再回落到 8
    #[arg(long)]
    pub timeout: Option<u64>,
}
