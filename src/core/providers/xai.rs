use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const BASE_URL: &str = "https://management-api.x.ai";

#[derive(Deserialize)]
struct Total {
    val: String,
}

#[derive(Deserialize)]
struct BalanceResponse {
    total: Total,
}

fn team_id_valid(team_id: &str) -> bool {
    !team_id.is_empty() && team_id != "." && team_id != ".." && !team_id.contains('/')
}

fn parse_balance_usd(val: &str) -> Result<f64> {
    let trimmed = val.trim();
    let is_valid_number = !trimmed.is_empty()
        && trimmed
            .trim_start_matches('-')
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.');
    if !is_valid_number {
        anyhow::bail!("xAI returned a non-numeric balance value: {}", val);
    }
    let cents: f64 = trimmed
        .parse()
        .with_context(|| format!("Failed to parse xAI balance value: {}", val))?;
    Ok(-cents / 100.0)
}

/// Fetch prepaid credit balance from the xAI Management API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("XAI_MANAGEMENT_API_KEY")
        .context("XAI_MANAGEMENT_API_KEY env var not set")?;
    let team_id = std::env::var("XAI_TEAM_ID").context("XAI_TEAM_ID env var not set")?;

    if !team_id_valid(&team_id) {
        anyhow::bail!("XAI_TEAM_ID is invalid");
    }

    let url = format!(
        "{}/v1/billing/teams/{}/prepaid/balance",
        BASE_URL,
        urlencoding_path(&team_id)
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .context("Failed to send request to xAI Management API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("xAI rejected the Management API key — inference API keys are not accepted");
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("Team not found — check XAI_TEAM_ID belongs to this Management API key");
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        anyhow::bail!("Rate limited by xAI");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: BalanceResponse = response
        .json()
        .await
        .context("Failed to parse xAI balance response")?;

    let balance_usd = parse_balance_usd(&data.total.val)?;

    let credits = Some(CreditsSnapshot {
        remaining: balance_usd.max(0.0),
        has_credits: balance_usd > 0.0,
        unlimited: false,
        used: None,
        limit: None,
        currency: Some("usd".to_string()),
        period: Some("Prepaid credits".to_string()),
    });

    let usage = UsageSnapshot {
        provider: Provider::Xai,
        source: "api".to_string(),
        primary: None,
        secondary: None,
        tertiary: None,
        identity: None,
    };

    Ok(FetchResult { usage, credits })
}

fn urlencoding_path(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_balance_response() {
        let json = r#"{ "total": { "val": "-1000" } }"#;
        let data: BalanceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.total.val, "-1000");
    }

    #[test]
    fn parse_balance_usd_inverts_and_scales() {
        let usd = parse_balance_usd("-1000").unwrap();
        assert!((usd - 10.0).abs() < 1e-9);
    }

    #[test]
    fn parse_balance_usd_positive_means_owed() {
        let usd = parse_balance_usd("500").unwrap();
        assert!((usd - -5.0).abs() < 1e-9);
    }

    #[test]
    fn parse_balance_usd_rejects_non_numeric() {
        assert!(parse_balance_usd("not-a-number").is_err());
    }

    #[test]
    fn team_id_validation() {
        assert!(team_id_valid("team-123"));
        assert!(!team_id_valid(""));
        assert!(!team_id_valid("."));
        assert!(!team_id_valid(".."));
        assert!(!team_id_valid("has/slash"));
    }
}
