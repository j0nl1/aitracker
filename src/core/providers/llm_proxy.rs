use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize)]
#[serde(untagged)]
enum QuotaGroups {
    List(Vec<QuotaGroup>),
    Map(std::collections::HashMap<String, QuotaGroup>),
}

impl QuotaGroups {
    fn into_vec(self) -> Vec<QuotaGroup> {
        match self {
            QuotaGroups::List(v) => v,
            QuotaGroups::Map(m) => m.into_values().collect(),
        }
    }
}

#[derive(Deserialize)]
struct QuotaGroup {
    remaining_percent: Option<f64>,
    reset_time: Option<String>,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct ProviderTokens {
    #[serde(default)]
    input_cached: u64,
    #[serde(default)]
    input_uncached: u64,
    #[serde(default)]
    output: u64,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ProviderStats {
    #[serde(default)]
    total_requests: u64,
    #[serde(default)]
    approx_cost: f64,
    #[serde(default)]
    tokens: ProviderTokens,
    #[serde(default)]
    quota_groups: Option<QuotaGroups>,
}

#[derive(Deserialize)]
struct Summary {
    #[serde(default)]
    approx_cost: f64,
}

#[derive(Deserialize)]
struct QuotaStatsResponse {
    #[serde(default)]
    providers: std::collections::HashMap<String, ProviderStats>,
    summary: Option<Summary>,
}

fn normalize_base_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{}/quota-stats", trimmed)
    } else {
        format!("{}/v1/quota-stats", trimmed)
    }
}

/// Fetch aggregate quota stats from a self-hosted LLM Proxy gateway.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("LLM_PROXY_API_KEY").context("LLM_PROXY_API_KEY env var not set")?;
    let base_url =
        std::env::var("LLM_PROXY_BASE_URL").context("LLM_PROXY_BASE_URL env var not set")?;

    let url = normalize_base_url(&base_url);
    validate_endpoint(&url, "LLM Proxy")?;

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to LLM Proxy")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: QuotaStatsResponse = response
        .json()
        .await
        .context("Failed to parse LLM Proxy quota-stats response")?;

    let now = Utc::now();
    let mut min_remaining: Option<f64> = None;
    let mut next_reset: Option<DateTime<Utc>> = None;
    let mut total_cost = 0.0;

    for provider in data.providers.values() {
        total_cost += provider.approx_cost;
    }

    for provider in data.providers.into_values() {
        if let Some(groups) = provider.quota_groups {
            for group in groups.into_vec() {
                if let Some(pct) = group.remaining_percent {
                    min_remaining = Some(min_remaining.map_or(pct, |m: f64| m.min(pct)));
                }
                if let Some(reset_str) = group.reset_time {
                    if let Ok(reset) = reset_str.parse::<DateTime<Utc>>() {
                        if reset > now {
                            next_reset = Some(next_reset.map_or(reset, |n| n.min(reset)));
                        }
                    }
                }
            }
        }
    }

    let primary = min_remaining.map(|remaining| RateWindow {
        used_percent: (100.0 - remaining).clamp(0.0, 100.0),
        window_minutes: 0,
        resets_at: next_reset,
        reset_description: None,
    });

    let approx_cost = data.summary.map(|s| s.approx_cost).unwrap_or(total_cost);
    let credits = if approx_cost > 0.0 {
        Some(CreditsSnapshot {
            remaining: 0.0,
            has_credits: false,
            unlimited: false,
            used: Some(approx_cost),
            limit: None,
            currency: Some("usd".to_string()),
            period: Some("Approx. spend".to_string()),
        })
    } else {
        None
    };

    let usage = UsageSnapshot {
        provider: Provider::LlmProxy,
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
    fn normalize_base_url_appends_v1() {
        assert_eq!(
            normalize_base_url("https://proxy.example.com"),
            "https://proxy.example.com/v1/quota-stats"
        );
    }

    #[test]
    fn normalize_base_url_keeps_existing_v1() {
        assert_eq!(
            normalize_base_url("https://proxy.example.com/v1"),
            "https://proxy.example.com/v1/quota-stats"
        );
    }

    #[test]
    fn deserialize_array_quota_groups() {
        let json = r#"{
            "providers": {
                "openai": { "total_requests": 10, "approx_cost": 1.5, "tokens": {}, "quota_groups": [{"remaining_percent": 42.0, "reset_time": "2099-01-01T00:00:00Z"}] }
            },
            "summary": { "total_requests": 10, "approx_cost": 1.5, "total_tokens": 100 }
        }"#;
        let data: QuotaStatsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.providers.len(), 1);
    }

    #[test]
    fn deserialize_map_quota_groups() {
        let json = r#"{
            "providers": {
                "openai": { "total_requests": 10, "approx_cost": 1.5, "tokens": {}, "quota_groups": {"a": {"remaining_percent": 10.0, "reset_time": null}} }
            }
        }"#;
        let data: QuotaStatsResponse = serde_json::from_str(json).unwrap();
        let provider = data.providers.get("openai").unwrap();
        let groups = provider.quota_groups.as_ref().unwrap();
        match groups {
            QuotaGroups::Map(m) => assert_eq!(m.len(), 1),
            _ => panic!("expected map"),
        }
    }
}
