/// 输出语言
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// 从 CLI 字符串解析,无法识别回退中文
    pub fn parse(s: &str) -> Lang {
        match s.to_ascii_lowercase().as_str() {
            "en" | "en-us" | "english" => Lang::En,
            _ => Lang::Zh,
        }
    }

    /// 二选一取对应语言文案
    pub fn pick(self, zh: &'static str, en: &'static str) -> &'static str {
        match self {
            Lang::Zh => zh,
            Lang::En => en,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_en_variants() {
        assert_eq!(Lang::parse("en"), Lang::En);
        assert_eq!(Lang::parse("EN-US"), Lang::En);
        assert_eq!(Lang::parse("english"), Lang::En);
    }

    #[test]
    fn parse_defaults_zh() {
        assert_eq!(Lang::parse("zh"), Lang::Zh);
        assert_eq!(Lang::parse("garbage"), Lang::Zh);
    }

    #[test]
    fn pick_selects_language() {
        assert_eq!(Lang::Zh.pick("中", "en"), "中");
        assert_eq!(Lang::En.pick("中", "en"), "en");
    }
}
