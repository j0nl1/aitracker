use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::{validate_endpoint, FetchResult};
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct KeyInfo {
    user_id: Option<String>,
    team_id: Option<String>,
}

#[derive(Deserialize)]
struct KeyInfoResponse {
    info: KeyInfo,
}

#[derive(Deserialize)]
struct UserInfo {
    max_budget: Option<f64>,
    spend: Option<f64>,
}

#[derive(Deserialize)]
struct UserInfoResponse {
    user_info: Option<UserInfo>,
}

#[derive(Deserialize)]
struct TeamInfo {
    max_budget: Option<f64>,
    spend: Option<f64>,
}

#[derive(Deserialize)]
struct TeamInfoResponse {
    team_info: Option<TeamInfo>,
}

fn management_base_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_suffix("/v1") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

fn info_url(base: &str, resource: &str, id: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&format!("{}/{}/info", base, resource))
        .context("Failed to construct LiteLLM info URL")?;
    url.query_pairs_mut()
        .append_pair(&format!("{}_id", resource), id);
    Ok(url)
}

fn budget_window(spend: Option<f64>, budget: Option<f64>) -> Option<RateWindow> {
    let budget = budget.filter(|b| *b > 0.0)?;
    let spend = spend.unwrap_or(0.0);
    Some(RateWindow {
        used_percent: (spend / budget * 100.0).clamp(0.0, 100.0),
        window_minutes: 0,
        resets_at: None,
        reset_description: None,
    })
}

/// Fetch personal/team budget and spend from a self-hosted LiteLLM proxy.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("LITELLM_API_KEY").context("LITELLM_API_KEY env var not set")?;
    let base_url =
        std::env::var("LITELLM_BASE_URL").context("LITELLM_BASE_URL env var not set")?;
    let base = management_base_url(&base_url);
    validate_endpoint(&format!("{}/", base), "LiteLLM")?;

    let client = reqwest::Client::new();

    let key_info_resp = client
        .get(format!("{}/key/info", base))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to LiteLLM /key/info")?;

    let status = key_info_resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Unauthorized — check LITELLM_API_KEY");
    }
    if !status.is_success() {
        let body = key_info_resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let key_info: KeyInfoResponse = key_info_resp
        .json()
        .await
        .context("Failed to parse LiteLLM /key/info response")?;

    let primary = if let Some(user_id) = key_info.info.user_id {
        let resp = client
            .get(info_url(&base, "user", &user_id)?)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to LiteLLM /user/info")?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} from /user/info", resp.status().as_u16());
        }
        let data: UserInfoResponse = resp
            .json()
            .await
            .context("Failed to parse LiteLLM /user/info response")?;
        data.user_info
            .and_then(|u| budget_window(u.spend, u.max_budget))
    } else if let Some(team_id) = key_info.info.team_id {
        let resp = client
            .get(info_url(&base, "team", &team_id)?)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to LiteLLM /team/info")?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} from /team/info", resp.status().as_u16());
        }
        let data: TeamInfoResponse = resp
            .json()
            .await
            .context("Failed to parse LiteLLM /team/info response")?;
        data.team_info
            .and_then(|t| budget_window(t.spend, t.max_budget))
    } else {
        anyhow::bail!("LiteLLM key has neither user_id nor team_id");
    };

    let usage = UsageSnapshot {
        provider: Provider::LiteLlm,
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
    fn info_urls_preserve_user_and_team_ids_as_single_query_values() {
        let id = "name+tag@example.com&other=value#section /?%";
        for resource in ["user", "team"] {
            let url = info_url("https://gw.example.com/litellm", resource, id).unwrap();

            assert_eq!(url.path(), format!("/litellm/{}/info", resource));
            assert_eq!(url.fragment(), None);
            assert_eq!(
                url.query_pairs().collect::<Vec<_>>(),
                [(format!("{}_id", resource).into(), id.into())]
            );
        }
    }

    #[test]
    fn deserialize_key_info() {
        let json = r#"{ "info": { "key_name": "x", "spend": 1.0, "user_id": "u1", "team_id": "t1" } }"#;
        let data: KeyInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.info.user_id.as_deref(), Some("u1"));
    }

    #[test]
    fn management_base_url_strips_v1() {
        assert_eq!(
            management_base_url("https://gw.example.com/litellm/v1"),
            "https://gw.example.com/litellm"
        );
    }

    #[test]
    fn management_base_url_leaves_root() {
        assert_eq!(
            management_base_url("https://gw.example.com"),
            "https://gw.example.com"
        );
    }

    #[test]
    fn budget_window_computes_percent() {
        let window = budget_window(Some(25.0), Some(100.0)).unwrap();
        assert!((window.used_percent - 25.0).abs() < 1e-9);
    }

    #[test]
    fn budget_window_none_without_budget() {
        assert!(budget_window(Some(25.0), None).is_none());
    }
}
