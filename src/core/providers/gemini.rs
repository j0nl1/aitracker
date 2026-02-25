use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const QUOTA_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GEMINI_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const GEMINI_CLIENT_SECRET: &str = "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";

/// Safety margin (in ms) before actual expiry to trigger a refresh.
const EXPIRY_MARGIN_MS: u64 = 60_000;

// --- Credential files ---

#[derive(Deserialize, Serialize)]
struct GeminiOAuthCreds {
    access_token: String,
    refresh_token: Option<String>,
    expiry_date: Option<u64>, // Unix timestamp in milliseconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    expires_in: u64, // seconds
    #[allow(dead_code)]
    scope: Option<String>,
    #[allow(dead_code)]
    token_type: Option<String>,
}

fn gemini_oauth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".gemini")
        .join("oauth_creds.json")
}

fn is_expired(expiry_date: Option<u64>) -> bool {
    match expiry_date {
        Some(expiry_ms) => {
            let now_ms = Utc::now().timestamp_millis() as u64;
            now_ms + EXPIRY_MARGIN_MS >= expiry_ms
        }
        // No expiry info — assume expired to be safe.
        None => true,
    }
}

async fn refresh_access_token(creds: &mut GeminiOAuthCreds) -> Result<()> {
    let refresh_token = creds
        .refresh_token
        .as_deref()
        .context("No refresh_token in Gemini OAuth credentials — re-authenticate with Gemini CLI")?;

    let client = reqwest::Client::new();
    let response = client
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("client_id", GEMINI_CLIENT_ID),
            ("client_secret", GEMINI_CLIENT_SECRET),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .context("Failed to send token refresh request to Google")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Token refresh failed (HTTP {}): {}", status.as_u16(), body);
    }

    let token_resp: TokenRefreshResponse = response
        .json()
        .await
        .context("Failed to parse token refresh response")?;

    creds.access_token = token_resp.access_token;
    creds.expiry_date = Some(Utc::now().timestamp_millis() as u64 + token_resp.expires_in * 1000);

    // Write updated credentials back to disk.
    let path = gemini_oauth_path();
    let json = serde_json::to_string_pretty(creds)
        .context("Failed to serialize updated Gemini OAuth credentials")?;
    std::fs::write(&path, json)
        .with_context(|| format!("Failed to write updated credentials to {}", path.display()))?;

    Ok(())
}

async fn resolve_gemini_access_token() -> Result<String> {
    let path = gemini_oauth_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let mut creds: GeminiOAuthCreds =
        serde_json::from_str(&content).context("Failed to parse Gemini OAuth credentials")?;

    if creds.access_token.is_empty() && creds.refresh_token.is_none() {
        anyhow::bail!("Empty access_token and no refresh_token in Gemini OAuth credentials");
    }

    if creds.access_token.is_empty() || is_expired(creds.expiry_date) {
        refresh_access_token(&mut creds).await?;
    }

    Ok(creds.access_token)
}

// --- API response ---

#[derive(Deserialize)]
struct QuotaResponse {
    #[serde(default)]
    buckets: Vec<QuotaBucket>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaBucket {
    remaining_fraction: Option<f64>,
    reset_time: Option<String>,
    model_id: Option<String>,
}

fn is_pro_model(model_id: &str) -> bool {
    model_id.to_lowercase().contains("pro")
}

fn bucket_to_window(bucket: &QuotaBucket) -> RateWindow {
    let remaining_fraction = bucket.remaining_fraction.unwrap_or(1.0);
    let used_percent = (1.0 - remaining_fraction) * 100.0;

    // Gemini uses a rolling 24h window — the reset time is only meaningful
    // when some quota has actually been consumed.
    let resets_at = if used_percent > 0.0 {
        bucket
            .reset_time
            .as_deref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
    } else {
        None
    };

    RateWindow {
        used_percent,
        window_minutes: 0,
        resets_at,
        reset_description: None,
    }
}

/// Fetch usage data from the Gemini quota API.
pub async fn fetch() -> Result<FetchResult> {
    let token = resolve_gemini_access_token()
        .await
        .context("Gemini credentials not found — authenticate with Gemini CLI first")?;

    let client = reqwest::Client::new();
    let response = client
        .post(QUOTA_URL)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .context("Failed to send request to Gemini quota API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Unauthorized — re-authenticate with Gemini CLI");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: QuotaResponse = response
        .json()
        .await
        .context("Failed to parse Gemini quota response")?;

    // Separate Pro vs Flash buckets
    let mut pro_buckets: Vec<&QuotaBucket> = Vec::new();
    let mut flash_buckets: Vec<&QuotaBucket> = Vec::new();

    for bucket in &data.buckets {
        if let Some(id) = &bucket.model_id {
            if is_pro_model(id) {
                pro_buckets.push(bucket);
            } else {
                flash_buckets.push(bucket);
            }
        } else {
            flash_buckets.push(bucket);
        }
    }

    // Primary = most-used Pro bucket, Secondary = most-used Flash bucket
    let primary = pro_buckets.first().map(|b| bucket_to_window(b));
    let secondary = flash_buckets.first().map(|b| bucket_to_window(b));

    let usage = UsageSnapshot {
        provider: Provider::Gemini,
        source: "oauth".to_string(),
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
    fn deserialize_quota_response() {
        let json = r#"{
            "buckets": [
                {
                    "remainingFraction": 0.75,
                    "resetTime": "2026-02-25T00:00:00Z",
                    "modelId": "gemini-2.0-pro"
                },
                {
                    "remainingFraction": 0.90,
                    "resetTime": "2026-02-25T00:00:00Z",
                    "modelId": "gemini-2.0-flash"
                }
            ]
        }"#;
        let data: QuotaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.buckets.len(), 2);
        assert!((data.buckets[0].remaining_fraction.unwrap() - 0.75).abs() < f64::EPSILON);
        assert_eq!(data.buckets[0].model_id.as_deref(), Some("gemini-2.0-pro"));
    }

