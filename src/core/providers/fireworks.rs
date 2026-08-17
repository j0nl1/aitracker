use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct Money {
    #[serde(rename = "currencyCode")]
    currency_code: Option<String>,
    units: Option<String>,
    nanos: Option<i64>,
}

impl Money {
    fn as_f64(&self) -> f64 {
        let units: f64 = self.units.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
        let nanos = self.nanos.unwrap_or(0) as f64 / 1_000_000_000.0;
        units + nanos
    }
}

#[derive(Deserialize)]
struct LineItem {
    #[serde(rename = "totalCost")]
    total_cost: Option<Money>,
}

#[derive(Deserialize)]
struct BillingSummaryResponse {
    #[serde(rename = "lineItems", default)]
    line_items: Vec<LineItem>,
}

fn account_slug_valid(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Fetch 30-day rated spend from the Fireworks billing summary API.
/// Fireworks has no credit-balance endpoint — only rated spend is available.
pub async fn fetch() -> Result<FetchResult> {
    let api_key =
        std::env::var("FIREWORKS_API_KEY").context("FIREWORKS_API_KEY env var not set")?;
    let account_slug = std::env::var("FIREWORKS_ACCOUNT_SLUG")
        .context("FIREWORKS_ACCOUNT_SLUG env var not set")?;

    if !account_slug_valid(&account_slug) {
        anyhow::bail!("FIREWORKS_ACCOUNT_SLUG contains invalid characters");
    }

    let end_time = Utc::now();
    let start_time = end_time - chrono::Duration::days(30);

    let url = format!(
        "https://api.fireworks.ai/v1/accounts/{}/billing/summary?startTime={}&endTime={}",
        account_slug,
        start_time.to_rfc3339(),
        end_time.to_rfc3339()
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Fireworks API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Authentication rejected — check FIREWORKS_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: BillingSummaryResponse = response
        .json()
        .await
        .context("Failed to parse Fireworks billing summary response")?;

    let mut total = 0.0;
    let mut currency: Option<String> = None;
    for item in &data.line_items {
        if let Some(cost) = &item.total_cost {
            if currency.is_none() {
                currency = cost.currency_code.clone();
            }
            if currency.as_deref() == cost.currency_code.as_deref() {
                total += cost.as_f64();
            }
        }
    }

    let credits = Some(CreditsSnapshot {
        remaining: 0.0,
        has_credits: false,
        unlimited: false,
        used: Some(total),
        limit: None,
        currency: currency.or_else(|| Some("usd".to_string())),
        period: Some("Last 30 days".to_string()),
    });

    let usage = UsageSnapshot {
        provider: Provider::Fireworks,
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
            "lineItems": [
                { "totalCost": { "currencyCode": "USD", "units": "12", "nanos": 500000000 } },
                { "totalCost": { "currencyCode": "USD", "units": "3", "nanos": 0 } }
            ]
        }"#;
        let data: BillingSummaryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.line_items.len(), 2);
        let total: f64 = data
            .line_items
            .iter()
            .filter_map(|i| i.total_cost.as_ref())
            .map(|m| m.as_f64())
            .sum();
        assert!((total - 15.5).abs() < 1e-9);
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: BillingSummaryResponse = serde_json::from_str(json).unwrap();
        assert!(data.line_items.is_empty());
    }

    #[test]
    fn money_as_f64_handles_missing_fields() {
        let m = Money {
            currency_code: None,
            units: None,
            nanos: None,
        };
        assert_eq!(m.as_f64(), 0.0);
    }

    #[test]
    fn account_slug_validation() {
        assert!(account_slug_valid("x0mh0x"));
        assert!(account_slug_valid("my-account_1.2"));
        assert!(!account_slug_valid(""));
        assert!(!account_slug_valid("has/slash"));
    }
}
