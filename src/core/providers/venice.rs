use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const BALANCE_URL: &str = "https://api.venice.ai/api/v1/billing/balance";

#[derive(Deserialize)]
struct Balances {
    diem: Option<f64>,
    usd: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalanceResponse {
    #[serde(default = "default_can_consume")]
    can_consume: bool,
    consumption_currency: Option<String>,
    balances: Balances,
}

fn default_can_consume() -> bool {
    true
}

fn resolve_api_key() -> Result<String> {
    std::env::var("VENICE_API_KEY")
        .or_else(|_| std::env::var("VENICE_KEY"))
        .context("VENICE_API_KEY env var not set")
}

/// Fetch DIEM or USD balance from the Venice AI billing API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = resolve_api_key()?;

    let client = reqwest::Client::new();
    let response = client
        .get(BALANCE_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Venice API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Venice API error: HTTP {}: {}", status.as_u16(), body);
    }

    let data: BalanceResponse = response
        .json()
        .await
        .context("Failed to parse Venice balance response")?;

    let currency = data.consumption_currency.map(|c| c.to_uppercase());
    let use_usd = currency.as_deref() == Some("USD") && data.balances.usd.unwrap_or(0.0) > 0.0;

    let credits = if !data.can_consume {
        Some(CreditsSnapshot {
            remaining: 0.0,
            has_credits: false,
            unlimited: false,
            used: None,
            limit: None,
            currency: Some("diem".to_string()),
            period: None,
        })
    } else if use_usd {
        Some(CreditsSnapshot {
            remaining: data.balances.usd.unwrap_or(0.0),
            has_credits: true,
            unlimited: false,
            used: None,
            limit: None,
            currency: Some("usd".to_string()),
            period: None,
        })
    } else if let Some(diem) = data.balances.diem.filter(|d| *d > 0.0) {
        Some(CreditsSnapshot {
            remaining: diem,
            has_credits: true,
            unlimited: false,
            used: None,
            limit: None,
            currency: Some("diem".to_string()),
            period: None,
        })
    } else if let Some(usd) = data.balances.usd.filter(|u| *u > 0.0) {
        Some(CreditsSnapshot {
            remaining: usd,
            has_credits: true,
            unlimited: false,
            used: None,
            limit: None,
            currency: Some("usd".to_string()),
            period: None,
        })
    } else {
        Some(CreditsSnapshot {
            remaining: 0.0,
            has_credits: false,
            unlimited: false,
            used: None,
            limit: None,
            currency: Some("diem".to_string()),
            period: None,
        })
    };

    let usage = UsageSnapshot {
        provider: Provider::Venice,
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
            "canConsume": true,
            "consumptionCurrency": "DIEM",
            "diemEpochAllocation": 1000.0,
            "balances": { "diem": 812.5, "usd": 0.0 }
        }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert!(data.can_consume);
        assert!((data.balances.diem.unwrap() - 812.5).abs() < 1e-10);
    }

    #[test]
    fn deserialize_missing_can_consume_defaults_true() {
        let json = r#"{ "balances": { "diem": 1.0, "usd": null } }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert!(data.can_consume);
    }

    #[test]
    fn deserialize_exhausted_response() {
        let json = r#"{
            "canConsume": false,
            "balances": { "diem": 0.0, "usd": 0.0 }
        }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert!(!data.can_consume);
    }
}
