use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const QUOTAS_URL: &str = "https://api.synthetic.new/v2/quotas";

#[derive(Deserialize)]
#[allow(dead_code)]
struct QuotaEntry {
    #[serde(rename = "percentUsed")]
    percent_used: Option<f64>,
    limit: Option<f64>,
    used: Option<f64>,
    remaining: Option<f64>,
    #[serde(rename = "resetAt")]
    reset_at: Option<String>,
    #[serde(rename = "windowHours")]
    window_hours: Option<u64>,
}

#[derive(Deserialize)]
struct SyntheticResponse {
    quotas: Option<Vec<QuotaEntry>>,
}

fn parse_quota(entry: &QuotaEntry) -> RateWindow {
    let used_percent = entry.percent_used.unwrap_or(0.0);

    let resets_at = entry
        .reset_at
        .as_deref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    let window_minutes = entry.window_hours.map(|h| h * 60).unwrap_or(0);

    RateWindow {
        used_percent,
        window_minutes,
        resets_at,
        reset_description: None,
    }
}

/// Fetch quota data from the Synthetic API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key =
        std::env::var("SYNTHETIC_API_KEY").context("SYNTHETIC_API_KEY env var not set")?;

    let client = reqwest::Client::new();
    let response = client
        .get(QUOTAS_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Synthetic API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check SYNTHETIC_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: SyntheticResponse = response
        .json()
        .await
        .context("Failed to parse Synthetic quotas response")?;

    let quotas = data.quotas.as_deref().unwrap_or_default();
    let primary = quotas.first().map(parse_quota);
    let secondary = quotas.get(1).map(parse_quota);

    let usage = UsageSnapshot {
        provider: Provider::Synthetic,
        source: "api".to_string(),
        primary,
        secondary,
        tertiary: None,
        identity: None,
    };

    Ok(FetchResult {
        usage,
        credits: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "quotas": [
                {
                    "percentUsed": 42.5,
                    "limit": 1000,
                    "used": 425,
                    "remaining": 575,
                    "resetAt": "2025-12-10T00:00:00Z",
                    "windowHours": 24
                },
                {
                    "percentUsed": 10.0,
                    "limit": 5000,
                    "used": 500,
                    "remaining": 4500,
                    "resetAt": "2025-12-17T00:00:00Z",
                    "windowHours": 168
                }
            ]
        }"#;
        let data: SyntheticResponse = serde_json::from_str(json).unwrap();
        let quotas = data.quotas.unwrap();
        assert_eq!(quotas.len(), 2);
        assert!((quotas[0].percent_used.unwrap() - 42.5).abs() < 1e-10);
        assert!((quotas[1].percent_used.unwrap() - 10.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_single_quota() {
        let json = r#"{
            "quotas": [{
                "percentUsed": 75.0,
                "limit": 100,
                "used": 75,
                "remaining": 25,
                "resetAt": "2025-12-10T00:00:00Z",
                "windowHours": 4
            }]
        }"#;
        let data: SyntheticResponse = serde_json::from_str(json).unwrap();
        let quotas = data.quotas.unwrap();
        assert_eq!(quotas.len(), 1);
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: SyntheticResponse = serde_json::from_str(json).unwrap();
        assert!(data.quotas.is_none());
    }

    #[test]
    fn deserialize_empty_quotas() {
        let json = r#"{"quotas": []}"#;
        let data: SyntheticResponse = serde_json::from_str(json).unwrap();
        assert!(data.quotas.unwrap().is_empty());
    }

    #[test]
    fn parse_quota_entry() {
        let entry = QuotaEntry {
            percent_used: Some(42.5),
            limit: Some(1000.0),
            used: Some(425.0),
            remaining: Some(575.0),
            reset_at: Some("2099-12-10T00:00:00Z".to_string()),
            window_hours: Some(24),
        };
        let window = parse_quota(&entry);
        assert!((window.used_percent - 42.5).abs() < 1e-10);
        assert_eq!(window.window_minutes, 1440);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn parse_quota_missing_fields() {
        let entry = QuotaEntry {
            percent_used: None,
            limit: None,
            used: None,
            remaining: None,
            reset_at: None,
            window_hours: None,
        };
        let window = parse_quota(&entry);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
        assert_eq!(window.window_minutes, 0);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn parse_quota_invalid_datetime() {
        let entry = QuotaEntry {
            percent_used: Some(50.0),
            limit: Some(100.0),
            used: Some(50.0),
            remaining: Some(50.0),
            reset_at: Some("not-a-date".to_string()),
            window_hours: Some(8),
        };
        let window = parse_quota(&entry);
        assert!((window.used_percent - 50.0).abs() < 1e-10);
        assert_eq!(window.window_minutes, 480);
        assert!(window.resets_at.is_none());
    }
}
