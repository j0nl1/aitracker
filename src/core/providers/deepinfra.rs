use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const CHECKLIST_URL: &str = "https://api.deepinfra.com/payment/checklist?compute_owed=true";
const USAGE_URL: &str = "https://api.deepinfra.com/payment/usage?from=current";

#[derive(Deserialize)]
struct ChecklistResponse {
    stripe_balance: f64,
    recent: f64,
    limit: Option<f64>,
    #[serde(default)]
    suspended: bool,
}

#[derive(Deserialize)]
struct MonthUsage {
    total_cost: f64,
}

#[derive(Deserialize)]
struct UsageResponse {
    #[serde(default)]
    months: Vec<MonthUsage>,
}

fn resolve_api_key() -> Result<String> {
    std::env::var("DEEPINFRA_API_KEY")
        .or_else(|_| std::env::var("DEEPINFRA_TOKEN"))
        .context("DEEPINFRA_API_KEY (or DEEPINFRA_TOKEN) env var not set")
}

/// Fetch prepaid balance, current-month spend, and spending limit from DeepInfra.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = resolve_api_key()?;
    let client = reqwest::Client::new();

    let checklist_response = client
        .get(CHECKLIST_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to DeepInfra checklist API")?;

    let status = checklist_response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check DEEPINFRA_API_KEY");
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Forbidden — key can't access billing data");
    }
    if !status.is_success() {
        let body = checklist_response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let checklist: ChecklistResponse = checklist_response
        .json()
        .await
        .context("Failed to parse DeepInfra checklist response")?;

    let recent = checklist.recent;
    let available = (-(checklist.stripe_balance + recent)).max(0.0);

    let current_month_cost = match client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.json::<UsageResponse>().await {
            Ok(usage) => usage
                .months
                .last()
                .map(|m| m.total_cost / 100.0)
                .unwrap_or(recent.max(0.0)),
            Err(_) => recent.max(0.0),
        },
        _ => recent.max(0.0),
    };

    let credits = Some(CreditsSnapshot {
        remaining: available,
        has_credits: !checklist.suspended && available > 0.0,
        unlimited: false,
        used: Some(current_month_cost),
        limit: checklist.limit.filter(|l| *l > 0.0),
        currency: Some("usd".to_string()),
        period: Some("Current month".to_string()),
    });

    let usage = UsageSnapshot {
        provider: Provider::DeepInfra,
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
    fn deserialize_checklist_response() {
        let json = r#"{
            "stripe_balance": -5.0,
            "recent": 1.23,
            "limit": 100.0,
            "suspended": false
        }"#;
        let data: ChecklistResponse = serde_json::from_str(json).unwrap();
        assert!((data.stripe_balance - -5.0).abs() < 1e-10);
        assert!(!data.suspended);
    }

    #[test]
    fn deserialize_usage_response() {
        let json = r#"{
            "months": [{"period": "2026-07", "total_cost": 1234.0}],
            "initial_month": "2026.07"
        }"#;
        let data: UsageResponse = serde_json::from_str(json).unwrap();
        assert!((data.months.last().unwrap().total_cost - 1234.0).abs() < 1e-10);
    }

    #[test]
    fn available_balance_inverts_negative_stripe_balance() {
        let stripe_balance: f64 = -10.0;
        let recent: f64 = 2.0;
        let available = (-(stripe_balance + recent)).max(0.0);
        assert!((available - 8.0).abs() < 1e-10);
    }

    #[test]
    fn available_balance_zero_when_owed() {
        let stripe_balance: f64 = 10.0;
        let recent: f64 = 2.0;
        let available = (-(stripe_balance + recent)).max(0.0);
        assert_eq!(available, 0.0);
    }
}
