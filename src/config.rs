/// 配置文件加载:~/.config/ipano/config.toml
///
/// 示例配置:
/// ```toml
/// lang = "zh"          # 默认语言(zh/en)
/// timeout = 8          # 单源超时(秒)
/// no_color = false
/// ping0_token = ""     # ping0 token(可选)
///
/// # AbuseIPDB / IPQS API key 请用环境变量:
/// #   IPANO_ABUSEIPDB_KEY / IPANO_IPQS_KEY
///
/// [always]             # 始终开启的探测模块
/// probe  = false       # 流媒体&AI 解锁检测
/// mail   = false       # 邮局连通性
/// route  = false       # 三网回程路由
/// dnsbl  = false       # DNSBL 黑名单检测
/// ```
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub lang: Option<String>,
    pub timeout: Option<u64>,
    pub no_color: Option<bool>,
    pub ping0_token: Option<String>,
    pub always: Option<AlwaysFlags>,
    /// 测速配置;例:
    /// [speedtest]
    /// spec = "cn"          # 默认选择(同 --speedtest SPEC 语法)
    /// [[speedtest.custom]] # 追加目录外的 Ookla 节点
    /// name = "自建"
    /// carrier = "telecom"
    /// host = "speedtest.example.com:8080"
    pub speedtest: Option<SpeedtestCfg>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SpeedtestCfg {
    pub spec: Option<String>,
    pub custom: Option<Vec<CustomNode>>,
}

#[derive(Debug, Deserialize)]
pub struct CustomNode {
    pub name: String,
    pub carrier: String,   // telecom/unicom/mobile/edu/hk/us/jp/sg
    pub host: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct AlwaysFlags {
    pub probe: Option<bool>,
    pub mail: Option<bool>,
    pub route: Option<bool>,
    pub dnsbl: Option<bool>,
}

/// 尝试从 ~/.config/ipano/config.toml 加载配置;文件不存在或解析失败时静默返回默认值
pub fn load() -> Config {
    let path = home_config_path();
    let content = match path.and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(s) => s,
        None => return Config::default(),
    };
    toml::from_str(&content).unwrap_or_default()
}

fn home_config_path() -> Option<std::path::PathBuf> {
    // 优先 $XDG_CONFIG_HOME,其次 $HOME/.config
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .ok()?;
    let p = base.join("ipano").join("config.toml");
    if p.exists() { Some(p) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_is_all_none() {
        let c = Config::default();
        assert!(c.lang.is_none());
        assert!(c.timeout.is_none());
        assert!(c.always.is_none());
    }

    #[test]
    fn config_parses_toml() {
        let src = r#"
lang = "en"
timeout = 10
no_color = true
ping0_token = "abc"

[always]
probe = true
mail = false
"#;
        let c: Config = toml::from_str(src).unwrap();
        assert_eq!(c.lang.as_deref(), Some("en"));
        assert_eq!(c.timeout, Some(10));
        assert_eq!(c.no_color, Some(true));
        assert_eq!(c.ping0_token.as_deref(), Some("abc"));
        let always = c.always.unwrap();
        assert_eq!(always.probe, Some(true));
        assert_eq!(always.mail, Some(false));
        assert_eq!(always.dnsbl, None);
    }

    #[test]
    fn config_parses_empty_toml() {
        let c: Config = toml::from_str("").unwrap();
        assert!(c.lang.is_none());
    }

    #[test]
    fn config_parses_speedtest() {
        let src = r#"
[speedtest]
spec = "cn"
[[speedtest.custom]]
name = "自建"
carrier = "telecom"
host = "speedtest.example.com:8080"
"#;
        let c: Config = toml::from_str(src).unwrap();
        let st = c.speedtest.unwrap();
        assert_eq!(st.spec.as_deref(), Some("cn"));
        assert_eq!(st.custom.unwrap()[0].host, "speedtest.example.com:8080");
    }

    #[test]
    fn load_returns_default_when_no_file() {
        // 正常环境下配置文件不存在,应返回 Default
        // (如果用户真的有这个文件该测试仍通过,只是 Config 非 Default)
        let _c = load(); // 不 panic 即可
    }
}
