use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::auth::decode_jwt_claims;
use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const USAGE_URL: &str =
    "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages";

#[derive(Deserialize)]
struct UsageDetail {
    limit: Option<f64>,
    used: Option<f64>,
    remaining: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Deserialize)]
struct UsageEntry {
    detail: Option<UsageDetail>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LimitEntry {
    limit: Option<f64>,
    used: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Deserialize)]
struct KimiUsageResponse {
    usages: Option<Vec<UsageEntry>>,
    #[allow(dead_code)]
    limits: Option<Vec<LimitEntry>>,
}

fn parse_usage_window(detail: &UsageDetail) -> RateWindow {
    let used = detail.used.unwrap_or(0.0);
    let limit = detail.limit.unwrap_or(0.0);
    let used_percent = if limit > 0.0 {
        used / limit * 100.0
    } else {
        0.0
    };

    let resets_at = detail
        .reset_time
        .as_deref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    let window_minutes = resets_at
        .map(|r| {
            let now = Utc::now();
            if r > now {
                (r - now).num_minutes().max(0) as u64
            } else {
                0
            }
        })
        .unwrap_or(0);

    RateWindow {
        used_percent,
        window_minutes,
        resets_at,
        reset_description: None,
    }
}

fn parse_credits(detail: &UsageDetail) -> CreditsSnapshot {
    let remaining = detail.remaining.unwrap_or(0.0);
    let used = detail.used;
    let limit = detail.limit;

    CreditsSnapshot {
        remaining,
        has_credits: remaining > 0.0,
        unlimited: false,
        used,
        limit,
        currency: None,
        period: None,
    }
}

/// Fetch usage data from the Kimi billing API.
pub async fn fetch() -> Result<FetchResult> {
    let token = std::env::var("KIMI_TOKEN").context("KIMI_TOKEN env var not set")?;

    let claims = decode_jwt_claims(&token).context("Failed to decode KIMI_TOKEN JWT")?;

    let device_id = claims["device_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let ssid = claims["ssid"].as_str().unwrap_or_default().to_string();
    let sub = claims["sub"].as_str().unwrap_or_default().to_string();

    let client = reqwest::Client::new();
    let response = client
        .post(USAGE_URL)
        .header("Authorization", format!("Bearer {}", token))
        .header("Cookie", format!("refresh_token={}", token))
        .header("x-device-id", &device_id)
        .header("x-ssid", &ssid)
        .header("x-sub", &sub)
        .header("Content-Type", "application/json")
        .body(r#"{"scope":["FEATURE_CODING"]}"#)
        .send()
        .await
        .context("Failed to send request to Kimi API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check KIMI_TOKEN");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: KimiUsageResponse = response
        .json()
        .await
        .context("Failed to parse Kimi usage response")?;

    let detail = data
        .usages
        .as_ref()
        .and_then(|u| u.first())
        .and_then(|e| e.detail.as_ref());

    let primary = detail.map(parse_usage_window);
    let credits = detail.map(parse_credits);

    let usage = UsageSnapshot {
        provider: Provider::Kimi,
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
            "usages": [{
                "detail": {
                    "limit": 1000.0,
                    "used": 250.0,
                    "remaining": 750.0,
                    "resetTime": "2025-12-10T00:00:00Z"
                }
            }],
            "limits": [{
                "limit": 500.0,
                "used": 100.0,
                "resetTime": "2025-12-10T00:00:00Z"
            }]
        }"#;
        let data: KimiUsageResponse = serde_json::from_str(json).unwrap();
        let usages = data.usages.unwrap();
        assert_eq!(usages.len(), 1);
        let detail = usages[0].detail.as_ref().unwrap();
        assert!((detail.limit.unwrap() - 1000.0).abs() < 1e-10);
        assert!((detail.used.unwrap() - 250.0).abs() < 1e-10);
        assert!((detail.remaining.unwrap() - 750.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: KimiUsageResponse = serde_json::from_str(json).unwrap();
        assert!(data.usages.is_none());
        assert!(data.limits.is_none());
    }

    #[test]
    fn parse_usage_window_calculates_percent() {
        let detail = UsageDetail {
            limit: Some(1000.0),
            used: Some(250.0),
            remaining: Some(750.0),
            reset_time: Some("2099-12-10T00:00:00Z".to_string()),
        };
        let window = parse_usage_window(&detail);
        assert!((window.used_percent - 25.0).abs() < 1e-10);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn parse_usage_window_zero_limit() {
        let detail = UsageDetail {
            limit: Some(0.0),
            used: Some(0.0),
            remaining: Some(0.0),
            reset_time: None,
        };
        let window = parse_usage_window(&detail);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn parse_usage_window_invalid_datetime() {
        let detail = UsageDetail {
            limit: Some(100.0),
            used: Some(50.0),
            remaining: Some(50.0),
            reset_time: Some("not-a-date".to_string()),
        };
        let window = parse_usage_window(&detail);
        assert!((window.used_percent - 50.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn parse_credits_from_detail() {
        let detail = UsageDetail {
            limit: Some(1000.0),
            used: Some(250.0),
            remaining: Some(750.0),
            reset_time: None,
        };
        let credits = parse_credits(&detail);
        assert!((credits.remaining - 750.0).abs() < 1e-10);
        assert!(credits.has_credits);
        assert!(!credits.unlimited);
        assert!((credits.used.unwrap() - 250.0).abs() < 1e-10);
        assert!((credits.limit.unwrap() - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn parse_credits_zero_remaining() {
        let detail = UsageDetail {
            limit: Some(100.0),
            used: Some(100.0),
            remaining: Some(0.0),
            reset_time: None,
        };
        let credits = parse_credits(&detail);
        assert!((credits.remaining - 0.0).abs() < 1e-10);
        assert!(!credits.has_credits);
    }

    #[test]
    fn deserialize_partial_response_no_detail() {
        let json = r#"{
            "usages": [{}]
        }"#;
        let data: KimiUsageResponse = serde_json::from_str(json).unwrap();
        let usages = data.usages.unwrap();
        assert!(usages[0].detail.is_none());
    }
}
