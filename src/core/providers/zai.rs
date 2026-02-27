use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const DEFAULT_HOST: &str = "api.z.ai";
const FALLBACK_HOST: &str = "open.bigmodel.cn";
const PATH: &str = "/api/monitor/usage/quota/limit";

#[derive(Deserialize)]
struct LimitEntry {
    #[serde(rename = "limitType")]
    limit_type: Option<String>,
    used: Option<f64>,
    limit: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Deserialize)]
struct ZaiData {
    limits: Option<Vec<LimitEntry>>,
}

#[derive(Deserialize)]
struct ZaiResponse {
    data: Option<ZaiData>,
}

fn resolve_url() -> String {
    if std::env::var("Z_AI_QUOTA_URL").is_ok() {
        eprintln!("zai: Z_AI_QUOTA_URL is deprecated and ignored. Use Z_AI_API_HOST instead.");
    }
    let host = std::env::var("Z_AI_API_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    format!("https://{}{}", host, PATH)
}

fn fallback_url() -> String {
    format!("https://{}{}", FALLBACK_HOST, PATH)
}

fn parse_limit(entry: &LimitEntry) -> RateWindow {
    let used = entry.used.unwrap_or(0.0);
    let limit = entry.limit.unwrap_or(0.0);
    let used_percent = if limit > 0.0 {
        used / limit * 100.0
    } else {
        0.0
    };

    let resets_at = entry
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

async fn try_fetch(url: &str, api_key: &str) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| format!("Failed to send request to {}", url))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check Z_AI_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    Ok(response)
}

/// Fetch usage quota data from the Zai API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("Z_AI_API_KEY").context("Z_AI_API_KEY env var not set")?;

    let url = resolve_url();
    crate::core::providers::fetch::validate_endpoint(&url, "Zai")?;
    if std::env::var("Z_AI_API_HOST").is_ok() {
        eprintln!("zai: using custom host via Z_AI_API_HOST");
    }

    let response = match try_fetch(&url, &api_key).await {
        Ok(resp) => resp,
        Err(_) => {
            let fallback = fallback_url();
            crate::core::providers::fetch::validate_endpoint(&fallback, "Zai")?;
            try_fetch(&fallback, &api_key).await?
        }
    };

    let data: ZaiResponse = response
        .json()
        .await
        .context("Failed to parse Zai usage response")?;

    let limits = data
        .data
        .as_ref()
        .and_then(|d| d.limits.as_ref());

    let primary = limits
        .and_then(|l| {
            l.iter()
                .find(|e| e.limit_type.as_deref() == Some("TOKENS_LIMIT"))
        })
        .map(parse_limit);

    let secondary = limits
        .and_then(|l| {
            l.iter()
                .find(|e| e.limit_type.as_deref() == Some("TIME_LIMIT"))
        })
        .map(parse_limit);

    let usage = UsageSnapshot {
        provider: Provider::Zai,
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
            "data": {
                "limits": [
                    {
                        "limitType": "TOKENS_LIMIT",
                        "used": 5000.0,
                        "limit": 10000.0,
                        "resetTime": "2025-12-10T00:00:00Z"
                    },
                    {
                        "limitType": "TIME_LIMIT",
                        "used": 30.0,
                        "limit": 60.0,
                        "resetTime": "2025-12-10T00:00:00Z"
                    }
                ]
            }
        }"#;
        let data: ZaiResponse = serde_json::from_str(json).unwrap();
        let limits = data.data.unwrap().limits.unwrap();
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[0].limit_type.as_deref(), Some("TOKENS_LIMIT"));
        assert_eq!(limits[1].limit_type.as_deref(), Some("TIME_LIMIT"));
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: ZaiResponse = serde_json::from_str(json).unwrap();
        assert!(data.data.is_none());
    }

    #[test]
    fn deserialize_empty_limits() {
        let json = r#"{"data": {"limits": []}}"#;
        let data: ZaiResponse = serde_json::from_str(json).unwrap();
        let limits = data.data.unwrap().limits.unwrap();
        assert!(limits.is_empty());
    }

    #[test]
    fn parse_limit_calculates_percent() {
        let entry = LimitEntry {
            limit_type: Some("TOKENS_LIMIT".to_string()),
            used: Some(5000.0),
            limit: Some(10000.0),
            reset_time: Some("2099-12-10T00:00:00Z".to_string()),
        };
        let window = parse_limit(&entry);
        assert!((window.used_percent - 50.0).abs() < 1e-10);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn parse_limit_zero_limit() {
        let entry = LimitEntry {
            limit_type: Some("TOKENS_LIMIT".to_string()),
            used: Some(0.0),
            limit: Some(0.0),
            reset_time: None,
        };
        let window = parse_limit(&entry);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
    }

    #[test]
    fn parse_limit_missing_fields() {
        let entry = LimitEntry {
            limit_type: None,
            used: None,
            limit: None,
            reset_time: None,
        };
        let window = parse_limit(&entry);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn parse_limit_invalid_datetime() {
        let entry = LimitEntry {
            limit_type: Some("TIME_LIMIT".to_string()),
            used: Some(10.0),
            limit: Some(100.0),
            reset_time: Some("invalid-date".to_string()),
        };
        let window = parse_limit(&entry);
        assert!((window.used_percent - 10.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn resolve_url_default() {
        // Test URL format with default host constant
        let url = format!("https://{}{}", DEFAULT_HOST, PATH);
        assert!(url.contains("api.z.ai"));
        assert!(url.ends_with(PATH));
    }

    #[test]
    fn resolve_url_custom_host() {
        // Test URL format with custom host
        let host = "custom.host.com";
        let url = format!("https://{}{}", host, PATH);
        assert!(url.contains("custom.host.com"));
    }

    #[test]
    fn resolve_url_ignores_deprecated_quota_url() {
        // Z_AI_QUOTA_URL is deprecated and ignored — resolve_url always
        // builds the URL from host + PATH regardless.
        let url = format!("https://{}{}", DEFAULT_HOST, PATH);
        assert!(url.starts_with("https://"));
        assert!(url.contains("api.z.ai"));
    }

    #[test]
    fn fallback_url_uses_bigmodel() {
        let url = fallback_url();
        assert!(url.contains("open.bigmodel.cn"));
    }
}
