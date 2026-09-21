use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize, Default)]
struct QuotaWindow {
    #[serde(alias = "requests")]
    used: Option<f64>,
    limit: Option<f64>,
    #[serde(alias = "reset_at")]
    resets_at: Option<String>,
    window_minutes: Option<u64>,
}

#[derive(Deserialize, Default)]
struct SubscriptionUsageResponse {
    monthly: Option<QuotaWindow>,
    rolling_window: Option<QuotaWindow>,
}

fn parse_window(w: &QuotaWindow, default_minutes: u64) -> Option<RateWindow> {
    let limit = w.limit?;
    if limit <= 0.0 {
        return None;
    }
    let used = w.used.unwrap_or(0.0);
    let resets_at: Option<DateTime<Utc>> = w
        .resets_at
        .as_deref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());
    Some(RateWindow {
        used_percent: (used / limit * 100.0).clamp(0.0, 100.0),
        window_minutes: w.window_minutes.unwrap_or(default_minutes),
        resets_at,
        reset_description: None,
    })
}

/// Fetch subscription usage (rolling 4h + monthly windows) from the Chutes API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("CHUTES_API_KEY").context("CHUTES_API_KEY env var not set")?;
    let base = std::env::var("CHUTES_API_URL").unwrap_or_else(|_| "https://api.chutes.ai".to_string());
    validate_endpoint(&base, "Chutes")?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/users/me/subscription_usage", base.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Chutes API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Invalid credentials — check CHUTES_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: SubscriptionUsageResponse = response
        .json()
        .await
        .context("Failed to parse Chutes subscription usage response")?;

    let primary = data
        .rolling_window
        .as_ref()
        .and_then(|w| parse_window(w, 240));
    let secondary = data.monthly.as_ref().and_then(|w| parse_window(w, 43200));

    let usage = UsageSnapshot {
        provider: Provider::Chutes,
        source: "api".to_string(),
        primary,
        secondary,
        tertiary: None,
        identity: None,
    };

    Ok(FetchResult {
        usage,
        credits: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "subscription": { "active": true, "plan_name": "Pro", "current_period_end": "2026-07-01T00:00:00Z" },
            "monthly": { "used": 250, "limit": 1000, "resets_at": "2026-07-01T00:00:00Z", "unit": "credits" },
            "rolling_window": { "requests": 40, "limit": 100, "window_minutes": 240, "reset_at": "2026-06-13T18:00:00Z", "unit": "requests" }
        }"#;
        let data: SubscriptionUsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.monthly.unwrap().limit, Some(1000.0));
        assert_eq!(data.rolling_window.unwrap().window_minutes, Some(240));
    }

    #[test]
    fn parse_window_computes_percent() {
        let w = QuotaWindow {
            used: Some(25.0),
            limit: Some(100.0),
            resets_at: None,
            window_minutes: None,
        };
        let window = parse_window(&w, 240).unwrap();
        assert!((window.used_percent - 25.0).abs() < 1e-9);
        assert_eq!(window.window_minutes, 240);
    }

    #[test]
    fn parse_window_none_without_limit() {
        let w = QuotaWindow::default();
        assert!(parse_window(&w, 240).is_none());
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: SubscriptionUsageResponse = serde_json::from_str(json).unwrap();
        assert!(data.monthly.is_none());
        assert!(data.rolling_window.is_none());
    }
}
