use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct PromResult {
    value: Option<(serde_json::Value, serde_json::Value)>,
}

#[derive(Deserialize)]
struct PromData {
    #[serde(default)]
    result: Vec<PromResult>,
}

#[derive(Deserialize)]
struct PromResponse {
    status: String,
    data: Option<PromData>,
    error: Option<String>,
}

fn scalar_value(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

async fn query_metric(client: &reqwest::Client, base: &str, api_key: &str, query: &str) -> Result<f64> {
    let url = format!(
        "{}/metrics/prometheus/api/v1/query?query={}",
        base,
        urlencoding_encode(query)
    );
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to GroqCloud Prometheus API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Access denied — check GROQ_API_KEY (requires Enterprise tier)");
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("GroqCloud Prometheus metrics not available for this key (Enterprise-only)");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: PromResponse = response
        .json()
        .await
        .context("Failed to parse GroqCloud Prometheus response")?;

    if data.status != "success" {
        anyhow::bail!(
            "GroqCloud Prometheus query failed: {}",
            data.error.unwrap_or_default()
        );
    }

    let sum = data
        .data
        .map(|d| {
            d.result
                .iter()
                .filter_map(|r| r.value.as_ref())
                .map(|(_, v)| scalar_value(v))
                .sum()
        })
        .unwrap_or(0.0);

    Ok(sum)
}

fn urlencoding_encode(s: &str) -> String {
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

/// Fetch request/token rate metrics from GroqCloud's Enterprise Prometheus endpoint.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("GROQ_API_KEY").context("GROQ_API_KEY env var not set")?;
    let base = std::env::var("GROQ_API_URL").unwrap_or_else(|_| "https://api.groq.com/v1".to_string());
    validate_endpoint(&base, "GroqCloud")?;

    let client = reqwest::Client::new();

    let request_rate = query_metric(
        &client,
        &base,
        &api_key,
        "sum(model_project_id_status_code:requests:rate5m)",
    )
    .await?;
    let input_rate = query_metric(
        &client,
        &base,
        &api_key,
        "sum(model_project_id:tokens_in:rate5m)",
    )
    .await
    .unwrap_or(0.0);
    let output_rate = query_metric(
        &client,
        &base,
        &api_key,
        "sum(model_project_id:tokens_out:rate5m)",
    )
    .await
    .unwrap_or(0.0);

    let requests_per_minute = request_rate * 60.0;
    let tokens_per_minute = (input_rate + output_rate) * 60.0;

    let primary = Some(RateWindow {
        used_percent: 0.0,
        window_minutes: 5,
        resets_at: None,
        reset_description: Some(format!(
            "{:.0} req/min, {:.0} tok/min",
            requests_per_minute, tokens_per_minute
        )),
    });

    let usage = UsageSnapshot {
        provider: Provider::GroqCloud,
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
    fn deserialize_success_response() {
        let json = r#"{
            "status": "success",
            "data": { "result": [ { "value": [1234567890, "12.5"] } ] }
        }"#;
        let data: PromResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.status, "success");
        let sum: f64 = data
            .data
            .unwrap()
            .result
            .iter()
            .filter_map(|r| r.value.as_ref())
            .map(|(_, v)| scalar_value(v))
            .sum();
        assert!((sum - 12.5).abs() < 1e-9);
    }

    #[test]
    fn deserialize_error_response() {
        let json = r#"{ "status": "error", "error": "bad query" }"#;
        let data: PromResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.status, "error");
        assert_eq!(data.error.as_deref(), Some("bad query"));
    }

    #[test]
    fn scalar_value_handles_number_and_string() {
        assert_eq!(scalar_value(&serde_json::json!(5.0)), 5.0);
        assert_eq!(scalar_value(&serde_json::json!("5.0")), 5.0);
    }

    #[test]
    fn urlencoding_encode_basic_query() {
        let encoded = urlencoding_encode("sum(x)");
        assert_eq!(encoded, "sum%28x%29");
    }
}
