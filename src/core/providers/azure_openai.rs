use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::usage::{ProviderIdentity, UsageSnapshot};
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

fn default_api_version() -> String {
    "2024-10-21".to_string()
}

#[derive(Deserialize)]
struct ChatCompletionProbe {
    model: Option<String>,
}

/// Azure OpenAI exposes no billing/credit API. This performs a minimal,
/// bounded (1-token) chat completion probe to confirm the deployment is
/// reachable and report which model backs it — no usage/credit data exists.
pub async fn fetch() -> Result<FetchResult> {
    let api_key =
        std::env::var("AZURE_OPENAI_API_KEY").context("AZURE_OPENAI_API_KEY env var not set")?;
    let endpoint =
        std::env::var("AZURE_OPENAI_ENDPOINT").context("AZURE_OPENAI_ENDPOINT env var not set")?;
    let deployment = std::env::var("AZURE_OPENAI_DEPLOYMENT_NAME")
        .context("AZURE_OPENAI_DEPLOYMENT_NAME env var not set")?;
    let api_version =
        std::env::var("AZURE_OPENAI_API_VERSION").unwrap_or_else(|_| default_api_version());

    let endpoint = endpoint.trim_end_matches('/');
    let url = format!(
        "{}/openai/deployments/{}/chat/completions?api-version={}",
        endpoint, deployment, api_version
    );
    validate_endpoint(&url, "Azure OpenAI")?;

    let body = serde_json::json!({
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1,
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("api-key", api_key)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to send request to Azure OpenAI")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — check AZURE_OPENAI_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: ChatCompletionProbe = response
        .json()
        .await
        .context("Failed to parse Azure OpenAI response")?;

    let identity = Some(ProviderIdentity {
        email: None,
        organization: None,
        plan: data.model.or(Some(deployment)),
    });

    let usage = UsageSnapshot {
        provider: Provider::AzureOpenAi,
        source: "api".to_string(),
        primary: None,
        secondary: None,
        tertiary: None,
        identity,
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
    fn deserialize_probe_response() {
        let json = r#"{"model": "gpt-4o-2024-08-06"}"#;
        let data: ChatCompletionProbe = serde_json::from_str(json).unwrap();
        assert_eq!(data.model.as_deref(), Some("gpt-4o-2024-08-06"));
    }

    #[test]
    fn deserialize_probe_response_no_model() {
        let json = r#"{}"#;
        let data: ChatCompletionProbe = serde_json::from_str(json).unwrap();
        assert!(data.model.is_none());
    }

    #[test]
    fn default_api_version_is_set() {
        assert_eq!(default_api_version(), "2024-10-21");
    }
}
