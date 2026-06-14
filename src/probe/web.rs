use async_trait::async_trait;
use reqwest::Client;
use crate::probe::{Probe, ProbeResult, ProbeStatus};
use crate::probe::unlock_util::{between, extract_cookie};

// ===== Bing =====
// GET bing.com → 200 解析 Region:"XX";cn.bing.com → cn;403/451=封锁。
pub fn parse_bing(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Bing", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        if body.contains("cn.bing.com") {
            return ProbeResult::new("Bing", ProbeStatus::Unlocked, Some("cn".into()));
        }
        let region = between(body, "Region:\"", "\"").map(|r| r.to_lowercase());
        return ProbeResult::new("Bing", ProbeStatus::Unlocked, region);
    }
    ProbeResult::unknown("Bing")
}

pub struct Bing { pub base: String }
impl Default for Bing {
    fn default() -> Self { Bing { base: "https://www.bing.com".to_string() } }
}
#[async_trait]
impl Probe for Bing {
    fn name(&self) -> &'static str { "Bing" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_bing(st, &body)
            }
            Err(_) => ProbeResult::unknown("Bing"),
        }
    }
}

// ===== GoogleSearch =====
// GET google.com/search → 429=限流;"unusual traffic from"/403/451=封锁;200=可用。
pub fn parse_google_search(status: u16, body: &str) -> ProbeResult {
    if status == 429 { return ProbeResult::new("GoogleSearch", ProbeStatus::Unknown, None).with_info("Rate Limited"); }
    if status == 403 || status == 451 || body.contains("unusual traffic from") {
        return ProbeResult::new("GoogleSearch", ProbeStatus::Blocked, None);
    }
    if status == 200 { return ProbeResult::new("GoogleSearch", ProbeStatus::Unlocked, None); }
    ProbeResult::unknown("GoogleSearch")
}
pub struct GoogleSearch { pub base: String }
impl Default for GoogleSearch {
    fn default() -> Self { GoogleSearch { base: "https://www.google.com".to_string() } }
}
#[async_trait]
impl Probe for GoogleSearch {
    fn name(&self) -> &'static str { "GoogleSearch" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/search?q=ipano-probe-check", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_google_search(st, &body)
            }
            Err(_) => ProbeResult::unknown("GoogleSearch"),
        }
    }
}

// ===== Reddit =====
// GET reddit.com/ → 429=限流;200/302=可用;403+"been blocked"=封锁。
pub fn parse_reddit(status: u16, body: &str) -> ProbeResult {
    if status == 429 { return ProbeResult::new("Reddit", ProbeStatus::Unknown, None).with_info("Rate Limited"); }
    if status == 200 || status == 302 { return ProbeResult::new("Reddit", ProbeStatus::Unlocked, None); }
    if status == 403 && body.contains("been blocked") { return ProbeResult::new("Reddit", ProbeStatus::Blocked, None); }
    ProbeResult::unknown("Reddit")
}
pub struct Reddit { pub base: String }
impl Default for Reddit {
    fn default() -> Self { Reddit { base: "https://www.reddit.com".to_string() } }
}
#[async_trait]
impl Probe for Reddit {
    fn name(&self) -> &'static str { "Reddit" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_reddit(st, &body)
            }
            Err(_) => ProbeResult::unknown("Reddit"),
        }
    }
}

// ===== Wikipedia(可编辑性)=====
// GET 编辑页 → 200=可编辑;429=限流;其余=封锁。
pub fn parse_wikipedia(status: u16) -> ProbeResult {
    match status {
        200 => ProbeResult::new("Wikipedia", ProbeStatus::Unlocked, None),
        429 => ProbeResult::new("Wikipedia", ProbeStatus::Unknown, None).with_info("Rate Limited"),
        _ => ProbeResult::new("Wikipedia", ProbeStatus::Blocked, None),
    }
}
pub struct Wikipedia { pub base: String }
impl Default for Wikipedia {
    fn default() -> Self { Wikipedia { base: "https://zh.wikipedia.org".to_string() } }
}
#[async_trait]
impl Probe for Wikipedia {
    fn name(&self) -> &'static str { "Wikipedia" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/w/index.php?title=Wikipedia:%E6%B2%99%E7%9B%92&action=edit", self.base);
        match client.get(&url).send().await {
            Ok(resp) => parse_wikipedia(resp.status().as_u16()),
            Err(_) => ProbeResult::unknown("Wikipedia"),
        }
    }
}

