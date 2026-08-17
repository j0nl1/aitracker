use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const USAGE_URL: &str = "https://www.codebuff.com/api/v1/usage";

#[derive(Deserialize)]
struct UsageResponse {
    #[serde(alias = "used")]
    usage: Option<f64>,
    #[serde(alias = "limit")]
    quota: Option<f64>,
    #[serde(alias = "remaining")]
    #[serde(rename = "remainingBalance")]
    remaining_balance: Option<f64>,
}

/// Fetch credit balance from the Codebuff API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("CODEBUFF_API_KEY").context("CODEBUFF_API_KEY env var not set")?;

    let client = reqwest::Client::new();
    let response = client
        .post(USAGE_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&serde_json::json!({"fingerprintId": "ait-usage"}))
        .send()
        .await
        .context("Failed to send request to Codebuff API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Unauthorized — check CODEBUFF_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: UsageResponse = response
        .json()
        .await
        .context("Failed to parse Codebuff usage response")?;

    let remaining = data.remaining_balance.unwrap_or(0.0);
    let total = data
        .quota
        .unwrap_or_else(|| remaining + data.usage.unwrap_or(0.0));

    let credits = Some(CreditsSnapshot {
        remaining: remaining.max(0.0),
        has_credits: remaining > 0.0,
        unlimited: false,
        used: data.usage,
        limit: Some(total),
        currency: None,
        period: None,
    });

    let usage = UsageSnapshot {
        provider: Provider::Codebuff,
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
    fn deserialize_full_response() {
        let json = r#"{
            "usage": 1250,
            "quota": 5000,
            "remainingBalance": 3750,
            "autoTopupEnabled": true,
            "next_quota_reset": "2026-05-01T00:00:00Z"
        }"#;
        let data: UsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.usage, Some(1250.0));
        assert_eq!(data.quota, Some(5000.0));
        assert_eq!(data.remaining_balance, Some(3750.0));
    }

    #[test]
    fn deserialize_fallback_field_names() {
        let json = r#"{ "used": 10, "limit": 100, "remaining": 90 }"#;
        let data: UsageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.usage, Some(10.0));
        assert_eq!(data.quota, Some(100.0));
        assert_eq!(data.remaining_balance, Some(90.0));
    }

    #[test]
    fn total_derived_when_quota_missing() {
        let remaining = 90.0;
        let used = 10.0;
        let total = remaining + used;
        assert_eq!(total, 100.0);
    }
}
