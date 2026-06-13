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
    /// 逐源原始详表(securityCheck 同款,每字段标来源)
    #[arg(long)]
    pub raw: bool,
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
    /// 启用 DNSBL 黑名单检测(211 个邮件/滥用黑名单,并发查询,仅 IPv4)
    #[arg(long)]
    pub dnsbl: bool,
    /// 多节点测速(对 speedtest.net 三网/国际节点测 延迟+下载+上传,从本机出口发起);
    /// 不带值=默认 6 代表;可选 SPEC 须用等号: --speedtest=all/cn/ct/cu/cm/hk/edu/intl/us/jp/sg/list 或 server id 列表(逗号分隔)。
    /// 会消耗较多流量,故不含在 --all 内。require_equals 避免 `--speedtest <IP>` 把目标 IP 误当 SPEC 吞掉。
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "")]
    pub speedtest: Option<String>,
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
