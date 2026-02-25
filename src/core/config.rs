use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_format")]
    pub default_format: String,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_format() -> String {
    "text".to_string()
}
fn default_color() -> String {
    "auto".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_format: default_format(),
            color: default_color(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_source")]
    pub source: String,
    pub api_key: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_source() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            providers: vec![
                ProviderConfig {
                    id: "claude".into(),
                    enabled: true,
                    source: "auto".into(),
                    api_key: None,
                },
                ProviderConfig {
                    id: "codex".into(),
                    enabled: true,
                    source: "auto".into(),
                    api_key: None,
                },
                ProviderConfig {
                    id: "copilot".into(),
                    enabled: false,
                    source: "auto".into(),
                    api_key: None,
                },
                ProviderConfig {
                    id: "openrouter".into(),
                    enabled: false,
                    source: "auto".into(),
                    api_key: None,
                },
            ],
        }
    }
}

impl AppConfig {
    /// Get the config file path, respecting XDG_CONFIG_HOME
    pub fn config_path() -> PathBuf {
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("~"))
                    .join(".config")
            });
        config_dir.join("ait").join("config.toml")
    }

    /// Load config from the default path, falling back to defaults if not found
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Serialize and write this config to the config file path.
    pub fn save(&self) -> Result<PathBuf, std::io::Error> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).expect("Failed to serialize config");
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Generate a config file with specific providers enabled.
    /// Includes all non-stub providers; only those in `enabled_ids` are set to `enabled = true`.
    pub fn generate_with_providers(enabled_ids: &[String]) -> Result<PathBuf, std::io::Error> {
        let providers: Vec<ProviderConfig> = crate::core::providers::Provider::all()
            .iter()
            .filter(|p| !p.is_stub())
            .map(|p| ProviderConfig {
                id: p.id().to_string(),
                enabled: enabled_ids.iter().any(|id| id == p.id()),
                source: "auto".to_string(),
                api_key: None,
            })
            .collect();
        let config = Self {
            settings: Settings::default(),
            providers,
        };
        config.save()
    }

    /// Update which providers are enabled, preserving existing settings, source, and api_key.
    /// Adds any new non-stub providers missing from the current config.
    pub fn update_providers(&mut self, enabled_ids: &[String]) -> Result<PathBuf, std::io::Error> {
        // Update enabled flag for existing providers
        for provider in &mut self.providers {
            provider.enabled = enabled_ids.iter().any(|id| id == &provider.id);
        }

        // Add any new non-stub providers not yet in config
        let existing_ids: Vec<String> = self.providers.iter().map(|p| p.id.clone()).collect();
        for p in crate::core::providers::Provider::all() {
            if !p.is_stub() && !existing_ids.contains(&p.id().to_string()) {
                self.providers.push(ProviderConfig {
                    id: p.id().to_string(),
                    enabled: enabled_ids.iter().any(|id| id == p.id()),
                    source: "auto".to_string(),
                    api_key: None,
                });
            }
        }

        self.save()
    }

    /// Validate the config
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if !["text", "json"].contains(&self.settings.default_format.as_str()) {
            issues.push(format!(
                "Invalid default_format: '{}' (must be 'text' or 'json')",
                self.settings.default_format
            ));
        }
        if !["auto", "always", "never"].contains(&self.settings.color.as_str()) {
            issues.push(format!(
                "Invalid color: '{}' (must be 'auto', 'always', or 'never')",
                self.settings.color
            ));
        }
        for p in &self.providers {
            if !["auto", "oauth", "cli", "api"].contains(&p.source.as_str()) {
                issues.push(format!(
                    "Provider '{}': invalid source '{}' (must be auto|oauth|cli|api)",
                    p.id, p.source
                ));
            }
            if crate::core::providers::Provider::from_id(&p.id).is_none() {
                issues.push(format!("Unknown provider ID: '{}'", p.id));
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_valid() {
        let config = AppConfig::default();
        let issues = config.validate();
        assert!(issues.is_empty(), "Default config should be valid, got: {:?}", issues);
    }

    #[test]
    fn default_format_is_text() {
        let settings = Settings::default();
        assert_eq!(settings.default_format, "text");
    }

    #[test]
    fn default_color_is_auto() {
        let settings = Settings::default();
        assert_eq!(settings.color, "auto");
    }

    #[test]
    fn default_providers_include_claude_and_codex_enabled() {
        let config = AppConfig::default();
        let claude = config.providers.iter().find(|p| p.id == "claude").unwrap();
        assert!(claude.enabled);
        let codex = config.providers.iter().find(|p| p.id == "codex").unwrap();
        assert!(codex.enabled);
    }

    #[test]
    fn default_providers_have_copilot_and_openrouter_disabled() {
        let config = AppConfig::default();
        let copilot = config.providers.iter().find(|p| p.id == "copilot").unwrap();
        assert!(!copilot.enabled);
        let openrouter = config.providers.iter().find(|p| p.id == "openrouter").unwrap();
        assert!(!openrouter.enabled);
    }

    #[test]
    fn validate_catches_invalid_format() {
        let mut config = AppConfig::default();
        config.settings.default_format = "xml".to_string();
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.contains("default_format")));
    }

    #[test]
    fn validate_catches_invalid_color() {
        let mut config = AppConfig::default();
        config.settings.color = "blue".to_string();
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.contains("color")));
    }

    #[test]
    fn validate_catches_invalid_source() {
        let mut config = AppConfig::default();
        config.providers[0].source = "magic".to_string();
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.contains("source")));
    }

    #[test]
    fn validate_catches_unknown_provider_id() {
        let mut config = AppConfig::default();
        config.providers.push(ProviderConfig {
            id: "notareal".to_string(),
            enabled: true,
            source: "auto".to_string(),
            api_key: None,
        });
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.contains("Unknown provider")));
    }

    #[test]
    fn parse_minimal_toml() {
        let toml = r#"
[settings]
default_format = "json"
color = "always"
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.settings.default_format, "json");
        assert_eq!(config.settings.color, "always");
        assert!(config.providers.is_empty());
    }

    #[test]
    fn parse_provider_toml() {
        let toml = r#"
[[providers]]
id = "claude"
enabled = true
source = "oauth"
"#;
        let config: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].id, "claude");
        assert_eq!(config.providers[0].source, "oauth");
    }

    #[test]
    fn parse_empty_toml_gives_defaults() {
        let config: AppConfig = toml::from_str("").unwrap();
        assert_eq!(config.settings.default_format, "text");
        assert_eq!(config.settings.color, "auto");
    }

    #[test]
    fn config_path_uses_xdg_when_set() {
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/test_xdg_config");
        let path = AppConfig::config_path();
        std::env::remove_var("XDG_CONFIG_HOME");
        assert_eq!(path, PathBuf::from("/tmp/test_xdg_config/ait/config.toml"));
    }
}
