use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::auth::read_claude_credentials;
use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{ProviderIdentity, RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Deserialize)]
struct ClaudeWindowRaw {
    utilization: f64,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeExtraUsageRaw {
    is_enabled: Option<bool>,
    monthly_limit: Option<f64>,
    used_credits: Option<f64>,
    currency: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeUsageResponse {
    five_hour: Option<ClaudeWindowRaw>,
    seven_day: Option<ClaudeWindowRaw>,
    seven_day_sonnet: Option<ClaudeWindowRaw>,
    plan: Option<String>,
    email: Option<String>,
    extra_usage: Option<ClaudeExtraUsageRaw>,
}

fn parse_window(raw: ClaudeWindowRaw, window_minutes: u64) -> RateWindow {
    let resets_at = raw
        .resets_at
        .as_deref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());
    // API may return utilization as a fraction (0.0-1.0) or a percentage (0-100).
    // If > 1.0, treat it as already a percentage.
    let used_percent = if raw.utilization > 1.0 {
        raw.utilization
    } else {
        raw.utilization * 100.0
    };
    RateWindow {
        used_percent,
        window_minutes,
        resets_at,
        reset_description: None,
    }
}

fn parse_extra_usage(raw: &ClaudeExtraUsageRaw) -> Option<CreditsSnapshot> {
    if raw.is_enabled != Some(true) {
        return None;
    }
    // Values from the API are in cents — convert to dollars
    let used = raw.used_credits.map(|c| c / 100.0);
    let limit = raw.monthly_limit.map(|c| c / 100.0);
    let remaining = match (limit, used) {
        (Some(l), Some(u)) => (l - u).max(0.0),
        _ => 0.0,
    };

    Some(CreditsSnapshot {
        remaining,
        has_credits: true,
        unlimited: false,
        used,
        limit,
        currency: raw.currency.clone(),
        period: Some("Monthly".to_string()),
    })
}

/// Fetch usage data from the Claude OAuth API.
pub async fn fetch() -> Result<FetchResult> {
    let creds = read_claude_credentials().context("Failed to read Claude credentials")?;

    let client = reqwest::Client::new();
    let response = client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {}", creds.access_token))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await
        .context("Failed to send request to Claude API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — run `claude` to re-authenticate");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: ClaudeUsageResponse = response
        .json()
        .await
        .context("Failed to parse Claude usage response")?;

    let primary = data.five_hour.map(|w| parse_window(w, 300));
    let secondary = data.seven_day.map(|w| parse_window(w, 10080));
    let tertiary = data.seven_day_sonnet.map(|w| parse_window(w, 10080));

    let identity = if data.plan.is_some() || data.email.is_some() {
        Some(ProviderIdentity {
            email: data.email,
            organization: None,
            plan: data.plan,
        })
    } else {
        None
    };

    let credits = data.extra_usage.as_ref().and_then(parse_extra_usage);

    let usage = UsageSnapshot {
        provider: Provider::Claude,
        source: "oauth".to_string(),
        primary,
        secondary,
        tertiary,
        identity,
    };

    Ok(FetchResult { usage, credits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_window_converts_utilization_to_percent() {
        let raw = ClaudeWindowRaw {
            utilization: 0.28,
            resets_at: Some("2025-12-04T19:15:00Z".to_string()),
        };
        let window = parse_window(raw, 300);
        assert!((window.used_percent - 28.0).abs() < 1e-10);
        assert_eq!(window.window_minutes, 300);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn parse_window_handles_missing_resets_at() {
        let raw = ClaudeWindowRaw {
            utilization: 0.5,
            resets_at: None,
        };
        let window = parse_window(raw, 10080);
        assert!((window.used_percent - 50.0).abs() < f64::EPSILON);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn parse_window_handles_invalid_datetime() {
        let raw = ClaudeWindowRaw {
            utilization: 0.1,
            resets_at: Some("not-a-date".to_string()),
        };
        let window = parse_window(raw, 300);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "five_hour": { "utilization": 0.28, "resets_at": "2025-12-04T19:15:00Z" },
            "seven_day": { "utilization": 0.59, "resets_at": "2025-12-05T17:00:00Z" },
            "seven_day_sonnet": { "utilization": 0.12, "resets_at": "2025-12-05T17:00:00Z" },
            "plan": "pro",
            "email": "user@example.com"
        }"#;
        let data: ClaudeUsageResponse = serde_json::from_str(json).unwrap();
        assert!(data.five_hour.is_some());
        assert!(data.seven_day.is_some());
        assert!(data.seven_day_sonnet.is_some());
        assert_eq!(data.plan.as_deref(), Some("pro"));
        assert_eq!(data.email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn deserialize_partial_response() {
        let json = r#"{ "plan": "free" }"#;
        let data: ClaudeUsageResponse = serde_json::from_str(json).unwrap();
        assert!(data.five_hour.is_none());
        assert!(data.seven_day.is_none());
        assert_eq!(data.plan.as_deref(), Some("free"));
    }

    #[test]
    fn parse_extra_usage_enabled() {
        let raw = ClaudeExtraUsageRaw {
            is_enabled: Some(true),
            monthly_limit: Some(5000.0), // 50.00 dollars in cents
            used_credits: Some(1234.0),  // 12.34 dollars in cents
            currency: Some("usd".to_string()),
        };
        let credits = parse_extra_usage(&raw).unwrap();
        assert!((credits.used.unwrap() - 12.34).abs() < 1e-10);
        assert!((credits.limit.unwrap() - 50.0).abs() < 1e-10);
        assert!((credits.remaining - 37.66).abs() < 1e-10);
        assert_eq!(credits.currency.as_deref(), Some("usd"));
        assert_eq!(credits.period.as_deref(), Some("Monthly"));
    }

    #[test]
    fn parse_extra_usage_disabled() {
        let raw = ClaudeExtraUsageRaw {
            is_enabled: Some(false),
            monthly_limit: None,
            used_credits: None,
            currency: None,
        };
        assert!(parse_extra_usage(&raw).is_none());
    }

    #[test]
    fn parse_extra_usage_not_present() {
        let raw = ClaudeExtraUsageRaw {
            is_enabled: None,
            monthly_limit: None,
            used_credits: None,
            currency: None,
        };
        assert!(parse_extra_usage(&raw).is_none());
    }

    #[test]
    fn deserialize_response_with_extra_usage() {
        let json = r#"{
            "five_hour": { "utilization": 0.28, "resets_at": "2025-12-04T19:15:00Z" },
            "plan": "pro",
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 5000,
                "used_credits": 1234,
                "currency": "usd"
            }
        }"#;
        let data: ClaudeUsageResponse = serde_json::from_str(json).unwrap();
        let extra = data.extra_usage.unwrap();
        assert_eq!(extra.is_enabled, Some(true));
        assert!((extra.monthly_limit.unwrap() - 5000.0).abs() < 1e-10);
    }
}
