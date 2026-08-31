//! 配置模型（R3）：多档案 + TMDB + 计费 + 显示。

use std::path::Path;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Profile {
    pub name: String,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_ctx")]
    pub context_length: u64,
    #[serde(default)]
    pub thinking_mode: String, // off | on | advanced
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub extra_body_json: Option<String>,
}
fn default_ctx() -> u64 {
    8192
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TmdbCfg {
    pub key: String,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default = "default_lang")]
    pub language: String,
}
fn default_lang() -> String {
    "zh-CN".into()
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DisplayCfg {
    #[serde(default = "default_title_main")]
    pub title_main: String, // zh | en | original | hide
    #[serde(default = "default_title_sub")]
    pub title_sub: String,
}
fn default_title_main() -> String {
    "zh".into()
}
fn default_title_sub() -> String {
    "original".into()
}
impl Default for DisplayCfg {
    fn default() -> Self {
        Self {
            title_main: default_title_main(),
            title_sub: default_title_sub(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Billing {
    #[serde(default = "default_currency")]
    pub display_currency: String,
    #[serde(default)]
    pub fx_rates: std::collections::HashMap<String, f64>, // 币种 → 对 display_currency 汇率
    pub budget_monthly: Option<f64>,
    pub budget_total: Option<f64>,
}
fn default_currency() -> String {
    "CNY".into()
}
impl Default for Billing {
    fn default() -> Self {
        Billing {
            display_currency: default_currency(),
            fx_rates: Default::default(),
            budget_monthly: None,
            budget_total: None,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub active_profile: Option<String>,
    pub tmdb: TmdbCfg,
    #[serde(default)]
    pub billing: Billing,
    #[serde(default)]
    pub display: DisplayCfg,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        let raw =
            std::fs::read_to_string(path).map_err(|e| format!("读取 {}: {e}", path.display()))?;
        toml::from_str(&raw).map_err(|e| format!("config 解析失败: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let s = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }

    pub fn active(&self) -> Result<&Profile, String> {
        let name = self
            .active_profile
            .as_deref()
            .ok_or("未设置 active_profile")?;
        self.profiles
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| format!("active_profile `{name}` 不存在"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[tmdb]
key = "abc123"

[[profiles]]
name = "课程平台"
endpoint = "https://x/v1"
api_key = "sk-x"
model = "glm-5.3-flash"

[billing]
display_currency = "CNY"
budget_monthly = 10.0
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.tmdb.language, "zh-CN");
        assert_eq!(cfg.billing.budget_monthly, Some(10.0));
        assert_eq!(cfg.display.title_main, "zh");
    }
}

#[cfg(test)]
impl Config {
    pub fn default_for_test() -> Self {
        serde_json::from_str("{\"tmdb\": {\"key\": \"\"}}").unwrap()
    }
}
