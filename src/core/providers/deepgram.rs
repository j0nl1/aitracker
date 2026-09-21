use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct Project {
    project_id: String,
}

#[derive(Deserialize)]
struct ProjectsResponse {
    #[serde(default)]
    projects: Vec<Project>,
}

#[derive(Deserialize, Default)]
struct UsageResult {
    #[serde(default)]
    hours: f64,
    #[serde(default)]
    tokens_in: u64,
    #[serde(default)]
    tokens_out: u64,
    #[serde(default)]
    tts_characters: u64,
    #[serde(default)]
    requests: u64,
}

#[derive(Deserialize)]
struct BreakdownResponse {
    #[serde(default)]
    results: Vec<UsageResult>,
}

fn resolve_base_url() -> Result<String> {
    let base = std::env::var("DEEPGRAM_API_URL")
        .unwrap_or_else(|_| "https://api.deepgram.com/v1".to_string());
    let base = base.trim_end_matches('/').to_string();
    validate_endpoint(&format!("{}/", base), "Deepgram")?;
    Ok(base)
}

/// Fetch aggregate usage (audio hours, tokens, TTS characters, requests)
/// from the Deepgram Management API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("DEEPGRAM_API_KEY").context("DEEPGRAM_API_KEY env var not set")?;
    let base = resolve_base_url()?;

    let client = reqwest::Client::new();

    let project_ids: Vec<String> = if let Ok(id) = std::env::var("DEEPGRAM_PROJECT_ID") {
        vec![id]
    } else {
        let resp = client
            .get(format!("{}/projects", base))
            .header("Authorization", format!("Token {}", api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to Deepgram /projects")?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("Unauthorized — check DEEPGRAM_API_KEY");
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!("Forbidden — key lacks access to the Management API");
        }
        if !status.is_success() {
            anyhow::bail!("HTTP {} from /projects", status.as_u16());
        }
        let data: ProjectsResponse = resp
            .json()
            .await
            .context("Failed to parse Deepgram /projects response")?;
        data.projects.into_iter().map(|p| p.project_id).collect()
    };

    let mut total_hours = 0.0;
    let mut total_tokens_in = 0u64;
    let mut total_tokens_out = 0u64;
    let mut total_tts_characters = 0u64;
    let mut total_requests = 0u64;

    for project_id in &project_ids {
        let resp = client
            .get(format!("{}/projects/{}/usage/breakdown", base, project_id))
            .header("Authorization", format!("Token {}", api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to Deepgram usage breakdown")?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("Authentication expired — check DEEPGRAM_API_KEY");
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            anyhow::bail!("Rate limited by Deepgram");
        }
        if !status.is_success() {
            anyhow::bail!("HTTP {} from usage breakdown", status.as_u16());
        }
        let data: BreakdownResponse = resp
            .json()
            .await
            .context("Failed to parse Deepgram usage breakdown response")?;
        for result in data.results {
            total_hours += result.hours;
            total_tokens_in += result.tokens_in;
            total_tokens_out += result.tokens_out;
            total_tts_characters += result.tts_characters;
            total_requests += result.requests;
        }
    }

    let identity = Some(crate::core::models::usage::ProviderIdentity {
        email: None,
        organization: None,
        plan: Some(format!(
            "{:.1}h, {} tok, {} tts chars, {} reqs",
            total_hours,
            total_tokens_in + total_tokens_out,
            total_tts_characters,
            total_requests
        )),
    });

    let usage = UsageSnapshot {
        provider: Provider::Deepgram,
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
    fn deserialize_projects_response() {
        let json = r#"{ "projects": [ { "project_id": "abc" } ] }"#;
        let data: ProjectsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.projects.len(), 1);
        assert_eq!(data.projects[0].project_id, "abc");
    }

    #[test]
    fn deserialize_breakdown_response() {
        let json = r#"{
            "results": [
                { "hours": 1.5, "total_hours": 1.5, "agent_hours": 0, "tokens_in": 100, "tokens_out": 50, "tts_characters": 20, "requests": 3 }
            ]
        }"#;
        let data: BreakdownResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.results.len(), 1);
        assert_eq!(data.results[0].tokens_in, 100);
    }

    #[test]
    fn deserialize_breakdown_missing_fields_default_zero() {
        let json = r#"{ "results": [{}] }"#;
        let data: BreakdownResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.results[0].hours, 0.0);
        assert_eq!(data.results[0].requests, 0);
    }
}
