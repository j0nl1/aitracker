use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;

use crate::core::auth::read_codex_credentials;
use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{ProviderIdentity, RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

// --- Config ---

#[derive(Deserialize, Default)]
struct CodexConfig {
    chatgpt_base_url: Option<String>,
}

fn codex_config_path() -> PathBuf {
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".codex")
        })
        .join("config.toml")
}

fn read_codex_config() -> CodexConfig {
    let path = codex_config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return CodexConfig::default(),
    };
    toml::from_str(&content).unwrap_or_default()
}

/// Resolve the full usage URL from the optional configured base URL.
fn resolve_usage_url(base_url: Option<&str>) -> String {
    let base = base_url.unwrap_or("https://chatgpt.com/backend-api/");

    // Normalise chatgpt.com and chat.openai.com to include /backend-api
    let base = if (base.contains("chatgpt.com") || base.contains("chat.openai.com"))
        && !base.contains("backend-api")
    {
        let trimmed = base.trim_end_matches('/');
        format!("{}/backend-api/", trimmed)
    } else {
        base.to_string()
    };

    let base = if base.ends_with('/') {
        base
    } else {
        format!("{}/", base)
    };

    if base.contains("backend-api") {
        format!("{}wham/usage", base)
    } else {
        format!("{}api/codex/usage", base)
    }
}

// --- Response types ---

#[derive(Deserialize)]
struct CodexWindowRaw {
    used_percent: f64,
    reset_at: Option<i64>,
    limit_window_seconds: Option<u64>,
}

#[derive(Deserialize)]
struct CodexRateLimitRaw {
    primary_window: Option<CodexWindowRaw>,
    secondary_window: Option<CodexWindowRaw>,
}

/// `balance` can be a JSON number or a JSON string — handle both.
fn deserialize_balance<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| serde::de::Error::custom("balance number out of f64 range")),
        serde_json::Value::String(s) => s.parse::<f64>().map_err(serde::de::Error::custom),
        other => Err(serde::de::Error::custom(format!(
            "expected number or string for balance, got {:?}",
            other
        ))),
    }
}

#[derive(Deserialize)]
struct CodexCreditsRaw {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    #[serde(deserialize_with = "deserialize_balance", default)]
    balance: f64,
}

#[derive(Deserialize)]
struct CodexUsageResponse {
    plan_type: Option<String>,
    rate_limit: Option<CodexRateLimitRaw>,
    credits: Option<CodexCreditsRaw>,
}

fn parse_window(raw: CodexWindowRaw) -> RateWindow {
    let window_minutes = raw
        .limit_window_seconds
        .map(|s| s / 60)
        .unwrap_or(0);

    let resets_at: Option<DateTime<Utc>> = raw
        .reset_at
        .and_then(|epoch| Utc.timestamp_opt(epoch, 0).single());

    RateWindow {
        used_percent: raw.used_percent,
        window_minutes,
        resets_at,
        reset_description: None,
    }
}