// ===== OneTrust(地理)=====
// GET dnsfeed → 解析 country(+stateName)。无 country=封锁。
pub fn parse_onetrust(status: u16, body: &str) -> ProbeResult {
    if status != 200 { return ProbeResult::unknown("OneTrust"); }
    match between(body, "\"country\":\"", "\"") {
        Some(c) => {
            let region = match between(body, "\"stateName\":\"", "\"") {
                Some(s) if !s.is_empty() => format!("{c} {s}"),
                _ => c.to_string(),
            };
            ProbeResult::new("OneTrust", ProbeStatus::Unlocked, Some(region))
        }
        None => ProbeResult::new("OneTrust", ProbeStatus::Blocked, None),
    }
}
pub struct OneTrust { pub base: String }
impl Default for OneTrust {
    fn default() -> Self { OneTrust { base: "https://geolocation.onetrust.com".to_string() } }
}
#[async_trait]
impl Probe for OneTrust {
    fn name(&self) -> &'static str { "OneTrust" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/cookieconsentpub/v1/geo/location/dnsfeed", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_onetrust(st, &body)
            }
            Err(_) => ProbeResult::unknown("OneTrust"),
        }
    }
}

// ===== Apple(区域)=====
// GET pep/gcc → 返回两字母国家码。
pub fn parse_apple(status: u16, body: &str) -> ProbeResult {
    let code = body.trim();
    if status == 200 && code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) {
        return ProbeResult::new("Apple", ProbeStatus::Unlocked, Some(code.to_lowercase()));
    }
    ProbeResult::new("Apple", ProbeStatus::Blocked, None)
}
pub struct Apple { pub base: String }
impl Default for Apple {
    fn default() -> Self { Apple { base: "https://gspe1-ssl.ls.apple.com".to_string() } }
}
#[async_trait]
impl Probe for Apple {
    fn name(&self) -> &'static str { "Apple" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/pep/gcc", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_apple(st, &body)
            }
            Err(_) => ProbeResult::unknown("Apple"),
        }
    }
}

// ===== Steam(商店区域)=====
// Set-Cookie 取 steamCountry= 前两位作 region;有则可用 + 社区可达。
pub fn parse_steam(set_cookie: &str) -> ProbeResult {
    match extract_cookie(set_cookie, "steamCountry") {
        Some(v) if v.len() >= 2 => ProbeResult::new("Steam", ProbeStatus::Unlocked,
            Some(v[..2].to_lowercase())).with_info("Community Available"),
        _ => ProbeResult::new("Steam", ProbeStatus::Blocked, None),
    }
}
pub struct Steam { pub base: String }
impl Default for Steam {
    fn default() -> Self { Steam { base: "https://store.steampowered.com".to_string() } }
}
#[async_trait]
impl Probe for Steam {
    fn name(&self) -> &'static str { "Steam" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let cookies = resp.headers().get_all("set-cookie").iter()
                    .filter_map(|v| v.to_str().ok()).collect::<Vec<_>>().join("; ");
                parse_steam(&cookies)
            }
            Err(_) => ProbeResult::unknown("Steam"),
        }
    }
}

// ===== MetaAI =====
// GET meta.ai/ajax(带浏览器 UA)→ 400/404=可用;200=地区封锁;403=封锁。
pub fn parse_meta_ai(status: u16) -> ProbeResult {
    match status {
        400 | 404 => ProbeResult::new("MetaAI", ProbeStatus::Unlocked, None),
        200 => ProbeResult::new("MetaAI", ProbeStatus::Blocked, None).with_info("GeoBlocked"),
        403 | 451 => ProbeResult::new("MetaAI", ProbeStatus::Blocked, None),
        _ => ProbeResult::unknown("MetaAI"),
    }
}
pub struct MetaAI { pub base: String }
impl Default for MetaAI {
    fn default() -> Self { MetaAI { base: "https://www.meta.ai".to_string() } }
}
#[async_trait]
impl Probe for MetaAI {
    fn name(&self) -> &'static str { "MetaAI" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/ajax", self.base);
        let req = client.get(&url)
            .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36");
        match req.send().await {
            Ok(resp) => parse_meta_ai(resp.status().as_u16()),
            Err(_) => ProbeResult::unknown("MetaAI"),
        }
    }
}

