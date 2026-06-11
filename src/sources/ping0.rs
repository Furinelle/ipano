use std::net::IpAddr;
use async_trait::async_trait;
use reqwest::Client;
use crate::sources::Source;
use crate::model::{SourceData, SourceError, SourceResult, IpType};

/// 判定响应是否为 Cloudflare Turnstile / Aliyun 验证码页(实测特征)
pub fn is_challenge(body: &str) -> bool {
    body.contains("cf-turnstile")
        || body.contains("captcha-element")
        || body.contains("AliyunCaptchaConfig")
}

/// 从 ping0 认证后 HTML 解析风控值/原生 IP。
/// 选择器为 best-effort:基于"风控值"标签后首个 0-100 整数、"原生 IP"文本标记。
/// 待真实认证样本校正(ping0 改版/A-B 测试可能改变结构)。
pub fn parse(body: &str) -> Result<SourceData, SourceError> {
    let mut d = SourceData::new("ping0");
    if let Some(v) = risk_after_label(body, "风控值") {
        d.risk_score = Some(v);
    }
    if body.contains("原生 IP") {
        d.ip_type = Some(IpType::Native);
    }
    if d.risk_score.is_none() && d.ip_type.is_none() {
        return Err(SourceError::Parse("ping0 页面结构无法识别(可能改版)".to_string()));
    }
    Ok(d)
}

/// 在 label 之后提取首个 0-100 的整数(跳过非数字字符)
fn risk_after_label(body: &str, label: &str) -> Option<i64> {
    let idx = body.find(label)? + label.len();
    let tail = &body[idx..];
    let digits: String = tail.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<i64>().ok().filter(|v| (0..=100).contains(v))
}

pub struct Ping0 {
    pub base: String,
    pub token: Option<String>,
    pub tokentype: String,
}

impl Default for Ping0 {
    fn default() -> Self {
        Ping0 {
            base: "https://ping0.cc".to_string(),
            token: std::env::var("IPANO_PING0_TOKEN").ok().filter(|s| !s.is_empty()),
            tokentype: std::env::var("IPANO_PING0_TOKENTYPE").unwrap_or_else(|_| "cf".to_string()),
        }
    }
}

#[async_trait]
impl Source for Ping0 {
    fn id(&self) -> &'static str { "ping0" }
    fn needs_key(&self) -> Option<&'static str> { Some("IPANO_PING0_TOKEN") }
    async fn fetch(&self, client: &Client, ip: IpAddr) -> SourceResult {
        let token = self.token.as_ref().ok_or_else(|| SourceError::NeedsKey(
            "IPANO_PING0_TOKEN(浏览器解 Turnstile 后从 cookie 复制,60 秒内有效)".to_string()))?;
        let url = format!("{}/ip/{}", self.base, ip);
        let cookie = format!("token={}; tokentype={}", token, self.tokentype);
        let resp = client.get(&url).header(reqwest::header::COOKIE, cookie).send().await
            .map_err(|e| if e.is_timeout() { SourceError::Timeout }
                         else { SourceError::Unavailable(e.to_string()) })?;
        let body = resp.text().await.map_err(|e| SourceError::Unavailable(e.to_string()))?;
        if is_challenge(&body) { return Err(SourceError::ChallengeFailed); }
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ping0 验证码页特征(实测 2026-06-12:含 cf-turnstile 与 captcha-element)
    const CHALLENGE: &str = r#"<html><head>
        <script>window.AliyunCaptchaConfig={region:"cn"};</script></head>
        <body><div id="captcha-element" class="cf-turnstile"
        data-sitekey="0x4AAAAAAB01fdNepRQppzkd"></div></body></html>"#;

    // 认证后 ping0 页面片段。注:选择器为 best-effort,基于"风控值 + 数字"
    // 与"原生 IP"文本标记;待真实认证样本校正(见 parse 注释)。
    const PING0_HTML: &str = r#"<html><body>
        <div class="line"><span class="name">IP 风控值</span>
        <span class="value">41</span></div>
        <div class="line"><span class="name">IP 类型</span>
        <span class="value">原生 IP</span></div>
        </body></html>"#;

    #[test]
    fn detects_turnstile_challenge() {
        assert!(is_challenge(CHALLENGE));
        assert!(!is_challenge("<html><body>风控值 41</body></html>"));
    }

    #[tokio::test]
    async fn no_token_yields_needs_key() {
        let src = Ping0 { base: "https://ping0.cc".into(), token: None, tokentype: "cf".into() };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        assert!(matches!(err, SourceError::NeedsKey(_)));
    }

    #[tokio::test]
    async fn challenge_page_yields_challenge_failed() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/ip/1.1.1.1");
            then.status(200).body(CHALLENGE);
        });
        let src = Ping0 { base: server.base_url(), token: Some("abc".into()), tokentype: "cf".into() };
        let client = crate::fetch::build_client(5);
        let err = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap_err();
        m.assert();
        assert!(matches!(err, SourceError::ChallengeFailed));
    }

    #[tokio::test]
    async fn sends_token_cookie_and_parses() {
        let server = httpmock::MockServer::start();
        let m = server.mock(|when, then| {
            when.path("/ip/1.1.1.1").header("cookie", "token=abc; tokentype=cf");
            then.status(200).body(PING0_HTML);
        });
        let src = Ping0 { base: server.base_url(), token: Some("abc".into()), tokentype: "cf".into() };
        let client = crate::fetch::build_client(5);
        let d = src.fetch(&client, "1.1.1.1".parse().unwrap()).await.unwrap();
        m.assert();
        assert_eq!(d.source_id, "ping0");
        assert_eq!(d.risk_score, Some(41));
    }

    #[test]
    fn parse_extracts_risk_and_native() {
        let d = parse(PING0_HTML).unwrap();
        assert_eq!(d.risk_score, Some(41));
        assert_eq!(d.ip_type, Some(IpType::Native));
    }

    #[test]
    fn parse_unrecognized_page_errors() {
        let err = parse("<html><body>欢迎</body></html>").unwrap_err();
        assert!(matches!(err, SourceError::Parse(_)));
    }

    #[test]
    fn risk_label_rejects_out_of_range() {
        assert_eq!(risk_after_label("风控值 250 分", "风控值"), None);
        assert_eq!(risk_after_label("风控值 88 分", "风控值"), Some(88));
    }
}
