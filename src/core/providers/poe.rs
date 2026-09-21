use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const BALANCE_URL: &str = "https://api.poe.com/usage/current_balance";

#[derive(Deserialize)]
struct BalanceResponse {
    current_point_balance: Option<f64>,
}

/// Fetch current point balance from the Poe API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("POE_API_KEY").context("POE_API_KEY env var not set")?;

    let client = reqwest::Client::new();
    let response = client
        .get(BALANCE_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Poe API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Invalid or expired Poe API token");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: BalanceResponse = response
        .json()
        .await
        .context("Failed to parse Poe balance response")?;

    let credits = data.current_point_balance.map(|balance| CreditsSnapshot {
        remaining: balance.max(0.0),
        has_credits: balance > 0.0,
        unlimited: false,
        used: None,
        limit: None,
        currency: Some("points".to_string()),
        period: None,
    });

    let usage = UsageSnapshot {
        provider: Provider::Poe,
        source: "api".to_string(),
        primary: None,
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
    fn deserialize_balance_response() {
        let json = r#"{ "current_point_balance": 12345 }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.current_point_balance, Some(12345.0));
    }

    #[test]
    fn deserialize_missing_balance() {
        let json = r#"{}"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert!(data.current_point_balance.is_none());
    }

    #[test]
    fn deserialize_string_balance() {
        let json = r#"{ "current_point_balance": "12345" }"#;
        let result: Result<BalanceResponse, _> = serde_json::from_str(json);
        // Poe's docs allow numeric strings for this field in some cases;
        // our simple f64 field only accepts JSON numbers.
        assert!(result.is_err());
    }
}