// ===== SonyLiv =====
// GET sonyliv.com/signin → 403=封锁;200 解析 country_code:"XX"=可用;无码=Unknown。
pub fn parse_sonyliv(status: u16, body: &str) -> ProbeResult {
    if status == 403 { return ProbeResult::new("SonyLiv", ProbeStatus::Blocked, None); }
    if status == 200 {
        if let Some(cc) = between(body, "country_code:\"", "\"") {
            return ProbeResult::new("SonyLiv", ProbeStatus::Unlocked, Some(cc.to_lowercase()));
        }
    }
    ProbeResult::unknown("SonyLiv")
}
pub struct SonyLiv { pub base: String }
impl Default for SonyLiv {
    fn default() -> Self { SonyLiv { base: "https://www.sonyliv.com".to_string() } }
}
#[async_trait]
impl Probe for SonyLiv {
    fn name(&self) -> &'static str { "SonyLiv" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/signin", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_sonyliv(st, &body)
            }
            Err(_) => ProbeResult::unknown("SonyLiv"),
        }
    }
}

// ===== GooglePlay =====
// GET play.google.com/store/games → 解析 region(两种模式);cn=封锁;有码=可用。
fn extract_google_play_region(body: &str) -> Option<String> {
    between(body, "\"zQmIje\":\"", "\"")
        .or_else(|| between(body, "<div class=\"yVZQTb\">", "<"))
        .map(|s| s.trim().to_string())
}
pub fn parse_google_play(status: u16, body: &str) -> ProbeResult {
    if status != 200 { return ProbeResult::unknown("GooglePlay"); }
    match extract_google_play_region(body) {
        Some(r) if r.eq_ignore_ascii_case("cn") =>
            ProbeResult::new("GooglePlay", ProbeStatus::Blocked, Some("cn".into())),
        Some(r) if !r.is_empty() =>
            ProbeResult::new("GooglePlay", ProbeStatus::Unlocked, Some(r.to_lowercase())),
        _ => ProbeResult::new("GooglePlay", ProbeStatus::Blocked, None),
    }
}
pub struct GooglePlay { pub base: String }
impl Default for GooglePlay {
    fn default() -> Self { GooglePlay { base: "https://play.google.com".to_string() } }
}
#[async_trait]
impl Probe for GooglePlay {
    fn name(&self) -> &'static str { "GooglePlay" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/store/games", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_google_play(st, &body)
            }
            Err(_) => ProbeResult::unknown("GooglePlay"),
        }
    }
}

// ===== InstagramMusic(授权音频)=====
// POST instagram.com/api/graphql(固定 payload,含会过期的 doc_id)。
// 200+含媒体数据=可用;200+错误/login=封锁;429=限流;其余=Unknown。
const IG_PAYLOAD: &str = "av=0&__d=www&__user=0&__a=1&__req=3&doc_id=10015901848480474&variables=%7B%22shortcode%22%3A%22C2YEAdOh9AB%22%7D&fb_api_req_friendly_name=PolarisPostActionLoadPostQueryQuery&server_timestamps=true";

