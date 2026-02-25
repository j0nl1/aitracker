use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const GRAPHQL_URL: &str = "https://app.warp.dev/graphql/v2?op=GetRequestLimitInfo";

const GRAPHQL_QUERY: &str =
    "query GetRequestLimitInfo { requestLimitInfo { used limit resetAt } bonusGrants { remaining } }";

#[derive(Deserialize)]
struct RequestLimitInfo {
    used: Option<u64>,
    limit: Option<u64>,
    #[serde(rename = "resetAt")]
    reset_at: Option<String>,
}

#[derive(Deserialize)]
struct BonusGrant {
    remaining: Option<f64>,
}

#[derive(Deserialize)]
struct WarpData {
    #[serde(rename = "requestLimitInfo")]
    request_limit_info: Option<RequestLimitInfo>,
    #[serde(rename = "bonusGrants")]
    bonus_grants: Option<Vec<BonusGrant>>,
}

#[derive(Deserialize)]
struct WarpGraphQLResponse {
    data: Option<WarpData>,
}

#[derive(serde::Serialize)]
struct GraphQLRequest {
    #[serde(rename = "operationName")]
    operation_name: &'static str,
    query: &'static str,
    variables: serde_json::Value,
}

fn parse_request_limit(info: &RequestLimitInfo) -> Option<RateWindow> {
    let limit = info.limit?;
    let used = info.used.unwrap_or(0);

    if limit == 0 {
        return None;
    }

    let used_percent = (used as f64 / limit as f64 * 100.0).min(100.0);
    let resets_at = info
        .reset_at
        .as_deref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    Some(RateWindow {
        used_percent,
        window_minutes: 0,
        resets_at,
        reset_description: None,
    })
}

fn sum_bonus_grants(grants: &[BonusGrant]) -> f64 {
    grants
        .iter()
        .filter_map(|g| g.remaining)
        .sum()
}

