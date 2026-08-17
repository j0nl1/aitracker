use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const CREDIT_GRANTS_URL: &str = "https://api.openai.com/v1/dashboard/billing/credit_grants";

#[derive(Deserialize)]
struct CreditGrantsResponse {
    total_granted: Option<f64>,
    total_used: Option<f64>,
    total_available: Option<f64>,
}

/// Fetch legacy credit balance from the OpenAI dashboard billing API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY env var not set")?;

    if api_key.is_empty() {
        anyhow::bail!("OPENAI_API_KEY is empty");
    }

    let client = reqwest::Client::new();
    let response = client
        .get(CREDIT_GRANTS_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to OpenAI billing API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check your OPENAI_API_KEY (must be a legacy/user key with billing access)");
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Forbidden — project/service-account keys can't access legacy billing data; use a legacy user key");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: CreditGrantsResponse = response
        .json()
        .await
        .context("Failed to parse OpenAI billing response")?;

    let granted = data.total_granted.unwrap_or(0.0);
    let used = data.total_used.unwrap_or(0.0);
    let available = data.total_available.unwrap_or((granted - used).max(0.0));

    let credits = Some(CreditsSnapshot {
        remaining: available.max(0.0),
        has_credits: available > 0.0,
        unlimited: false,
        used: Some(used),
        limit: Some(granted),
        currency: Some("usd".to_string()),
        period: None,
    });

    let usage = UsageSnapshot {
        provider: Provider::OpenAi,
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
            "total_granted": 100.0,
            "total_used": 37.5,
            "total_available": 62.5
        }"#;
        let data: CreditGrantsResponse = serde_json::from_str(json).unwrap();
        assert!((data.total_granted.unwrap() - 100.0).abs() < 1e-10);
        assert!((data.total_available.unwrap() - 62.5).abs() < 1e-10);
    }

    #[test]
    fn deserialize_partial_response() {
        let json = r#"{}"#;
        let data: CreditGrantsResponse = serde_json::from_str(json).unwrap();
        assert!(data.total_granted.is_none());
        assert!(data.total_used.is_none());
        assert!(data.total_available.is_none());
    }

    #[test]
    fn available_derived_when_missing() {
        let granted: f64 = 50.0;
        let used: f64 = 20.0;
        let available = (granted - used).max(0.0);
        assert!((available - 30.0).abs() < 1e-10);
    }
}
