//! 解锁探针共用纯函数:region 码转换、cookie 提取、正则/子串提取。

/// ISO 3166-1 alpha-3 → alpha-2(覆盖常见解锁地区;未知返回大写原值)。
pub fn three_to_two(code: &str) -> String {
    let c = code.to_uppercase();
    let m = match c.as_str() {
        "USA" => "US", "JPN" => "JP", "GBR" => "GB", "DEU" => "DE", "FRA" => "FR",
        "HKG" => "HK", "TWN" => "TW", "KOR" => "KR", "SGP" => "SG", "CHN" => "CN",
        "CAN" => "CA", "AUS" => "AU", "NLD" => "NL", "IND" => "IN", "BRA" => "BR",
        "RUS" => "RU", "ITA" => "IT", "ESP" => "ES", "THA" => "TH", "MYS" => "MY",
        "IDN" => "ID", "PHL" => "PH", "VNM" => "VN", "TUR" => "TR", "MEX" => "MX",
        _ => return c,
    };
    m.to_string()
}

/// 从 Set-Cookie 串里取某 cookie 的值(到分号或末尾)。无则 None。
pub fn extract_cookie(set_cookie: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let start = set_cookie.find(&needle)? + needle.len();
    let rest = &set_cookie[start..];
    let end = rest.find(';').unwrap_or(rest.len());
    let val = rest[..end].trim();
    if val.is_empty() { None } else { Some(val.to_string()) }
}

/// 从 body 中取 `prefix` 与 `suffix` 之间首个子串(简单无正则提取)。
pub fn between<'a>(body: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let s = body.find(prefix)? + prefix.len();
    let rest = &body[s..];
    let e = rest.find(suffix)?;
    Some(&rest[..e])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_to_two_maps_common() {
        assert_eq!(three_to_two("USA"), "US");
        assert_eq!(three_to_two("jpn"), "JP");
        assert_eq!(three_to_two("ZZZ"), "ZZZ"); // 未知透传
    }

    #[test]
    fn extract_cookie_picks_value() {
        let c = "foo=bar; steamCountry=US%7Cabc; path=/";
        assert_eq!(extract_cookie(c, "steamCountry").as_deref(), Some("US%7Cabc"));
        assert_eq!(extract_cookie(c, "missing"), None);
    }

    #[test]
    fn between_extracts() {
        assert_eq!(between(r#","region":"jp","#, r#""region":""#, r#"""#), Some("jp"));
        assert_eq!(between("abc", "x", "y"), None);
    }
}
