use async_trait::async_trait;
use reqwest::Client;
use crate::probe::{Probe, ProbeResult, ProbeStatus};

// ─────────────────────────────────────────────
// 公共工具
// ─────────────────────────────────────────────

/// 在响应体中按 key 前缀顺序搜索 2-3 字母国家码
fn extract_cc(body: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(idx) = body.find(key) {
            let tail = &body[idx + key.len()..];
            let cc: String = tail.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            if (2..=3).contains(&cc.len()) {
                return Some(cc.to_uppercase());
            }
        }
    }
    None
}

// ─────────────────────────────────────────────
// 1. Netflix
// 方法:请求非自制剧标题页;200=完全解锁,404=仅自制剧,403/451=封锁
// ─────────────────────────────────────────────

pub fn classify_netflix(status: u16, body: &str) -> ProbeResult {
    match status {
        200 => {
            let region = extract_cc(body, &[r#""requestCountry":""#, r#""countryCode":""#]);
            ProbeResult::new("Netflix", ProbeStatus::Unlocked, region)
        }
        404 => ProbeResult::new("Netflix", ProbeStatus::Restricted, None),
        403 | 451 => ProbeResult::new("Netflix", ProbeStatus::Blocked, None),
        _ => ProbeResult::unknown("Netflix"),
    }
}

pub struct Netflix { pub base: String }
impl Default for Netflix {
    fn default() -> Self { Netflix { base: "https://www.netflix.com".into() } }
}
#[async_trait]
impl Probe for Netflix {
    fn name(&self) -> &'static str { "Netflix" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/title/81280792", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_netflix(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 2. YouTube Premium
// 方法:请求 /premium 页;含 "Premium is not available" → 封锁;解析 countryCode
// ─────────────────────────────────────────────

pub fn classify_youtube(body: &str) -> ProbeResult {
    if body.contains("Premium is not available") || body.contains("不可用") {
        return ProbeResult::new("YouTube Premium", ProbeStatus::Blocked, None);
    }
    let region = extract_cc(body, &[r#""countryCode":""#]);
    ProbeResult::new("YouTube Premium", ProbeStatus::Unlocked, region)
}

pub struct YouTube { pub base: String }
impl Default for YouTube {
    fn default() -> Self { YouTube { base: "https://www.youtube.com".into() } }
}
#[async_trait]
impl Probe for YouTube {
    fn name(&self) -> &'static str { "YouTube Premium" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/premium", self.base);
        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => classify_youtube(&body),
                Err(_) => ProbeResult::unknown(self.name()),
            },
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 3. Disney+
// 方法:GET 主页;最终 URL 含 "not-available" 或 body 含封锁词 → Blocked;
//       200 → 尝试提取 countryCode
// ─────────────────────────────────────────────

pub fn classify_disney(status: u16, body: &str, final_url: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Disney+", ProbeStatus::Blocked, None);
    }
    if final_url.contains("not-available")
        || body.contains("Disney+ is not available")
        || body.contains("not available in your region")
    {
        return ProbeResult::new("Disney+", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        let region = extract_cc(body, &[r#""countryCode":""#, r#""country":""#, r#""geoCountry":""#]);
        ProbeResult::new("Disney+", ProbeStatus::Unlocked, region)
    } else {
        ProbeResult::unknown("Disney+")
    }
}

pub struct DisneyPlus { pub base: String }
impl Default for DisneyPlus {
    fn default() -> Self { DisneyPlus { base: "https://www.disneyplus.com".into() } }
}
#[async_trait]
impl Probe for DisneyPlus {
    fn name(&self) -> &'static str { "Disney+" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let final_url = resp.url().to_string();
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_disney(status, &body, &final_url)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 4. HBO Max
// 方法:GET https://www.max.com;检查封锁词 / 状态码
// ─────────────────────────────────────────────

pub fn classify_hbo(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("HBO Max", ProbeStatus::Blocked, None);
    }
    if body.contains("not available in your region") || body.contains("not available in your country") {
        return ProbeResult::new("HBO Max", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        let region = extract_cc(body, &[r#""countryCode":""#, r#""country":""#]);
        ProbeResult::new("HBO Max", ProbeStatus::Unlocked, region)
    } else {
        ProbeResult::unknown("HBO Max")
    }
}

pub struct HboMax { pub base: String }
impl Default for HboMax {
    fn default() -> Self { HboMax { base: "https://www.max.com".into() } }
}
#[async_trait]
impl Probe for HboMax {
    fn name(&self) -> &'static str { "HBO Max" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_hbo(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 5. Hulu (仅美国)
// 方法:GET 主页;200 且无封锁词 → Unlocked(US);403 / 封锁词 → Blocked
// ─────────────────────────────────────────────

pub fn classify_hulu(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Hulu", ProbeStatus::Blocked, None);
    }
    if body.contains("not available") || body.contains("not supported in your region") {
        return ProbeResult::new("Hulu", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        ProbeResult::new("Hulu", ProbeStatus::Unlocked, Some("US".into()))
    } else {
        ProbeResult::unknown("Hulu")
    }
}

pub struct Hulu { pub base: String }
impl Default for Hulu {
    fn default() -> Self { Hulu { base: "https://www.hulu.com".into() } }
}
#[async_trait]
impl Probe for Hulu {
    fn name(&self) -> &'static str { "Hulu" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_hulu(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 6. Amazon Prime Video
// 方法:GET https://www.primevideo.com;200 → 解锁;403 → 封锁
// ─────────────────────────────────────────────

pub fn classify_prime(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Prime Video", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        let region = extract_cc(body, &[r#""country":""#, r#""countryCode":""#, r#""marketplaceId":"A"#]);
        ProbeResult::new("Prime Video", ProbeStatus::Unlocked, region)
    } else {
        ProbeResult::unknown("Prime Video")
    }
}

pub struct PrimeVideo { pub base: String }
impl Default for PrimeVideo {
    fn default() -> Self { PrimeVideo { base: "https://www.primevideo.com".into() } }
}
#[async_trait]
impl Probe for PrimeVideo {
    fn name(&self) -> &'static str { "Prime Video" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_prime(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 7. Bilibili CN(大陆版)
// 方法:GET /x/web-interface/zone → JSON;country_code=CN → 解锁
// ─────────────────────────────────────────────

pub fn classify_bilibili_cn(body: &str) -> ProbeResult {
    if body.contains(r#""country_code":"CN""#) || body.contains(r#""country_code": "CN""#) {
        ProbeResult::new("Bilibili CN", ProbeStatus::Unlocked, Some("CN".into()))
    } else if body.contains(r#""code":0"#) {
        let region = extract_cc(body, &[r#""country_code":""#]);
        ProbeResult::new("Bilibili CN", ProbeStatus::Blocked, region)
    } else {
        ProbeResult::unknown("Bilibili CN")
    }
}

pub struct BilibiliCn { pub base: String }
impl Default for BilibiliCn {
    fn default() -> Self { BilibiliCn { base: "https://api.bilibili.com".into() } }
}
#[async_trait]
impl Probe for BilibiliCn {
    fn name(&self) -> &'static str { "Bilibili CN" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/x/web-interface/zone", self.base);
        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => classify_bilibili_cn(&body),
                Err(_) => ProbeResult::unknown(self.name()),
            },
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 8. Bilibili HK/TW(港澳台)
// 方法:请求港澳台独家番剧 ep playurl;code:0 → 解锁;-10403 → 大陆 IP 被拦截
// ─────────────────────────────────────────────

pub fn classify_bilibili_hktw(body: &str) -> ProbeResult {
    // 大陆 IP 访问港澳台内容通常返回 -10403
    if body.contains(r#""code":-10403"#)
        || body.contains("大陆地区")
        || body.contains("restricted")
    {
        ProbeResult::new("Bilibili HK/TW", ProbeStatus::Blocked, None)
    } else if body.contains(r#""code":0"#) {
        ProbeResult::new("Bilibili HK/TW", ProbeStatus::Unlocked, None)
    } else {
        ProbeResult::unknown("Bilibili HK/TW")
    }
}

pub struct BilibiliHkTw { pub base: String }
impl Default for BilibiliHkTw {
    fn default() -> Self { BilibiliHkTw { base: "https://api.bilibili.com".into() } }
}
#[async_trait]
impl Probe for BilibiliHkTw {
    fn name(&self) -> &'static str { "Bilibili HK/TW" }
    async fn check(&self, client: &Client) -> ProbeResult {
        // ep_id=374717:港澳台独家动漫(《白箱》港澳台版)
        let url = format!(
            "{}/pgc/player/web/v2/playurl?ep_id=374717&support_multi_area=1&qn=0&fnver=0&fnval=16",
            self.base
        );
        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => classify_bilibili_hktw(&body),
                Err(_) => ProbeResult::unknown(self.name()),
            },
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 9. AbemaTV
// 方法:GET /v1/ip/check → JSON;country=JP → 解锁
// ─────────────────────────────────────────────

pub fn classify_abema(body: &str) -> ProbeResult {
    if body.contains(r#""country":"JP""#) {
        ProbeResult::new("AbemaTV", ProbeStatus::Unlocked, Some("JP".into()))
    } else {
        let region = extract_cc(body, &[r#""country":""#]);
        ProbeResult::new("AbemaTV", ProbeStatus::Blocked, region)
    }
}

pub struct AbemaTV { pub base: String }
impl Default for AbemaTV {
    fn default() -> Self { AbemaTV { base: "https://api.abema.io".into() } }
}
#[async_trait]
impl Probe for AbemaTV {
    fn name(&self) -> &'static str { "AbemaTV" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/v1/ip/check?device_type=pc", self.base);
        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => classify_abema(&body),
                Err(_) => ProbeResult::unknown(self.name()),
            },
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 10. DAZN
// 方法:GET 启动端点;isAllowed:true → 解锁,否则封锁
// ─────────────────────────────────────────────

pub fn classify_dazn(body: &str) -> ProbeResult {
    if body.contains(r#""isAllowed":true"#) {
        let region = extract_cc(body, &[r#""Country":""#, r#""CountryCode":""#, r#""country":""#]);
        ProbeResult::new("DAZN", ProbeStatus::Unlocked, region)
    } else if body.contains(r#""isAllowed":false"#) || body.contains("not available") {
        let region = extract_cc(body, &[r#""Country":""#, r#""CountryCode":""#]);
        ProbeResult::new("DAZN", ProbeStatus::Blocked, region)
    } else {
        ProbeResult::unknown("DAZN")
    }
}

pub struct Dazn { pub base: String }
impl Default for Dazn {
    fn default() -> Self { Dazn { base: "https://startup.core.indazn.com".into() } }
}
#[async_trait]
impl Probe for Dazn {
    fn name(&self) -> &'static str { "DAZN" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!(
            "{}/misl/v5/Startup?keycloak=false&IFA=&language=en&Platform=web&PlatformId=2&Manufacturer=google&proposedCountry=US&isTV=false",
            self.base
        );
        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => classify_dazn(&body),
                Err(_) => ProbeResult::unknown(self.name()),
            },
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 11. BBC iPlayer
// 方法:GET mediaselector 接口;含 "media" 且无 selectionunavailable → 英国解锁
// ─────────────────────────────────────────────

pub fn classify_bbc(status: u16, body: &str) -> ProbeResult {
    if status == 200 && body.contains(r#""media""#) && !body.contains("selectionunavailable") {
        ProbeResult::new("BBC iPlayer", ProbeStatus::Unlocked, Some("GB".into()))
    } else {
        ProbeResult::new("BBC iPlayer", ProbeStatus::Blocked, None)
    }
}

pub struct BbcIplayer { pub base: String }
impl Default for BbcIplayer {
    fn default() -> Self {
        BbcIplayer { base: "https://open.live.bbc.co.uk".into() }
    }
}
#[async_trait]
impl Probe for BbcIplayer {
    fn name(&self) -> &'static str { "BBC iPlayer" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!(
            "{}/mediaselector/6/select/version/2.0/mediaset/iptv-all/vpid/bbc_one_london/format/json/",
            self.base
        );
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_bbc(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 12. Crunchyroll
// 方法:GET 主页;200 且无封锁词 → 解锁
// ─────────────────────────────────────────────

pub fn classify_crunchyroll(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Crunchyroll", ProbeStatus::Blocked, None);
    }
    if body.contains("not available in your region") || body.contains("Crunchyroll is not available") {
        return ProbeResult::new("Crunchyroll", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        ProbeResult::new("Crunchyroll", ProbeStatus::Unlocked, None)
    } else {
        ProbeResult::unknown("Crunchyroll")
    }
}

pub struct Crunchyroll { pub base: String }
impl Default for Crunchyroll {
    fn default() -> Self { Crunchyroll { base: "https://www.crunchyroll.com".into() } }
}
#[async_trait]
impl Probe for Crunchyroll {
    fn name(&self) -> &'static str { "Crunchyroll" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_crunchyroll(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 13. Paramount+
// 方法:GET 主页;200 且最终 URL 无 "not-available" → 解锁
// ─────────────────────────────────────────────

pub fn classify_paramount(status: u16, final_url: &str) -> ProbeResult {
    if status == 403 || status == 451 || final_url.contains("not-available") || final_url.contains("unavailable") {
        ProbeResult::new("Paramount+", ProbeStatus::Blocked, None)
    } else if status == 200 {
        ProbeResult::new("Paramount+", ProbeStatus::Unlocked, None)
    } else {
        ProbeResult::unknown("Paramount+")
    }
}

pub struct ParamountPlus { pub base: String }
impl Default for ParamountPlus {
    fn default() -> Self { ParamountPlus { base: "https://www.paramountplus.com".into() } }
}
#[async_trait]
impl Probe for ParamountPlus {
    fn name(&self) -> &'static str { "Paramount+" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let final_url = resp.url().to_string();
                let status = resp.status().as_u16();
                classify_paramount(status, &final_url)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 14. Peacock (仅美国)
// 方法:GET 主页;200 且无封锁词 → Unlocked(US)
// ─────────────────────────────────────────────

pub fn classify_peacock(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Peacock", ProbeStatus::Blocked, None);
    }
    if body.contains("not available") || body.contains("not supported in your region") {
        return ProbeResult::new("Peacock", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        ProbeResult::new("Peacock", ProbeStatus::Unlocked, Some("US".into()))
    } else {
        ProbeResult::unknown("Peacock")
    }
}

pub struct Peacock { pub base: String }
impl Default for Peacock {
    fn default() -> Self { Peacock { base: "https://www.peacocktv.com".into() } }
}
#[async_trait]
impl Probe for Peacock {
    fn name(&self) -> &'static str { "Peacock" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_peacock(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 15. Discovery+
// 方法:GET 主页;200 且最终 URL 无封锁标记 → 解锁
// ─────────────────────────────────────────────

pub fn classify_discovery(status: u16, final_url: &str) -> ProbeResult {
    if status == 403 || status == 451
        || final_url.contains("not-available")
        || final_url.contains("unavailable")
    {
        ProbeResult::new("Discovery+", ProbeStatus::Blocked, None)
    } else if status == 200 {
        ProbeResult::new("Discovery+", ProbeStatus::Unlocked, None)
    } else {
        ProbeResult::unknown("Discovery+")
    }
}

pub struct DiscoveryPlus { pub base: String }
impl Default for DiscoveryPlus {
    fn default() -> Self { DiscoveryPlus { base: "https://www.discoveryplus.com".into() } }
}
#[async_trait]
impl Probe for DiscoveryPlus {
    fn name(&self) -> &'static str { "Discovery+" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let final_url = resp.url().to_string();
                let status = resp.status().as_u16();
                classify_discovery(status, &final_url)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 16. Spotify
// 方法:GET https://open.spotify.com;200 → 解锁,尝试提取国家码
// ─────────────────────────────────────────────

pub fn classify_spotify(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Spotify", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        // HTML 中可能含 "countryCode":"JP" 或 og:locale="en_US"
        let region = extract_cc(body, &[r#""countryCode":""#, r#""country":""#]);
        ProbeResult::new("Spotify", ProbeStatus::Unlocked, region)
    } else {
        ProbeResult::unknown("Spotify")
    }
}

pub struct Spotify { pub base: String }
impl Default for Spotify {
    fn default() -> Self { Spotify { base: "https://open.spotify.com".into() } }
}
#[async_trait]
impl Probe for Spotify {
    fn name(&self) -> &'static str { "Spotify" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_spotify(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 17. TVB Anywhere+
// 方法:GET 主页;200 → 解锁;403/451 → 封锁
// ─────────────────────────────────────────────

pub fn classify_tvb(status: u16) -> ProbeResult {
    match status {
        200 => ProbeResult::new("TVB Anywhere+", ProbeStatus::Unlocked, None),
        403 | 451 => ProbeResult::new("TVB Anywhere+", ProbeStatus::Blocked, None),
        _ => ProbeResult::unknown("TVB Anywhere+"),
    }
}

pub struct TvbAnywhere { pub base: String }
impl Default for TvbAnywhere {
    fn default() -> Self { TvbAnywhere { base: "https://www.tvbanywhere.com.hk".into() } }
}
#[async_trait]
impl Probe for TvbAnywhere {
    fn name(&self) -> &'static str { "TVB Anywhere+" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => classify_tvb(resp.status().as_u16()),
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// 18. Funimation
// 方法:GET 主页;200 且无封锁词 → 解锁
// ─────────────────────────────────────────────

pub fn classify_funimation(status: u16, body: &str) -> ProbeResult {
    if status == 403 || status == 451 {
        return ProbeResult::new("Funimation", ProbeStatus::Blocked, None);
    }
    if body.contains("not available in your region") || body.contains("Funimation is not available") {
        return ProbeResult::new("Funimation", ProbeStatus::Blocked, None);
    }
    if status == 200 {
        ProbeResult::new("Funimation", ProbeStatus::Unlocked, None)
    } else {
        ProbeResult::unknown("Funimation")
    }
}

pub struct Funimation { pub base: String }
impl Default for Funimation {
    fn default() -> Self { Funimation { base: "https://www.funimation.com".into() } }
}
#[async_trait]
impl Probe for Funimation {
    fn name(&self) -> &'static str { "Funimation" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                classify_funimation(status, &body)
            }
            Err(_) => ProbeResult::unknown(self.name()),
        }
    }
}

// ─────────────────────────────────────────────
// iQIYI / KOCOWA / Viu / TikTok(UnlockTests 对标,阶段 B)
// ─────────────────────────────────────────────

// ===== iQIYI =====
// GET iq.com → 200=可用;403/451=封锁。
pub fn classify_iqiyi(status: u16) -> ProbeResult {
    match status {
        200 => ProbeResult::new("iQIYI", ProbeStatus::Unlocked, None),
        403 | 451 => ProbeResult::new("iQIYI", ProbeStatus::Blocked, None),
        _ => ProbeResult::unknown("iQIYI"),
    }
}
pub struct IQiYi { pub base: String }
impl Default for IQiYi {
    fn default() -> Self { IQiYi { base: "https://www.iq.com".to_string() } }
}
#[async_trait]
impl Probe for IQiYi {
    fn name(&self) -> &'static str { "iQIYI" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => classify_iqiyi(resp.status().as_u16()),
            Err(_) => ProbeResult::unknown("iQIYI"),
        }
    }
}

// ===== KOCOWA =====
// GET kocowa.com/ → body 含 "is not available in your region or country" 或 403=封锁;200=可用。
pub fn parse_kocowa(status: u16, body: &str) -> ProbeResult {
    if status == 403 || body.contains("is not available in your region or country") {
        return ProbeResult::new("KOCOWA", ProbeStatus::Blocked, None);
    }
    if status == 200 { return ProbeResult::new("KOCOWA", ProbeStatus::Unlocked, None); }
    ProbeResult::unknown("KOCOWA")
}
pub struct Kocowa { pub base: String }
impl Default for Kocowa {
    fn default() -> Self { Kocowa { base: "https://www.kocowa.com".to_string() } }
}
#[async_trait]
impl Probe for Kocowa {
    fn name(&self) -> &'static str { "KOCOWA" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_kocowa(st, &body)
            }
            Err(_) => ProbeResult::unknown("KOCOWA"),
        }
    }
}

// ===== Viu =====
// GET viu.com → 200=可用;403/451=封锁。
pub fn classify_viu(status: u16) -> ProbeResult {
    match status {
        200 => ProbeResult::new("Viu", ProbeStatus::Unlocked, None),
        403 | 451 => ProbeResult::new("Viu", ProbeStatus::Blocked, None),
        _ => ProbeResult::unknown("Viu"),
    }
}
pub struct Viu { pub base: String }
impl Default for Viu {
    fn default() -> Self { Viu { base: "https://www.viu.com".to_string() } }
}
#[async_trait]
impl Probe for Viu {
    fn name(&self) -> &'static str { "Viu" }
    async fn check(&self, client: &Client) -> ProbeResult {
        match client.get(&self.base).send().await {
            Ok(resp) => classify_viu(resp.status().as_u16()),
            Err(_) => ProbeResult::unknown("Viu"),
        }
    }
}

// ===== TikTok(含地区)=====
// GET tiktok.com/explore → 解析 "region":"XX";含 /hk/notfound=封锁;非 200=封锁。
pub fn parse_tiktok(status: u16, body: &str) -> ProbeResult {
    use crate::probe::unlock_util::between;
    if status != 200 { return ProbeResult::new("TikTok", ProbeStatus::Blocked, None); }
    if body.contains("https://www.tiktok.com/hk/notfound") {
        return ProbeResult::new("TikTok", ProbeStatus::Blocked, Some("hk".into()));
    }
    match between(body, "\"region\":\"", "\"") {
        Some(r) if !r.is_empty() => ProbeResult::new("TikTok", ProbeStatus::Unlocked, Some(r.to_lowercase())),
        _ => ProbeResult::new("TikTok", ProbeStatus::Blocked, None),
    }
}
pub struct TikTok { pub base: String }
impl Default for TikTok {
    fn default() -> Self { TikTok { base: "https://www.tiktok.com".to_string() } }
}
#[async_trait]
impl Probe for TikTok {
    fn name(&self) -> &'static str { "TikTok" }
    async fn check(&self, client: &Client) -> ProbeResult {
        let url = format!("{}/explore", self.base);
        match client.get(&url).send().await {
            Ok(resp) => {
                let st = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                parse_tiktok(st, &body)
            }
            Err(_) => ProbeResult::unknown("TikTok"),
        }
    }
}

// ─────────────────────────────────────────────
// 单元测试(TDD)
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_cc ──
    #[test]
    fn extract_cc_finds_two_letter() {
        assert_eq!(extract_cc(r#"{"countryCode":"JP"}"#, &[r#""countryCode":""#]), Some("JP".into()));
    }

    #[test]
    fn extract_cc_uppercase() {
        assert_eq!(extract_cc(r#""country":"us""#, &[r#""country":""#]), Some("US".into()));
    }

    #[test]
    fn extract_cc_skips_long() {
        // 4-letter sequence should be skipped
        assert_eq!(extract_cc(r#""country":"GLOB""#, &[r#""country":""#]), None);
    }

    #[test]
    fn extract_cc_tries_keys_in_order() {
        let body = r#"{"other":"XX","countryCode":"JP"}"#;
        // second key should find it
        assert_eq!(extract_cc(body, &[r#""missing":""#, r#""countryCode":""#]), Some("JP".into()));
    }

    // ── Netflix ──
    #[test]
    fn netflix_200_unlocked_with_region() {
        let r = classify_netflix(200, r#"{"requestCountry":"US"}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    #[test]
    fn netflix_200_unlocked_no_region() {
        let r = classify_netflix(200, "no region here");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region, None);
    }

    #[test]
    fn netflix_404_restricted() {
        let r = classify_netflix(404, "");
        assert_eq!(r.status, ProbeStatus::Restricted);
    }

    #[test]
    fn netflix_403_blocked() {
        let r = classify_netflix(403, "");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn netflix_451_blocked() {
        let r = classify_netflix(451, "");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── YouTube Premium ──
    #[test]
    fn youtube_blocked() {
        let r = classify_youtube("...Premium is not available in your country...");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn youtube_unlocked_with_region() {
        let r = classify_youtube(r#"{"countryCode":"JP"}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("JP"));
    }

    // ── Disney+ ──
    #[test]
    fn disney_blocked_by_url() {
        let r = classify_disney(200, "", "https://www.disneyplus.com/not-available");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn disney_blocked_by_body() {
        let r = classify_disney(200, "Disney+ is not available in your region", "https://www.disneyplus.com/");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn disney_blocked_403() {
        let r = classify_disney(403, "", "https://www.disneyplus.com/");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn disney_unlocked_with_region() {
        let r = classify_disney(200, r#"{"countryCode":"US"}"#, "https://www.disneyplus.com/");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    // ── AbemaTV ──
    #[test]
    fn abema_unlocked_jp() {
        let r = classify_abema(r#"{"country":"JP","continent":"AS"}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("JP"));
    }

    #[test]
    fn abema_blocked_non_jp() {
        let r = classify_abema(r#"{"country":"US","continent":"NA"}"#);
        assert_eq!(r.status, ProbeStatus::Blocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    // ── BBC iPlayer ──
    #[test]
    fn bbc_unlocked() {
        let r = classify_bbc(200, r#"{"media":[{"connection":[]}]}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("GB"));
    }

    #[test]
    fn bbc_blocked_selectionunavailable() {
        let r = classify_bbc(200, r#"{"result":["selectionunavailable"]}"#);
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn bbc_blocked_403() {
        let r = classify_bbc(403, "");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Bilibili CN ──
    #[test]
    fn bilibili_cn_unlocked() {
        let r = classify_bilibili_cn(r#"{"code":0,"data":{"country_code":"CN"}}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("CN"));
    }

    #[test]
    fn bilibili_cn_blocked_hk() {
        let r = classify_bilibili_cn(r#"{"code":0,"data":{"country_code":"HK"}}"#);
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Bilibili HK/TW ──
    #[test]
    fn bilibili_hktw_blocked_mainland() {
        let r = classify_bilibili_hktw(r#"{"code":-10403,"message":"大陆地区不可观看"}"#);
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn bilibili_hktw_unlocked() {
        let r = classify_bilibili_hktw(r#"{"code":0,"result":{"play_url":{"durl":[]}}}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
    }

    // ── DAZN ──
    #[test]
    fn dazn_unlocked() {
        let r = classify_dazn(r#"{"isAllowed":true,"Country":"DE"}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("DE"));
    }

    #[test]
    fn dazn_blocked() {
        let r = classify_dazn(r#"{"isAllowed":false,"Country":"CN"}"#);
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Hulu ──
    #[test]
    fn hulu_unlocked_us() {
        let r = classify_hulu(200, "<html>Welcome to Hulu</html>");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    #[test]
    fn hulu_blocked_body() {
        let r = classify_hulu(200, "Hulu is not available outside the US. not available");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    #[test]
    fn hulu_blocked_403() {
        let r = classify_hulu(403, "");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Paramount+ ──
    #[test]
    fn paramount_unlocked() {
        let r = classify_paramount(200, "https://www.paramountplus.com/");
        assert_eq!(r.status, ProbeStatus::Unlocked);
    }

    #[test]
    fn paramount_blocked_url() {
        let r = classify_paramount(200, "https://www.paramountplus.com/not-available/");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Peacock ──
    #[test]
    fn peacock_unlocked() {
        let r = classify_peacock(200, "<html>Peacock streaming</html>");
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    #[test]
    fn peacock_blocked() {
        let r = classify_peacock(200, "Peacock is not available in your region");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Discovery+ ──
    #[test]
    fn discovery_unlocked() {
        let r = classify_discovery(200, "https://www.discoveryplus.com/");
        assert_eq!(r.status, ProbeStatus::Unlocked);
    }

    #[test]
    fn discovery_blocked() {
        let r = classify_discovery(200, "https://www.discoveryplus.com/not-available");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Spotify ──
    #[test]
    fn spotify_unlocked_with_country() {
        let r = classify_spotify(200, r#"{"countryCode":"SE"}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("SE"));
    }

    #[test]
    fn spotify_blocked_403() {
        let r = classify_spotify(403, "");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── TVB ──
    #[test]
    fn tvb_unlocked() {
        assert_eq!(classify_tvb(200).status, ProbeStatus::Unlocked);
    }

    #[test]
    fn tvb_blocked() {
        assert_eq!(classify_tvb(403).status, ProbeStatus::Blocked);
    }

    // ── Funimation ──
    #[test]
    fn funimation_unlocked() {
        let r = classify_funimation(200, "<html>Watch anime</html>");
        assert_eq!(r.status, ProbeStatus::Unlocked);
    }

    #[test]
    fn funimation_blocked_body() {
        let r = classify_funimation(200, "not available in your region");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Crunchyroll ──
    #[test]
    fn crunchyroll_unlocked() {
        let r = classify_crunchyroll(200, "<html>Watch anime on Crunchyroll</html>");
        assert_eq!(r.status, ProbeStatus::Unlocked);
    }

    #[test]
    fn crunchyroll_blocked_body() {
        let r = classify_crunchyroll(200, "Crunchyroll is not available in your region");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Prime Video ──
    #[test]
    fn prime_unlocked() {
        let r = classify_prime(200, r#"{"country":"US"}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    #[test]
    fn prime_blocked_403() {
        let r = classify_prime(403, "");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── HBO Max ──
    #[test]
    fn hbo_unlocked_with_country() {
        let r = classify_hbo(200, r#"{"countryCode":"US"}"#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    #[test]
    fn hbo_blocked_body() {
        let r = classify_hbo(200, "not available in your country");
        assert_eq!(r.status, ProbeStatus::Blocked);
    }

    // ── Integration tests with httpmock ──

    #[tokio::test]
    async fn netflix_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/title/81280792");
            then.status(200).body(r#"{"requestCountry":"JP"}"#);
        });
        let p = Netflix { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("JP"));
        assert_eq!(r.name, "Netflix");
    }

    #[tokio::test]
    async fn netflix_check_restricted() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/title/81280792");
            then.status(404).body("not found");
        });
        let p = Netflix { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Restricted);
    }

    #[tokio::test]
    async fn youtube_check_parses_region() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/premium");
            then.status(200).body(r#"<html>{"countryCode":"US"}</html>"#);
        });
        let p = YouTube { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("US"));
    }

    #[tokio::test]
    async fn abema_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/v1/ip/check").query_param("device_type", "pc");
            then.status(200).body(r#"{"country":"JP","continent":"AS"}"#);
        });
        let p = AbemaTV { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("JP"));
    }

    #[tokio::test]
    async fn bbc_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path_contains("mediaselector");
            then.status(200).body(r#"{"media":[{"connection":[{"href":"https://example.com"}]}]}"#);
        });
        let p = BbcIplayer { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("GB"));
    }

    #[tokio::test]
    async fn bilibili_cn_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/x/web-interface/zone");
            then.status(200).body(r#"{"code":0,"data":{"country_code":"CN","country":"中国"}}"#);
        });
        let p = BilibiliCn { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("CN"));
    }

    #[tokio::test]
    async fn dazn_check_unlocked() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/misl/v5/Startup");
            then.status(200).body(r#"{"isAllowed":true,"Country":"DE"}"#);
        });
        let p = Dazn { base: server.base_url() };
        let client = crate::fetch::build_client(5);
        let r = p.check(&client).await;
        m.assert();
        assert_eq!(r.status, ProbeStatus::Unlocked);
    }

    #[test]
    fn iqiyi_status_mapping() {
        assert_eq!(classify_iqiyi(200).status, ProbeStatus::Unlocked);
        assert_eq!(classify_iqiyi(403).status, ProbeStatus::Blocked);
        assert_eq!(classify_iqiyi(500).status, ProbeStatus::Unknown);
    }

    #[test]
    fn kocowa_branches() {
        assert_eq!(parse_kocowa(200, "ok").status, ProbeStatus::Unlocked);
        assert_eq!(parse_kocowa(200, "is not available in your region or country").status, ProbeStatus::Blocked);
        assert_eq!(parse_kocowa(403, "").status, ProbeStatus::Blocked);
    }

    #[test]
    fn viu_status_mapping() {
        assert_eq!(classify_viu(200).status, ProbeStatus::Unlocked);
        assert_eq!(classify_viu(451).status, ProbeStatus::Blocked);
    }

    #[test]
    fn tiktok_region_parse() {
        let r = parse_tiktok(200, r#"...,"region":"US",..."#);
        assert_eq!(r.status, ProbeStatus::Unlocked);
        assert_eq!(r.region.as_deref(), Some("us"));
        let hk = parse_tiktok(200, "go to https://www.tiktok.com/hk/notfound page");
        assert_eq!(hk.status, ProbeStatus::Blocked);
        assert_eq!(hk.region.as_deref(), Some("hk"));
        assert_eq!(parse_tiktok(403, "").status, ProbeStatus::Blocked);
    }
}