pub fn parse_instagram(status: u16, body: &str) -> ProbeResult {
    if status == 429 {
        return ProbeResult::new("InstagramMusic", ProbeStatus::Unknown, None).with_info("Too Many Requests");
    }
    if status == 200 {
        if body.contains("login_required") || body.contains("\"errors\"") || body.contains("usepc") {
            return ProbeResult::new("InstagramMusic", ProbeStatus::Blocked, None);
        }
        if body.contains("xdt_api") || body.contains("\"data\"") {
            return ProbeResult::new("InstagramMusic", ProbeStatus::Unlocked, None);
        }
    }
    ProbeResult::unknown("InstagramMusic")
}
pub struct InstagramMusic { pub base: String }
impl Default for InstagramMusic {
    fn default() -> Self { InstagramMusic { base: "https://www.instagram.com".to_string() } }
}
#[async_trait]
impl Probe for InstagramMusic {
    fn name(&self) -> &'static str { "InstagramMusic" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/api/graphql", self.base);
        let req = client.post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(reqwest::header::ORIGIN, "https://www.instagram.com")
            .header(reqwest::header::REFERER, "https://www.instagram.com/p/C2YEAdOh9AB/")
            .header("X-FB-Friendly-Name", "PolarisPostActionLoadPostQueryQuery")
            .body(IG_PAYLOAD);
        match req.send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_instagram(st, &body)
            }
            Err(_) => ProbeResult::unknown("InstagramMusic"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bing_region_parse() {
        let r = parse_bing(200, r#"x Region:"US" y"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        assert_eq!(parse_bing(403, "").status, ProbeStatus::Blocked);
    }

    #[test]
    fn bing_cn_detect() {
        let r = parse_bing(200, "redirect to cn.bing.com here");
        assert_eq!(r.region.as_deref(), Some("cn"));
    }

    #[test]
    fn google_search_branches() {
        assert_eq!(parse_google_search(429, "").status, ProbeStatus::Unknown);
        assert_eq!(parse_google_search(429, "").info.as_deref(), Some("Rate Limited"));
        assert_eq!(parse_google_search(200, "results").status, ProbeStatus::Unlocked);
        assert_eq!(parse_google_search(200, "unusual traffic from your network").status, ProbeStatus::Blocked);
        assert_eq!(parse_google_search(403, "").status, ProbeStatus::Blocked);
    }

    #[test]
    fn reddit_branches() {
        assert_eq!(parse_reddit(200, "").status, ProbeStatus::Unlocked);
        assert_eq!(parse_reddit(302, "").status, ProbeStatus::Unlocked);
        assert_eq!(parse_reddit(403, "you have been blocked").status, ProbeStatus::Blocked);
        assert_eq!(parse_reddit(429, "").info.as_deref(), Some("Rate Limited"));
    }

    #[test]
    fn wikipedia_branches() {
        assert_eq!(parse_wikipedia(200).status, ProbeStatus::Unlocked);
        assert_eq!(parse_wikipedia(429).info.as_deref(), Some("Rate Limited"));
        assert_eq!(parse_wikipedia(403).status, ProbeStatus::Blocked);
    }

    #[test]
    fn onetrust_parse() {
        let body = r#"{"country":"US","stateName":"California"}"#;
        let r = parse_onetrust(200, body);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US California"));
        assert_eq!(parse_onetrust(200, "{}").status, ProbeStatus::Blocked);
    }

    #[test]
    fn apple_parse() {
        let r = parse_apple(200, "US\n");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        assert_eq!(parse_apple(200, "US|extra").status, ProbeStatus::Blocked);
    }

    #[test]
    fn steam_parse() {
        let r = parse_steam("foo=bar; steamCountry=US%7Cabc; path=/");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        assert_eq!(r.info.as_deref(), Some("Community Available"));
        assert_eq!(parse_steam("nothing=here").status, ProbeStatus::Blocked);
    }

    #[test]
    fn meta_ai_branches() {
        assert_eq!(parse_meta_ai(404).status, ProbeStatus::Unlocked);
        assert_eq!(parse_meta_ai(400).status, ProbeStatus::Unlocked);
        assert_eq!(parse_meta_ai(200).status, ProbeStatus::Blocked);
        assert_eq!(parse_meta_ai(200).info.as_deref(), Some("GeoBlocked"));
        assert_eq!(parse_meta_ai(403).status, ProbeStatus::Blocked);
        assert_eq!(parse_meta_ai(500).status, ProbeStatus::Unknown);
    }

    #[test]
    fn sonyliv_branches() {
        let r = parse_sonyliv(200, r#"...country_code:"IN"..."#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("in"));
        assert_eq!(parse_sonyliv(403, "").status, ProbeStatus::Blocked);
        assert_eq!(parse_sonyliv(200, "no code here").status, ProbeStatus::Unknown);
    }

    #[test]
    fn google_play_branches() {
        let r = parse_google_play(200, r#"x "zQmIje":"US" y"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        assert_eq!(parse_google_play(200, r#""zQmIje":"CN""#).status, ProbeStatus::Blocked);
        assert_eq!(parse_google_play(200, "nothing").status, ProbeStatus::Blocked);
        assert_eq!(parse_google_play(500, "").status, ProbeStatus::Unknown);
    }

    #[test]
    fn instagram_branches() {
        assert_eq!(parse_instagram(200, r#"{"data":{"xdt_api__v1__media__shortcode__web_info":{}}}"#).status, ProbeStatus::Unlocked);
        assert_eq!(parse_instagram(200, r#"{"errors":["login_required"]}"#).status, ProbeStatus::Blocked);
        assert_eq!(parse_instagram(429, "").status, ProbeStatus::Unknown);
        assert_eq!(parse_instagram(429, "").info.as_deref(), Some("Too Many Requests"));
        assert_eq!(parse_instagram(500, "").status, ProbeStatus::Unknown);
    }
}
