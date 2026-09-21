use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const BALANCE_URL: &str = "https://api.deepseek.com/user/balance";

#[derive(Deserialize)]
struct BalanceInfo {
    currency: String,
    total_balance: String,
    granted_balance: String,
    topped_up_balance: String,
}

#[derive(Deserialize)]
struct BalanceResponse {
    is_available: bool,
    balance_infos: Vec<BalanceInfo>,
}

/// Pick a preferred balance entry: a funded USD entry, else any funded entry,
/// else the USD entry, else the first entry.
fn pick_balance(infos: &[BalanceInfo]) -> Option<&BalanceInfo> {
    let is_funded = |b: &&BalanceInfo| b.total_balance.parse::<f64>().unwrap_or(0.0) > 0.0;
    infos
        .iter()
        .find(|b| b.currency.eq_ignore_ascii_case("usd") && is_funded(b))
        .or_else(|| infos.iter().find(is_funded))
        .or_else(|| infos.iter().find(|b| b.currency.eq_ignore_ascii_case("usd")))
        .or_else(|| infos.first())
}

/// Fetch account balance from the DeepSeek API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("DEEPSEEK_API_KEY").context("DEEPSEEK_API_KEY env var not set")?;

    let client = reqwest::Client::new();
    let response = client
        .get(BALANCE_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to DeepSeek API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check DEEPSEEK_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: BalanceResponse = response
        .json()
        .await
        .context("Failed to parse DeepSeek balance response")?;

    let selected = pick_balance(&data.balance_infos);

    let credits = selected.map(|b| {
        let total: f64 = b.total_balance.parse().unwrap_or(0.0);
        let granted: f64 = b.granted_balance.parse().unwrap_or(0.0);
        let topped_up: f64 = b.topped_up_balance.parse().unwrap_or(0.0);
        CreditsSnapshot {
            remaining: total.max(0.0),
            has_credits: data.is_available && total > 0.0,
            unlimited: false,
            used: None,
            limit: Some(granted + topped_up),
            currency: Some(b.currency.to_lowercase()),
            period: None,
        }
    });

    let usage = UsageSnapshot {
        provider: Provider::DeepSeek,
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
            "is_available": true,
            "balance_infos": [
                { "currency": "USD", "total_balance": "12.34", "granted_balance": "10.00", "topped_up_balance": "2.34" }
            ]
        }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert!(data.is_available);
        assert_eq!(data.balance_infos.len(), 1);
        assert!((data.balance_infos[0].total_balance.parse::<f64>().unwrap() - 12.34).abs() < 1e-10);
    }

    #[test]
    fn pick_balance_prefers_funded_usd() {
        let infos = vec![
            BalanceInfo {
                currency: "CNY".to_string(),
                total_balance: "50.0".to_string(),
                granted_balance: "0".to_string(),
                topped_up_balance: "50.0".to_string(),
            },
            BalanceInfo {
                currency: "USD".to_string(),
                total_balance: "10.0".to_string(),
                granted_balance: "10.0".to_string(),
                topped_up_balance: "0".to_string(),
            },
        ];
        let picked = pick_balance(&infos).unwrap();
        assert_eq!(picked.currency, "USD");
    }

    #[test]
    fn pick_balance_empty_returns_none() {
        assert!(pick_balance(&[]).is_none());
    }

    #[test]
    fn pick_balance_falls_back_to_first() {
        let infos = vec![BalanceInfo {
            currency: "JPY".to_string(),
            total_balance: "0".to_string(),
            granted_balance: "0".to_string(),
            topped_up_balance: "0".to_string(),
        }];
        let picked = pick_balance(&infos).unwrap();
        assert_eq!(picked.currency, "JPY");
    }
}
