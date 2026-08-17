use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct BalanceData {
    available_balance: f64,
    #[allow(dead_code)]
    voucher_balance: f64,
    #[allow(dead_code)]
    cash_balance: f64,
}

#[derive(Deserialize)]
struct BalanceResponse {
    code: i64,
    status: bool,
    scode: Option<String>,
    data: Option<BalanceData>,
}

fn resolve_api_key() -> Result<String> {
    std::env::var("MOONSHOT_API_KEY")
        .or_else(|_| std::env::var("MOONSHOT_KEY"))
        .context("MOONSHOT_API_KEY env var not set")
}

fn base_url_for_region(region: &str) -> &'static str {
    match region.to_lowercase().as_str() {
        "china" => "https://api.moonshot.cn",
        _ => "https://api.moonshot.ai",
    }
}

fn resolve_base_url() -> &'static str {
    base_url_for_region(&std::env::var("MOONSHOT_REGION").unwrap_or_default())
}

/// Fetch account balance from the Moonshot AI (Kimi Open Platform) API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = resolve_api_key()?;
    let url = format!("{}/v1/users/me/balance", resolve_base_url());

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Moonshot API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check MOONSHOT_API_KEY and MOONSHOT_REGION");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: BalanceResponse = response
        .json()
        .await
        .context("Failed to parse Moonshot balance response")?;

    if data.code != 0 || !data.status {
        anyhow::bail!(
            "Moonshot API error: code {}, scode {}",
            data.code,
            data.scode.unwrap_or_default()
        );
    }

    let balance = data
        .data
        .context("Moonshot response missing balance data")?;

    let credits = Some(CreditsSnapshot {
        remaining: balance.available_balance.max(0.0),
        has_credits: balance.available_balance > 0.0,
        unlimited: false,
        used: None,
        limit: None,
        currency: Some("usd".to_string()),
        period: None,
    });

    let usage = UsageSnapshot {
        provider: Provider::Moonshot,
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
    fn deserialize_success_response() {
        let json = r#"{
            "code": 0,
            "data": { "available_balance": 49.58, "voucher_balance": 50.0, "cash_balance": 12.34 },
            "scode": "0x0",
            "status": true
        }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.code, 0);
        assert!(data.status);
        assert!((data.data.unwrap().available_balance - 49.58).abs() < 1e-10);
    }

    #[test]
    fn deserialize_error_response() {
        let json = r#"{
            "code": 401,
            "data": {"available_balance":0,"voucher_balance":0,"cash_balance":0},
            "scode": "unauthorized",
            "status": false
        }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.code, 401);
        assert!(!data.status);
    }

    #[test]
    fn base_url_defaults_to_international() {
        assert_eq!(base_url_for_region(""), "https://api.moonshot.ai");
        assert_eq!(base_url_for_region("international"), "https://api.moonshot.ai");
    }

    #[test]
    fn base_url_china_region() {
        assert_eq!(base_url_for_region("china"), "https://api.moonshot.cn");
        assert_eq!(base_url_for_region("CHINA"), "https://api.moonshot.cn");
    }
}