/// Fetch usage data from the Warp GraphQL API.
pub async fn fetch() -> Result<FetchResult> {
    let token = std::env::var("WARP_TOKEN").context("WARP_TOKEN env var not set")?;

    if token.is_empty() {
        anyhow::bail!("WARP_TOKEN is empty");
    }

    let body = GraphQLRequest {
        operation_name: "GetRequestLimitInfo",
        query: GRAPHQL_QUERY,
        variables: serde_json::json!({}),
    };

    let client = reqwest::Client::new();
    let response = client
        .post(GRAPHQL_URL)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "Warp/1.0")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to send request to Warp API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized - check your WARP_TOKEN");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: WarpGraphQLResponse = response
        .json()
        .await
        .context("Failed to parse Warp GraphQL response")?;

    let warp_data = data.data.context("Missing 'data' field in Warp response")?;

    let primary = warp_data
        .request_limit_info
        .as_ref()
        .and_then(parse_request_limit);

    let bonus_total = warp_data
        .bonus_grants
        .as_deref()
        .map(sum_bonus_grants)
        .unwrap_or(0.0);

    let credits = Some(CreditsSnapshot {
        remaining: bonus_total,
        has_credits: bonus_total > 0.0,
        unlimited: false,
        used: None,
        limit: None,
        currency: None,
        period: None,
    });

    let usage = UsageSnapshot {
        provider: Provider::Warp,
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
    fn deserialize_full_response() {
        let json = r#"{
            "data": {
                "requestLimitInfo": {
                    "used": 42,
                    "limit": 100,
                    "resetAt": "2025-12-01T00:00:00Z"
                },
                "bonusGrants": [
                    { "remaining": 10.0 },
                    { "remaining": 5.5 }
                ]
            }
        }"#;
        let resp: WarpGraphQLResponse = serde_json::from_str(json).unwrap();
        let data = resp.data.unwrap();
        let info = data.request_limit_info.unwrap();
        assert_eq!(info.used, Some(42));
        assert_eq!(info.limit, Some(100));
        assert_eq!(info.reset_at.as_deref(), Some("2025-12-01T00:00:00Z"));
        let grants = data.bonus_grants.unwrap();
        assert_eq!(grants.len(), 2);
        assert!((grants[0].remaining.unwrap() - 10.0).abs() < 1e-10);
        assert!((grants[1].remaining.unwrap() - 5.5).abs() < 1e-10);
    }

    #[test]
    fn deserialize_partial_response_no_grants() {
        let json = r#"{
            "data": {
                "requestLimitInfo": {
                    "used": 10,
                    "limit": 50,
                    "resetAt": null
                }
            }
        }"#;
        let resp: WarpGraphQLResponse = serde_json::from_str(json).unwrap();
        let data = resp.data.unwrap();
        assert!(data.request_limit_info.is_some());
        assert!(data.bonus_grants.is_none());
    }

    #[test]
    fn deserialize_empty_data() {
        let json = r#"{ "data": {} }"#;
        let resp: WarpGraphQLResponse = serde_json::from_str(json).unwrap();
        let data = resp.data.unwrap();
        assert!(data.request_limit_info.is_none());
        assert!(data.bonus_grants.is_none());
    }

    #[test]
    fn deserialize_null_data() {
        let json = r#"{ "data": null }"#;
        let resp: WarpGraphQLResponse = serde_json::from_str(json).unwrap();
        assert!(resp.data.is_none());
    }

    #[test]
    fn parse_request_limit_calculates_percent() {
        let info = RequestLimitInfo {
            used: Some(42),
            limit: Some(100),
            reset_at: Some("2025-12-01T00:00:00Z".to_string()),
        };
        let window = parse_request_limit(&info).unwrap();
        assert!((window.used_percent - 42.0).abs() < 1e-10);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn parse_request_limit_no_limit_returns_none() {
        let info = RequestLimitInfo {
            used: Some(10),
            limit: None,
            reset_at: None,
        };
        assert!(parse_request_limit(&info).is_none());
    }

    #[test]
    fn parse_request_limit_zero_limit_returns_none() {
        let info = RequestLimitInfo {
            used: Some(0),
            limit: Some(0),
            reset_at: None,
        };
        assert!(parse_request_limit(&info).is_none());
    }

    #[test]
    fn parse_request_limit_caps_at_100_percent() {
        let info = RequestLimitInfo {
            used: Some(150),
            limit: Some(100),
            reset_at: None,
        };
        let window = parse_request_limit(&info).unwrap();
        assert!((window.used_percent - 100.0).abs() < 1e-10);
    }

    #[test]
    fn parse_request_limit_invalid_reset_date() {
        let info = RequestLimitInfo {
            used: Some(5),
            limit: Some(50),
            reset_at: Some("not-a-date".to_string()),
        };
        let window = parse_request_limit(&info).unwrap();
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn sum_bonus_grants_multiple() {
        let grants = vec![
            BonusGrant { remaining: Some(10.0) },
            BonusGrant { remaining: Some(5.5) },
            BonusGrant { remaining: Some(2.0) },
        ];
        let total = sum_bonus_grants(&grants);
        assert!((total - 17.5).abs() < 1e-10);
    }

    #[test]
    fn sum_bonus_grants_empty() {
        let grants: Vec<BonusGrant> = vec![];
        let total = sum_bonus_grants(&grants);
        assert!((total - 0.0).abs() < 1e-10);
    }

    #[test]
    fn sum_bonus_grants_with_none() {
        let grants = vec![
            BonusGrant { remaining: Some(10.0) },
            BonusGrant { remaining: None },
            BonusGrant { remaining: Some(3.0) },
        ];
        let total = sum_bonus_grants(&grants);
        assert!((total - 13.0).abs() < 1e-10);
    }

    #[test]
    fn graphql_request_serializes_correctly() {
        let req = GraphQLRequest {
            operation_name: "GetRequestLimitInfo",
            query: GRAPHQL_QUERY,
            variables: serde_json::json!({}),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["operationName"], "GetRequestLimitInfo");
        assert!(json["query"].as_str().unwrap().contains("requestLimitInfo"));
        assert_eq!(json["variables"], serde_json::json!({}));
    }
}
