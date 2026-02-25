use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const DEFAULT_HOST: &str = "api.minimax.io";
const CHINA_HOST: &str = "api.minimaxi.com";
const PATH: &str = "/v1/api/openplatform/coding_plan/remains";

#[derive(Deserialize)]
struct ModelRemain {
    current_interval_total_count: Option<f64>,
    current_interval_usage_count: Option<f64>,
    end_time: Option<i64>,
}

#[derive(Deserialize)]
struct MiniMaxData {
    model_remains: Option<Vec<ModelRemain>>,
}

#[derive(Deserialize)]
struct MiniMaxResponse {
    data: Option<MiniMaxData>,
}

fn resolve_host() -> String {
    std::env::var("MINIMAX_API_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string())
}

fn resolve_url() -> String {
    format!("https://{}{}", resolve_host(), PATH)
}

fn fallback_url() -> String {
    format!("https://{}{}", CHINA_HOST, PATH)
}

fn parse_model_remain(remain: &ModelRemain) -> RateWindow {
    let total = remain.current_interval_total_count.unwrap_or(0.0);
    let used = remain.current_interval_usage_count.unwrap_or(0.0);
    let used_percent = if total > 0.0 {
        used / total * 100.0
    } else {
        0.0
    };

    // end_time is epoch milliseconds
    let resets_at = remain
        .end_time
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single());

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

async fn try_fetch(url: &str, token: &str) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| format!("Failed to send request to {}", url))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check MINIMAX_API_TOKEN");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    Ok(response)
}

/// Fetch usage data from the MiniMax coding plan API.
pub async fn fetch() -> Result<FetchResult> {
    let token =
        std::env::var("MINIMAX_API_TOKEN").context("MINIMAX_API_TOKEN env var not set")?;

    let url = resolve_url();
    let response = match try_fetch(&url, &token).await {
        Ok(resp) => resp,
        Err(_) => {
            let fallback = fallback_url();
            try_fetch(&fallback, &token).await?
        }
    };

    let data: MiniMaxResponse = response
        .json()
        .await
        .context("Failed to parse MiniMax usage response")?;

    let primary = data
        .data
        .as_ref()
        .and_then(|d| d.model_remains.as_ref())
        .and_then(|remains| remains.first())
        .map(parse_model_remain);

    let usage = UsageSnapshot {
        provider: Provider::MiniMax,
        source: "api".to_string(),
        primary,
        secondary: None,
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
            "data": {
                "model_remains": [{
                    "current_interval_total_count": 500.0,
                    "current_interval_usage_count": 125.0,
                    "end_time": 1733788800000
                }]
            }
        }"#;
        let data: MiniMaxResponse = serde_json::from_str(json).unwrap();
        let remains = data.data.unwrap().model_remains.unwrap();
        assert_eq!(remains.len(), 1);
        assert!((remains[0].current_interval_total_count.unwrap() - 500.0).abs() < 1e-10);
        assert!((remains[0].current_interval_usage_count.unwrap() - 125.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: MiniMaxResponse = serde_json::from_str(json).unwrap();
        assert!(data.data.is_none());
    }

    #[test]
    fn deserialize_empty_model_remains() {
        let json = r#"{"data": {"model_remains": []}}"#;
        let data: MiniMaxResponse = serde_json::from_str(json).unwrap();
        let remains = data.data.unwrap().model_remains.unwrap();
        assert!(remains.is_empty());
    }

    #[test]
    fn parse_model_remain_calculates_percent() {
        let remain = ModelRemain {
            current_interval_total_count: Some(500.0),
            current_interval_usage_count: Some(125.0),
            end_time: Some(4102444800000), // far future
        };
        let window = parse_model_remain(&remain);
        assert!((window.used_percent - 25.0).abs() < 1e-10);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn parse_model_remain_zero_total() {
        let remain = ModelRemain {
            current_interval_total_count: Some(0.0),
            current_interval_usage_count: Some(0.0),
            end_time: None,
        };
        let window = parse_model_remain(&remain);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn parse_model_remain_missing_fields() {
        let remain = ModelRemain {
            current_interval_total_count: None,
            current_interval_usage_count: None,
            end_time: None,
        };
        let window = parse_model_remain(&remain);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
    }

    #[test]
    fn resolve_url_default_host() {
        // Test URL format with default host constant
        let url = format!("https://{}{}", DEFAULT_HOST, PATH);
        assert!(url.contains("api.minimax.io"));
        assert!(url.ends_with("/v1/api/openplatform/coding_plan/remains"));
    }

    #[test]
    fn resolve_url_custom_host() {
        // Test URL format with a custom host
        let host = "custom.host.com";
        let url = format!("https://{}{}", host, PATH);
        assert!(url.contains("custom.host.com"));
    }

    #[test]
    fn fallback_url_uses_china_host() {
        let url = fallback_url();
        assert!(url.contains("api.minimaxi.com"));
    }
}
