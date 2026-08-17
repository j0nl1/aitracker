use anyhow::{Context, Result};

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const PROBE_URL: &str = "https://ark.cn-beijing.volces.com/api/coding/v3/chat/completions";
const PROBE_MODELS: &[&str] = &["doubao-seed-2.0-code", "doubao-1.5-pro-32k", "doubao-lite-32k"];

fn resolve_api_key() -> Result<String> {
    std::env::var("ARK_API_KEY")
        .or_else(|_| std::env::var("VOLCENGINE_API_KEY"))
        .or_else(|_| std::env::var("DOUBAO_API_KEY"))
        .context("ARK_API_KEY env var not set")
}

fn parse_rate_headers(headers: &reqwest::header::HeaderMap) -> Option<RateWindow> {
    let remaining: f64 = headers
        .get("x-ratelimit-remaining-requests")?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    let limit: f64 = headers
        .get("x-ratelimit-limit-requests")?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    if limit <= 0.0 {
        return None;
    }
    let used = (limit - remaining).max(0.0);
    Some(RateWindow {
        used_percent: (used / limit * 100.0).clamp(0.0, 100.0),
        window_minutes: 0,
        resets_at: None,
        reset_description: Some(format!("{}/{} requests", used as i64, limit as i64)),
    })
}

/// Probe the Ark (Volcengine Doubao) API with a minimal chat completion and
/// read request-limit data from the response headers — Doubao's Ark API-key
/// path exposes no direct balance/quota endpoint.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = resolve_api_key()?;
    let client = reqwest::Client::new();

    let mut last_status: Option<reqwest::StatusCode> = None;
    for model in PROBE_MODELS {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "hi"}],
        });

        let response = client
            .post(PROBE_URL)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Doubao Ark API")?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("Unauthorized — check ARK_API_KEY");
        }
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::FORBIDDEN {
            last_status = Some(status);
            continue;
        }
        if status.is_success() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let primary = parse_rate_headers(response.headers());
            let usage = UsageSnapshot {
                provider: Provider::Doubao,
                source: "api".to_string(),
                primary,
                secondary: None,
                tertiary: None,
                identity: None,
            };
            return Ok(FetchResult {
                usage,
                credits: None,
            });
        }
        last_status = Some(status);
    }

    anyhow::bail!(
        "All Doubao probe models failed, last status: {}",
        last_status
            .map(|s| s.as_u16().to_string())
            .unwrap_or_else(|| "none".to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    #[test]
    fn parse_rate_headers_computes_used_percent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-ratelimit-remaining-requests",
            HeaderValue::from_static("75"),
        );
        headers.insert(
            "x-ratelimit-limit-requests",
            HeaderValue::from_static("100"),
        );
        let window = parse_rate_headers(&headers).unwrap();
        assert!((window.used_percent - 25.0).abs() < 1e-9);
    }

    #[test]
    fn parse_rate_headers_missing_returns_none() {
        let headers = HeaderMap::new();
        assert!(parse_rate_headers(&headers).is_none());
    }

    #[test]
    fn parse_rate_headers_zero_limit_returns_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-ratelimit-remaining-requests",
            HeaderValue::from_static("0"),
        );
        headers.insert("x-ratelimit-limit-requests", HeaderValue::from_static("0"));
        assert!(parse_rate_headers(&headers).is_none());
    }

    #[test]
    fn probe_models_list_is_ordered() {
        assert_eq!(PROBE_MODELS[0], "doubao-seed-2.0-code");
    }
}