    #[test]
    fn deserialize_empty_buckets() {
        let json = r#"{ "buckets": [] }"#;
        let data: QuotaResponse = serde_json::from_str(json).unwrap();
        assert!(data.buckets.is_empty());
    }

    #[test]
    fn deserialize_missing_buckets() {
        let json = r#"{}"#;
        let data: QuotaResponse = serde_json::from_str(json).unwrap();
        assert!(data.buckets.is_empty());
    }

    #[test]
    fn bucket_to_window_calculates_used_percent() {
        let bucket = QuotaBucket {
            remaining_fraction: Some(0.75),
            reset_time: Some("2026-02-25T12:00:00Z".to_string()),
            model_id: Some("gemini-2.0-pro".to_string()),
        };
        let window = bucket_to_window(&bucket);
        assert!((window.used_percent - 25.0).abs() < 1e-10);
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn bucket_to_window_defaults_remaining_to_1() {
        let bucket = QuotaBucket {
            remaining_fraction: None,
            reset_time: None,
            model_id: None,
        };
        let window = bucket_to_window(&bucket);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn bucket_to_window_invalid_reset_time() {
        let bucket = QuotaBucket {
            remaining_fraction: Some(0.5),
            reset_time: Some("not-a-date".to_string()),
            model_id: None,
        };
        let window = bucket_to_window(&bucket);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn bucket_to_window_suppresses_reset_when_unused() {
        let bucket = QuotaBucket {
            remaining_fraction: Some(1.0),
            reset_time: Some("2026-02-26T01:11:09Z".to_string()),
            model_id: Some("gemini-2.5-pro".to_string()),
        };
        let window = bucket_to_window(&bucket);
        assert!((window.used_percent - 0.0).abs() < 1e-10);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn is_pro_model_matches() {
        assert!(is_pro_model("gemini-2.0-pro"));
        assert!(is_pro_model("gemini-pro-exp"));
        assert!(is_pro_model("GEMINI-PRO"));
        assert!(!is_pro_model("gemini-2.0-flash"));
        assert!(!is_pro_model("gemini-flash"));
    }

    #[test]
    fn pro_and_flash_separation() {
        let json = r#"{
            "buckets": [
                { "remainingFraction": 0.6, "modelId": "gemini-2.0-pro" },
                { "remainingFraction": 0.9, "modelId": "gemini-2.0-flash" },
                { "remainingFraction": 0.3, "modelId": "gemini-1.5-pro" }
            ]
        }"#;
        let data: QuotaResponse = serde_json::from_str(json).unwrap();

        let mut pro: Vec<&QuotaBucket> = Vec::new();
        let mut flash: Vec<&QuotaBucket> = Vec::new();
        for bucket in &data.buckets {
            if let Some(id) = &bucket.model_id {
                if is_pro_model(id) {
                    pro.push(bucket);
                } else {
                    flash.push(bucket);
                }
            }
        }
        assert_eq!(pro.len(), 2);
        assert_eq!(flash.len(), 1);
    }

    #[test]
    fn deserialize_oauth_creds_minimal() {
        let json = r#"{ "access_token": "ya29.abc123" }"#;
        let creds: GeminiOAuthCreds = serde_json::from_str(json).unwrap();
        assert_eq!(creds.access_token, "ya29.abc123");
        assert!(creds.refresh_token.is_none());
        assert!(creds.expiry_date.is_none());
    }

    #[test]
    fn deserialize_oauth_creds_full() {
        let json = r#"{
            "access_token": "ya29.abc123",
            "refresh_token": "1//refresh",
            "expiry_date": 1771968603809,
            "scope": "openid",
            "token_type": "Bearer",
            "id_token": "eyJ"
        }"#;
        let creds: GeminiOAuthCreds = serde_json::from_str(json).unwrap();
        assert_eq!(creds.access_token, "ya29.abc123");
        assert_eq!(creds.refresh_token.as_deref(), Some("1//refresh"));
        assert_eq!(creds.expiry_date, Some(1771968603809));
        assert_eq!(creds.scope.as_deref(), Some("openid"));
    }

    #[test]
    fn serialize_oauth_creds_roundtrip() {
        let creds = GeminiOAuthCreds {
            access_token: "ya29.new".to_string(),
            refresh_token: Some("1//ref".to_string()),
            expiry_date: Some(9999999999999),
            scope: Some("openid".to_string()),
            token_type: Some("Bearer".to_string()),
            id_token: None,
        };
        let json = serde_json::to_string(&creds).unwrap();
        let parsed: GeminiOAuthCreds = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "ya29.new");
        assert_eq!(parsed.refresh_token.as_deref(), Some("1//ref"));
        assert_eq!(parsed.expiry_date, Some(9999999999999));
        // id_token was None, should not appear in output
        assert!(!json.contains("id_token"));
    }

    #[test]
    fn is_expired_past_date() {
        // A date clearly in the past
        assert!(is_expired(Some(1_000_000_000_000)));
    }

    #[test]
    fn is_expired_future_date() {
        // A date far in the future (year ~2317)
        assert!(!is_expired(Some(10_999_999_999_999)));
    }

    #[test]
    fn is_expired_none_treated_as_expired() {
        assert!(is_expired(None));
    }

    #[test]
    fn deserialize_token_refresh_response() {
        let json = r#"{
            "access_token": "ya29.new_token",
            "expires_in": 3599,
            "scope": "openid",
            "token_type": "Bearer"
        }"#;
        let resp: TokenRefreshResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "ya29.new_token");
        assert_eq!(resp.expires_in, 3599);
    }
}
