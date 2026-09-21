use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct Balance {
    credits_remaining_usd: Option<f64>,
    total_credits_usd: Option<f64>,
    credits_used_usd: Option<f64>,
}

#[derive(Deserialize)]
struct QuotaResponse {
    balance: Option<Balance>,
}

/// Fetch prepaid credit balance from the NeuralWatt API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key =
        std::env::var("NEURALWATT_API_KEY").context("NEURALWATT_API_KEY env var not set")?;
    let base =
        std::env::var("NEURALWATT_API_URL").unwrap_or_else(|_| "https://api.neuralwatt.com".to_string());
    let base = base.trim_end_matches('/');
    let url = if base.ends_with("/v1") {
        format!("{}/quota", base)
    } else {
        format!("{}/v1/quota", base)
    };
    validate_endpoint(&url, "NeuralWatt")?;

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to NeuralWatt API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Missing or invalid credentials — check NEURALWATT_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: QuotaResponse = response
        .json()
        .await
        .context("Failed to parse NeuralWatt quota response")?;

    let balance = data
        .balance
        .context("NeuralWatt response missing balance object")?;

    let remaining = balance.credits_remaining_usd.unwrap_or_else(|| {
        balance
            .total_credits_usd
            .unwrap_or(0.0)
            - balance.credits_used_usd.unwrap_or(0.0)
    });

    let credits = Some(CreditsSnapshot {
        remaining: remaining.max(0.0),
        has_credits: remaining > 0.0,
        unlimited: false,
        used: balance.credits_used_usd,
        limit: balance.total_credits_usd,
        currency: Some("usd".to_string()),
        period: Some("NeuralWatt prepaid balance".to_string()),
    });

    let usage = UsageSnapshot {
        provider: Provider::NeuralWatt,
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
            "balance": {
                "credits_remaining_usd": 32.6774,
                "total_credits_usd": 52.34,
                "credits_used_usd": 19.6626,
                "accounting_method": "energy"
            }
        }"#;
        let data: QuotaResponse = serde_json::from_str(json).unwrap();
        let balance = data.balance.unwrap();
        assert!((balance.credits_remaining_usd.unwrap() - 32.6774).abs() < 1e-9);
    }

    #[test]
    fn deserialize_missing_balance() {
        let json = r#"{}"#;
        let data: QuotaResponse = serde_json::from_str(json).unwrap();
        assert!(data.balance.is_none());
    }

    #[test]
    fn remaining_derived_when_missing() {
        let total = 50.0;
        let used = 12.0;
        let remaining = total - used;
        assert_eq!(remaining, 38.0);
    }
}
