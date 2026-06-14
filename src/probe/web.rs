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
}
