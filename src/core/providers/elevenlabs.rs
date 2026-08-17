use anyhow::{Context, Result};
use chrono::TimeZone;
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct SubscriptionResponse {
    tier: Option<String>,
    character_count: i64,
    character_limit: i64,
    status: Option<String>,
    next_character_count_reset_unix: Option<i64>,
}

fn resolve_api_key() -> Result<String> {
    std::env::var("ELEVENLABS_API_KEY")
        .or_else(|_| std::env::var("XI_API_KEY"))
        .context("ELEVENLABS_API_KEY env var not set")
}

fn resolve_url() -> Result<String> {
    let base = std::env::var("ELEVENLABS_API_URL")
        .unwrap_or_else(|_| "https://api.elevenlabs.io/v1".to_string());
    let base = base.trim_end_matches('/');
    let url = if base.ends_with("/v1") {
        format!("{}/user/subscription", base)
    } else {
        format!("{}/v1/user/subscription", base)
    };
    validate_endpoint(&url, "ElevenLabs")?;
    Ok(url)
}

/// Fetch character credits and voice slot usage from the ElevenLabs API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = resolve_api_key()?;
    let url = resolve_url()?;

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("xi-api-key", api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to ElevenLabs API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Missing or invalid credentials — check ELEVENLABS_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: SubscriptionResponse = response
        .json()
        .await
        .context("Failed to parse ElevenLabs subscription response")?;

    let primary = if data.character_limit > 0 {
        let used_percent = (data.character_count as f64 / data.character_limit as f64 * 100.0)
            .clamp(0.0, 100.0);
        let resets_at = data
            .next_character_count_reset_unix
            .and_then(|ts| chrono::Utc.timestamp_opt(ts, 0).single());
        Some(RateWindow {
            used_percent,
            window_minutes: 0,
            resets_at,
            reset_description: None,
        })
    } else {
        None
    };

    let plan = data.tier.map(|t| {
        let readable = t.replace('_', " ");
        let capitalized = readable
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        match &data.status {
            Some(s) if s != "active" => format!("{} · {}", capitalized, s),
            _ => capitalized,
        }
    });

    let identity = plan.map(|p| crate::core::models::usage::ProviderIdentity {
        email: None,
        organization: None,
        plan: Some(p),
    });

    let usage = UsageSnapshot {
        provider: Provider::ElevenLabs,
        source: "api".to_string(),
        primary,
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
    fn deserialize_full_response() {
        let json = r#"{
            "tier": "creator",
            "character_count": 25000,
            "character_limit": 100000,
            "voice_slots_used": 2,
            "voice_limit": 10,
            "status": "active",
            "next_character_count_reset_unix": 1738356858
        }"#;
        let data: SubscriptionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.character_count, 25000);
        assert_eq!(data.character_limit, 100000);
    }

    #[test]
    fn deserialize_minimal_response() {
        let json = r#"{ "tier": "free", "character_count": 100, "character_limit": 10000, "status": "active" }"#;
        let data: SubscriptionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.tier.as_deref(), Some("free"));
    }

    #[test]
    fn resolve_url_appends_v1() {
        std::env::remove_var("ELEVENLABS_API_URL");
        let url = resolve_url().unwrap();
        assert_eq!(url, "https://api.elevenlabs.io/v1/user/subscription");
    }
}
