use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Budget {
    #[serde(default)]
    configured: bool,
    limit_micros: Option<i64>,
    spent_micros: Option<i64>,
    window_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    actual_cost_micros: i64,
}

#[derive(Deserialize)]
struct Usage {
    summary: Summary,
}

#[derive(Deserialize)]
struct UsageResponse {
    budget: Budget,
    usage: Usage,
}

fn window_key_to_reset(window_key: &str) -> Option<DateTime<Utc>> {
    let parts: Vec<&str> = window_key.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    Utc.with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
        .single()
}

/// Fetch policy-wide budget and spend from a ClawRouter deployment.
pub async fn fetch() -> Result<FetchResult> {
    let api_key =
        std::env::var("CLAWROUTER_API_KEY").context("CLAWROUTER_API_KEY env var not set")?;
    let base_url = std::env::var("CLAWROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://clawrouter.openclaw.ai".to_string());
    let base_url = base_url.trim_end_matches('/');
    let url = if base_url.ends_with("/v1") {
        format!("{}/usage", base_url)
    } else {
        format!("{}/v1/usage", base_url)
    };
    validate_endpoint(&url, "ClawRouter")?;

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to ClawRouter API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Unauthorized — check CLAWROUTER_API_KEY");
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        anyhow::bail!("Rate limited by ClawRouter");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: UsageResponse = response
        .json()
        .await
        .context("Failed to parse ClawRouter usage response")?;

    let resets_at = data
        .budget
        .window_key
        .as_deref()
        .and_then(window_key_to_reset);

    let primary = match (data.budget.spent_micros, data.budget.limit_micros) {
        (Some(spent), Some(limit)) if limit > 0 => Some(RateWindow {
            used_percent: (spent as f64 / limit as f64 * 100.0).clamp(0.0, 100.0),
            window_minutes: 0,
            resets_at,
            reset_description: None,
        }),
        _ => None,
    };

    let spend_usd = data.usage.summary.actual_cost_micros as f64 / 1_000_000.0;
    let credits = if data.budget.configured {
        Some(CreditsSnapshot {
            remaining: data
                .budget
                .limit_micros
                .zip(data.budget.spent_micros)
                .map(|(l, s)| ((l - s).max(0) as f64) / 1_000_000.0)
                .unwrap_or(0.0),
            has_credits: true,
            unlimited: false,
            used: Some(spend_usd),
            limit: data.budget.limit_micros.map(|l| l as f64 / 1_000_000.0),
            currency: Some("usd".to_string()),
            period: None,
        })
    } else {
        Some(CreditsSnapshot {
            remaining: 0.0,
            has_credits: false,
            unlimited: true,
            used: Some(spend_usd),
            limit: None,
            currency: Some("usd".to_string()),
            period: None,
        })
    };

    let usage = UsageSnapshot {
        provider: Provider::ClawRouter,
        source: "api".to_string(),
        primary,
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
            "budget": { "configured": true, "ledger": "team-1", "limitMicros": 50000000, "spentMicros": 10000000, "windowKey": "2026-08" },
            "usage": { "summary": { "requestCount": 10, "successCount": 10, "errorCount": 0, "inputTokens": 100, "outputTokens": 50, "totalTokens": 150, "actualCostMicros": 250000 } }
        }"#;
        let data: UsageResponse = serde_json::from_str(json).unwrap();
        assert!(data.budget.configured);
        assert_eq!(data.budget.limit_micros, Some(50000000));
        assert_eq!(data.usage.summary.actual_cost_micros, 250000);
    }

    #[test]
    fn window_key_to_reset_rolls_into_next_month() {
        let reset = window_key_to_reset("2026-08").unwrap();
        assert_eq!(reset.to_rfc3339(), "2026-09-01T00:00:00+00:00");
    }

    #[test]
    fn window_key_to_reset_handles_december() {
        let reset = window_key_to_reset("2026-12").unwrap();
        assert_eq!(reset.to_rfc3339(), "2027-01-01T00:00:00+00:00");
    }

    #[test]
    fn window_key_to_reset_invalid_returns_none() {
        assert!(window_key_to_reset("not-a-key").is_none());
    }
}
