use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const CREDITS_URL: &str = "https://kimi-k2.ai/api/user/credits";

#[derive(Deserialize)]
struct KimiK2CreditsResponse {
    // Primary key paths
    consumed: Option<f64>,
    remaining: Option<f64>,
    // Alternate key paths
    used: Option<f64>,
    total: Option<f64>,
    // Another alternate key path
    credits_used: Option<f64>,
    credits_remaining: Option<f64>,
}

fn parse_credits(data: &KimiK2CreditsResponse) -> CreditsSnapshot {
    let (used, remaining) = if let (Some(consumed), Some(rem)) = (data.consumed, data.remaining) {
        (Some(consumed), rem)
    } else if let (Some(used), Some(total)) = (data.used, data.total) {
        (Some(used), (total - used).max(0.0))
    } else if let (Some(cu), Some(cr)) = (data.credits_used, data.credits_remaining) {
        (Some(cu), cr)
    } else {
        // Fall back to whatever fields are available
        let remaining = data
            .remaining
            .or(data.credits_remaining)
            .unwrap_or(0.0);
        let used = data.consumed.or(data.used).or(data.credits_used);
        (used, remaining)
    };

    CreditsSnapshot {
        remaining,
        has_credits: remaining > 0.0,
        unlimited: false,
        used,
        limit: None,
        currency: None,
        period: None,
    }
}

/// Fetch credit balance from the Kimi K2 API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("KIMI_K2_API_KEY").context("KIMI_K2_API_KEY env var not set")?;

    let client = reqwest::Client::new();
    let response = client
        .get(CREDITS_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Kimi K2 API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check KIMI_K2_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: KimiK2CreditsResponse = response
        .json()
        .await
        .context("Failed to parse Kimi K2 credits response")?;

    let credits = parse_credits(&data);

    let usage = UsageSnapshot {
        provider: Provider::KimiK2,
        source: "api".to_string(),
        primary: None,
        secondary: None,
        tertiary: None,
        identity: None,
    };

    Ok(FetchResult {
        usage,
        credits: Some(credits),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_consumed_remaining() {
        let json = r#"{"consumed": 150.0, "remaining": 850.0}"#;
        let data: KimiK2CreditsResponse = serde_json::from_str(json).unwrap();
        let credits = parse_credits(&data);
        assert!((credits.remaining - 850.0).abs() < 1e-10);
        assert!((credits.used.unwrap() - 150.0).abs() < 1e-10);
        assert!(credits.has_credits);
    }

    #[test]
    fn deserialize_used_total() {
        let json = r#"{"used": 200.0, "total": 1000.0}"#;
        let data: KimiK2CreditsResponse = serde_json::from_str(json).unwrap();
        let credits = parse_credits(&data);
        assert!((credits.remaining - 800.0).abs() < 1e-10);
        assert!((credits.used.unwrap() - 200.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_credits_used_credits_remaining() {
        let json = r#"{"credits_used": 50.0, "credits_remaining": 950.0}"#;
        let data: KimiK2CreditsResponse = serde_json::from_str(json).unwrap();
        let credits = parse_credits(&data);
        assert!((credits.remaining - 950.0).abs() < 1e-10);
        assert!((credits.used.unwrap() - 50.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: KimiK2CreditsResponse = serde_json::from_str(json).unwrap();
        let credits = parse_credits(&data);
        assert!((credits.remaining - 0.0).abs() < 1e-10);
        assert!(!credits.has_credits);
        assert!(credits.used.is_none());
    }

    #[test]
    fn deserialize_zero_remaining() {
        let json = r#"{"consumed": 1000.0, "remaining": 0.0}"#;
        let data: KimiK2CreditsResponse = serde_json::from_str(json).unwrap();
        let credits = parse_credits(&data);
        assert!((credits.remaining - 0.0).abs() < 1e-10);
        assert!(!credits.has_credits);
    }

    #[test]
    fn consumed_remaining_preferred_over_used_total() {
        let json = r#"{
            "consumed": 100.0,
            "remaining": 900.0,
            "used": 999.0,
            "total": 1000.0
        }"#;
        let data: KimiK2CreditsResponse = serde_json::from_str(json).unwrap();
        let credits = parse_credits(&data);
        // consumed/remaining path should win
        assert!((credits.remaining - 900.0).abs() < 1e-10);
        assert!((credits.used.unwrap() - 100.0).abs() < 1e-10);
    }
}
