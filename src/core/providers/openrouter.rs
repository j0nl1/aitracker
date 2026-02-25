use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";
const KEY_URL: &str = "https://openrouter.ai/api/v1/key";

#[derive(Deserialize)]
struct CreditsData {
    total_credits: Option<f64>,
    total_usage: Option<f64>,
}

#[derive(Deserialize)]
struct CreditsResponse {
    data: CreditsData,
}

#[derive(Deserialize)]
struct KeyData {
    limit: Option<f64>,
    usage: Option<f64>,
}

#[derive(Deserialize)]
struct KeyResponse {
    data: KeyData,
}

fn parse_key_window(key_data: &KeyData) -> Option<RateWindow> {
    let limit = key_data.limit?;
    let usage = key_data.usage.unwrap_or(0.0);

    if limit <= 0.0 {
        return None;
    }

    let used_percent = (usage / limit * 100.0).min(100.0);

    Some(RateWindow {
        used_percent,
        window_minutes: 0,
        resets_at: None,
        reset_description: None,
    })
}

/// Fetch usage data from the OpenRouter API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key =
        std::env::var("OPENROUTER_API_KEY").context("OPENROUTER_API_KEY env var not set")?;

    if api_key.is_empty() {
        anyhow::bail!("OPENROUTER_API_KEY is empty");
    }

    let client = reqwest::Client::new();

    // Fetch credits
    let credits_response = client
        .get(CREDITS_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to OpenRouter credits API")?;

    let status = credits_response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized - check your OPENROUTER_API_KEY");
    }
    if !status.is_success() {
        let body = credits_response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {} from credits endpoint: {}", status.as_u16(), body);
    }

    let credits_data: CreditsResponse = credits_response
        .json()
        .await
        .context("Failed to parse OpenRouter credits response")?;

    let total_credits = credits_data.data.total_credits.unwrap_or(0.0);
    let total_usage = credits_data.data.total_usage.unwrap_or(0.0);
    let balance = (total_credits - total_usage).max(0.0);

    let credits = Some(CreditsSnapshot {
        remaining: balance,
        has_credits: balance > 0.0,
        unlimited: false,
        used: Some(total_usage),
        limit: Some(total_credits),
        currency: Some("usd".to_string()),
        period: None,
    });

    // Optionally fetch key info for rate window
    let primary = match client
        .get(KEY_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<KeyResponse>().await {
                Ok(key_data) => parse_key_window(&key_data.data),
                Err(_) => None,
            }
        }
        _ => None,
    };

    let usage = UsageSnapshot {
        provider: Provider::OpenRouter,
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
    fn deserialize_credits_response() {
        let json = r#"{
            "data": {
                "total_credits": 100.0,
                "total_usage": 37.50
            }
        }"#;
        let resp: CreditsResponse = serde_json::from_str(json).unwrap();
        assert!((resp.data.total_credits.unwrap() - 100.0).abs() < 1e-10);
        assert!((resp.data.total_usage.unwrap() - 37.50).abs() < 1e-10);
    }

    #[test]
    fn deserialize_credits_response_partial() {
        let json = r#"{ "data": {} }"#;
        let resp: CreditsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.data.total_credits.is_none());
        assert!(resp.data.total_usage.is_none());
    }

    #[test]
    fn deserialize_key_response() {
        let json = r#"{
            "data": {
                "limit": 50.0,
                "usage": 12.75
            }
        }"#;
        let resp: KeyResponse = serde_json::from_str(json).unwrap();
        assert!((resp.data.limit.unwrap() - 50.0).abs() < 1e-10);
        assert!((resp.data.usage.unwrap() - 12.75).abs() < 1e-10);
    }

    #[test]
    fn deserialize_key_response_no_limit() {
        let json = r#"{ "data": {} }"#;
        let resp: KeyResponse = serde_json::from_str(json).unwrap();
        assert!(resp.data.limit.is_none());
        assert!(resp.data.usage.is_none());
    }

    #[test]
    fn parse_key_window_calculates_percent() {
        let data = KeyData {
            limit: Some(50.0),
            usage: Some(12.5),
        };
        let window = parse_key_window(&data).unwrap();
        assert!((window.used_percent - 25.0).abs() < 1e-10);
        assert_eq!(window.window_minutes, 0);
    }

    #[test]
    fn parse_key_window_no_limit_returns_none() {
        let data = KeyData {
            limit: None,
            usage: Some(10.0),
        };
        assert!(parse_key_window(&data).is_none());
    }

    #[test]
    fn parse_key_window_zero_limit_returns_none() {
        let data = KeyData {
            limit: Some(0.0),
            usage: Some(0.0),
        };
        assert!(parse_key_window(&data).is_none());
    }

    #[test]
    fn parse_key_window_caps_at_100_percent() {
        let data = KeyData {
            limit: Some(10.0),
            usage: Some(20.0),
        };
        let window = parse_key_window(&data).unwrap();
        assert!((window.used_percent - 100.0).abs() < 1e-10);
    }

    #[test]
    fn parse_key_window_no_usage_defaults_to_zero() {
        let data = KeyData {
            limit: Some(100.0),
            usage: None,
        };
        let window = parse_key_window(&data).unwrap();
        assert!((window.used_percent - 0.0).abs() < 1e-10);
    }

    #[test]
    fn balance_calculation() {
        let total_credits: f64 = 100.0;
        let total_usage: f64 = 37.50;
        let balance = (total_credits - total_usage).max(0.0);
        assert!((balance - 62.50).abs() < 1e-10);
    }

    #[test]
    fn balance_calculation_overdraft() {
        let total_credits: f64 = 10.0;
        let total_usage: f64 = 15.0;
        let balance = (total_credits - total_usage).max(0.0);
        assert!((balance - 0.0).abs() < 1e-10);
    }
}
