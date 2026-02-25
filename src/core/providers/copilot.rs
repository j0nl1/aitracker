use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{ProviderIdentity, RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const USER_URL: &str = "https://api.github.com/copilot_internal/user";

#[derive(Deserialize)]
struct QuotaSnapshot {
    percent_remaining: Option<f64>,
    entitlement: Option<String>,
    remaining: Option<u64>,
}

#[derive(Deserialize)]
struct QuotaSnapshots {
    premium_interactions: Option<QuotaSnapshot>,
    chat: Option<QuotaSnapshot>,
}

#[derive(Deserialize)]
struct CopilotUserResponse {
    copilot_plan: Option<String>,
    quota_reset_date: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

fn parse_premium_window(snapshot: &QuotaSnapshot, reset_date: Option<&str>) -> RateWindow {
    let used_percent = snapshot
        .percent_remaining
        .map(|pr| (100.0 - pr).max(0.0))
        .unwrap_or(0.0);

    let resets_at = reset_date.and_then(|s| s.parse::<DateTime<Utc>>().ok());

    // Compute window_minutes from now until reset if possible
    let window_minutes = resets_at
        .map(|reset| {
            let now = Utc::now();
            if reset > now {
                let diff = reset - now;
                diff.num_minutes().max(0) as u64
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

fn parse_chat_window(snapshot: &QuotaSnapshot) -> RateWindow {
    let used_percent = snapshot
        .percent_remaining
        .map(|pr| (100.0 - pr).max(0.0))
        .unwrap_or(0.0);

    RateWindow {
        used_percent,
        window_minutes: 0,
        resets_at: None,
        reset_description: None,
    }
}

/// Resolve the GitHub token from config api_key, GITHUB_TOKEN env, or `gh auth token` command.
fn resolve_github_token() -> Result<String> {
    // Try GITHUB_TOKEN env first
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Try `gh auth token` command
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context("Failed to run `gh auth token` - is GitHub CLI installed?")?;

    if output.status.success() {
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    anyhow::bail!(
        "No GitHub token found. Set GITHUB_TOKEN env or authenticate with `gh auth login`"
    )
}

/// Fetch usage data from the GitHub Copilot API.
pub async fn fetch() -> Result<FetchResult> {
    let token = resolve_github_token().context("Failed to resolve GitHub token")?;

    let client = reqwest::Client::new();
    let response = client
        .get(USER_URL)
        .header("Authorization", format!("token {}", token))
        .header("Editor-Version", "vscode/1.96.2")
        .header("Editor-Plugin-Version", "copilot-chat/0.26.7")
        .header("User-Agent", "GitHubCopilotChat/0.26.7")
        .header("X-Github-Api-Version", "2025-04-01")
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to Copilot API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized - check your GitHub token or run `gh auth login`");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: CopilotUserResponse = response
        .json()
        .await
        .context("Failed to parse Copilot user response")?;

    let primary = data
        .quota_snapshots
        .as_ref()
        .and_then(|qs| qs.premium_interactions.as_ref())
        .map(|pi| parse_premium_window(pi, data.quota_reset_date.as_deref()));

    let secondary = data
        .quota_snapshots
        .as_ref()
        .and_then(|qs| qs.chat.as_ref())
        .map(|c| parse_chat_window(c));

    let identity = data.copilot_plan.map(|plan| ProviderIdentity {
        email: None,
        organization: None,
        plan: Some(plan),
    });

    let credits = data
        .quota_snapshots
        .as_ref()
        .and_then(|qs| qs.premium_interactions.as_ref())
        .and_then(|pi| {
            pi.remaining.map(|remaining| CreditsSnapshot {
                remaining: remaining as f64,
                has_credits: remaining > 0,
                unlimited: false,
                used: None,
                limit: pi
                    .entitlement
                    .as_ref()
                    .and_then(|e| e.parse::<f64>().ok()),
                currency: None,
                period: Some("Monthly".to_string()),
            })
        });

    let usage = UsageSnapshot {
        provider: Provider::Copilot,
        source: "api".to_string(),
        primary,
        secondary,
        tertiary: None,
        identity,
    };

    Ok(FetchResult { usage, credits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "copilot_plan": "business",
            "quota_reset_date": "2025-02-01T00:00:00Z",
            "quota_snapshots": {
                "premium_interactions": {
                    "percent_remaining": 72.5,
                    "entitlement": "300",
                    "remaining": 217
                },
                "chat": {
                    "percent_remaining": 90.0,
                    "entitlement": "1000",
                    "remaining": 900
                }
            }
        }"#;
        let data: CopilotUserResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.copilot_plan.as_deref(), Some("business"));
        assert_eq!(
            data.quota_reset_date.as_deref(),
            Some("2025-02-01T00:00:00Z")
        );
        let snapshots = data.quota_snapshots.unwrap();
        let premium = snapshots.premium_interactions.unwrap();
        assert!((premium.percent_remaining.unwrap() - 72.5).abs() < 1e-10);
        assert_eq!(premium.entitlement.as_deref(), Some("300"));
        assert_eq!(premium.remaining, Some(217));
        let chat = snapshots.chat.unwrap();
        assert!((chat.percent_remaining.unwrap() - 90.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_partial_response() {
        let json = r#"{ "copilot_plan": "individual" }"#;
        let data: CopilotUserResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.copilot_plan.as_deref(), Some("individual"));
        assert!(data.quota_snapshots.is_none());
        assert!(data.quota_reset_date.is_none());
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: CopilotUserResponse = serde_json::from_str(json).unwrap();
        assert!(data.copilot_plan.is_none());
        assert!(data.quota_snapshots.is_none());
    }

    #[test]
    fn parse_premium_window_calculates_used_percent() {
        let snapshot = QuotaSnapshot {
            percent_remaining: Some(72.5),
            entitlement: Some("300".to_string()),
            remaining: Some(217),
        };
        let window = parse_premium_window(&snapshot, Some("2099-12-31T23:59:59Z"));
        assert!((window.used_percent - 27.5).abs() < 1e-10);
        assert!(window.resets_at.is_some());
        assert!(window.window_minutes > 0);
    }

    #[test]
    fn parse_premium_window_no_reset_date() {
        let snapshot = QuotaSnapshot {
            percent_remaining: Some(50.0),
            entitlement: None,
            remaining: None,
        };
        let window = parse_premium_window(&snapshot, None);
        assert!((window.used_percent - 50.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
        assert_eq!(window.window_minutes, 0);
    }

    #[test]
    fn parse_premium_window_invalid_date() {
        let snapshot = QuotaSnapshot {
            percent_remaining: Some(10.0),
            entitlement: None,
            remaining: None,
        };
        let window = parse_premium_window(&snapshot, Some("not-a-date"));
        assert!(window.resets_at.is_none());
        assert_eq!(window.window_minutes, 0);
    }

    #[test]
    fn parse_chat_window_calculates_used_percent() {
        let snapshot = QuotaSnapshot {
            percent_remaining: Some(90.0),
            entitlement: Some("1000".to_string()),
            remaining: Some(900),
        };
        let window = parse_chat_window(&snapshot);
        assert!((window.used_percent - 10.0).abs() < 1e-10);
        assert_eq!(window.window_minutes, 0);
    }

    #[test]
    fn parse_premium_window_zero_remaining() {
        let snapshot = QuotaSnapshot {
            percent_remaining: Some(0.0),
            entitlement: Some("300".to_string()),
            remaining: Some(0),
        };
        let window = parse_premium_window(&snapshot, None);
        assert!((window.used_percent - 100.0).abs() < 1e-10);
    }

    #[test]
    fn deserialize_response_with_only_premium() {
        let json = r#"{
            "copilot_plan": "enterprise",
            "quota_reset_date": "2025-03-01T00:00:00Z",
            "quota_snapshots": {
                "premium_interactions": {
                    "percent_remaining": 100.0,
                    "entitlement": "500",
                    "remaining": 500
                }
            }
        }"#;
        let data: CopilotUserResponse = serde_json::from_str(json).unwrap();
        let snapshots = data.quota_snapshots.unwrap();
        assert!(snapshots.premium_interactions.is_some());
        assert!(snapshots.chat.is_none());
    }
}