/// Fetch usage data from the Codex API.
pub async fn fetch() -> Result<FetchResult> {
    let creds = read_codex_credentials().context("Failed to read Codex credentials")?;

    let config = read_codex_config();
    let url = resolve_usage_url(config.chatgpt_base_url.as_deref());

    let client = reqwest::Client::new();
    let mut request = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", creds.access_token))
        .header("Accept", "application/json");

    if let Some(account_id) = &creds.account_id {
        request = request.header("ChatGPT-Account-Id", account_id);
    }

    let response = request
        .send()
        .await
        .context("Failed to send request to Codex API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — run `codex` to re-authenticate");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: CodexUsageResponse = response
        .json()
        .await
        .context("Failed to parse Codex usage response")?;

    let (primary, secondary) = if let Some(rl) = data.rate_limit {
        let primary = rl.primary_window.map(parse_window);
        let secondary = rl.secondary_window.map(parse_window);
        (primary, secondary)
    } else {
        (None, None)
    };

    let identity = data.plan_type.map(|plan| ProviderIdentity {
        email: None,
        organization: None,
        plan: Some(plan),
    });

    let credits = data.credits.map(|c| CreditsSnapshot {
        remaining: c.balance,
        has_credits: c.has_credits.unwrap_or(false),
        unlimited: c.unlimited.unwrap_or(false),
        used: None,
        limit: None,
        currency: None,
        period: None,
    });

    let usage = UsageSnapshot {
        provider: Provider::Codex,
        source: "oauth".to_string(),
        primary,
        secondary,
        tertiary: None,
        identity,
    };

    Ok(FetchResult { usage, credits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_url_default() {
        let url = resolve_usage_url(None);
        assert_eq!(url, "https://chatgpt.com/backend-api/wham/usage");
    }

    #[test]
    fn resolve_url_chatgpt_without_backend_api() {
        let url = resolve_usage_url(Some("https://chatgpt.com/"));
        assert!(url.contains("backend-api"), "url: {}", url);
        assert!(url.ends_with("wham/usage"), "url: {}", url);
    }

    #[test]
    fn resolve_url_with_backend_api_already() {
        let url = resolve_usage_url(Some("https://chatgpt.com/backend-api/"));
        assert_eq!(url, "https://chatgpt.com/backend-api/wham/usage");
    }

    #[test]
    fn resolve_url_chat_openai() {
        let url = resolve_usage_url(Some("https://chat.openai.com/"));
        assert!(url.contains("backend-api"), "url: {}", url);
        assert!(url.ends_with("wham/usage"), "url: {}", url);
    }

    #[test]
    fn resolve_url_custom_base() {
        let url = resolve_usage_url(Some("https://my.proxy.com/api/"));
        assert_eq!(url, "https://my.proxy.com/api/api/codex/usage");
    }

    #[test]
    fn parse_window_converts_seconds_to_minutes() {
        let raw = CodexWindowRaw {
            used_percent: 42.0,
            reset_at: Some(1713600000),
            limit_window_seconds: Some(18000),
        };
        let window = parse_window(raw);
        assert!((window.used_percent - 42.0).abs() < 1e-10);
        assert_eq!(window.window_minutes, 300);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn parse_window_handles_missing_fields() {
        let raw = CodexWindowRaw {
            used_percent: 10.0,
            reset_at: None,
            limit_window_seconds: None,
        };
        let window = parse_window(raw);
        assert_eq!(window.window_minutes, 0);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 42,
                    "reset_at": 1713600000,
                    "limit_window_seconds": 18000
                },
                "secondary_window": {
                    "used_percent": 15,
                    "reset_at": 1714204800,
                    "limit_window_seconds": 604800
                }
            },
            "credits": {
                "has_credits": true,
                "unlimited": false,
                "balance": 150.25
            }
        }"#;
        let data: CodexUsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.plan_type.as_deref(), Some("pro"));
        let rl = data.rate_limit.unwrap();
        let primary = rl.primary_window.unwrap();
        assert!((primary.used_percent - 42.0).abs() < 1e-10);
        let credits = data.credits.unwrap();
        assert!((credits.balance - 150.25).abs() < 1e-10);
    }

    #[test]
    fn deserialize_balance_as_string() {
        let json = r#"{
            "has_credits": true,
            "unlimited": false,
            "balance": "99.50"
        }"#;
        let credits: CodexCreditsRaw = serde_json::from_str(json).unwrap();
        assert!((credits.balance - 99.50).abs() < 1e-10);
    }

    #[test]
    fn deserialize_balance_as_number() {
        let json = r#"{
            "has_credits": true,
            "unlimited": false,
            "balance": 42.0
        }"#;
        let credits: CodexCreditsRaw = serde_json::from_str(json).unwrap();
        assert!((credits.balance - 42.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_partial_response_no_rate_limit() {
        let json = r#"{ "plan_type": "free" }"#;
        let data: CodexUsageResponse = serde_json::from_str(json).unwrap();
        assert!(data.rate_limit.is_none());
        assert!(data.credits.is_none());
    }
}
