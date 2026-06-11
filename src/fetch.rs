use std::time::Duration;
use reqwest::Client;

pub const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) ipano/0.1";

pub fn build_client(timeout_secs: u64) -> Client {
    Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .expect("构造 reqwest Client 失败")
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_client_with_timeout() {
        // 仅验证构造不 panic
        let _c = super::build_client(5);
    }
}
