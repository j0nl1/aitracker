use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const USAGE_URL: &str = "https://crof.ai/usage_api/";

#[derive(Deserialize)]
struct UsageResponse {
    credits: f64,
    requests_plan: Option<f64>,
    usable_requests: Option<f64>,
}

fn resolve_api_key() -> Result<String> {
    std::env::var("CROF_API_KEY")
        .or_else(|_| std::env::var("CROFAI_API_KEY"))
        .context("CROF_API_KEY env var not set")
}

fn parse_request_window(plan: f64, usable: f64) -> RateWindow {
    let clamped_remaining = usable.clamp(0.0, plan);
    let remaining_percent = if plan > 0.0 {
        (clamped_remaining / plan * 100.0).floor()
    } else {
        0.0
    };
    RateWindow {
        used_percent: (100.0 - remaining_percent).clamp(0.0, 100.0),
        window_minutes: 1440,
        resets_at: None,
        reset_description: Some(format!("{} requests left", clamped_remaining)),
    }
}

/// Fetch credit balance and request quota from the Crof AI usage API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = resolve_api_key()?;

    let client = reqwest::Client::new();
    let response = client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Crof API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: UsageResponse = response
        .json()
        .await
        .context("Failed to parse Crof usage response")?;

    let credits_amount = data.credits.max(0.0);
    let credits = Some(CreditsSnapshot {
        remaining: credits_amount,
        has_credits: credits_amount > 0.0,
        unlimited: false,
        used: None,
        limit: None,
        currency: Some("usd".to_string()),
        period: None,
    });

    let primary = match (data.requests_plan, data.usable_requests) {
        (Some(plan), Some(usable)) if plan > 0.0 => Some(parse_request_window(plan, usable)),
        _ => None,
    };

    let usage = UsageSnapshot {
        provider: Provider::Crof,
        source: "api".to_string(),
        primary,
        secondary: None,
        tertiary: None,
        identity: None,
    };

    Ok(FetchResult { usage, credits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "credits": 9.0441,
            "requests_plan": 100,
            "usable_requests": 42,
            "usage": {}
        }"#;
        let data: UsageResponse = serde_json::from_str(json).unwrap();
        assert!((data.credits - 9.0441).abs() < 1e-9);
        assert_eq!(data.requests_plan, Some(100.0));
    }

    #[test]
    fn deserialize_without_request_plan() {
        let json = r#"{ "credits": 5.0, "requests_plan": null, "usable_requests": null }"#;
        let data: UsageResponse = serde_json::from_str(json).unwrap();
        assert!(data.requests_plan.is_none());
    }

    #[test]
    fn parse_request_window_computes_used_percent() {
        let window = parse_request_window(100.0, 25.0);
        assert!((window.used_percent - 75.0).abs() < 1e-9);
        assert_eq!(window.window_minutes, 1440);
    }

    #[test]
    fn parse_request_window_clamps_over_plan() {
        let window = parse_request_window(10.0, 20.0);
        assert_eq!(window.used_percent, 0.0);
    }
}
