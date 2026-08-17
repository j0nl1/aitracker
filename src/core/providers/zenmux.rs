use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const SUBSCRIPTION_URL: &str = "https://zenmux.ai/api/v1/management/subscription/detail";
const PAYG_URL: &str = "https://zenmux.ai/api/v1/management/payg/balance";

#[derive(Deserialize)]
struct Plan {
    tier: String,
}

#[derive(Deserialize)]
struct QuotaWindow {
    usage_percentage: f64,
    resets_at: Option<String>,
    max_flows: f64,
    used_flows: f64,
}

#[derive(Deserialize)]
struct SubscriptionData {
    plan: Plan,
    account_status: String,
    quota_5_hour: Option<QuotaWindow>,
    quota_7_day: Option<QuotaWindow>,
}

#[derive(Deserialize)]
struct SubscriptionEnvelope {
    success: bool,
    data: Option<SubscriptionData>,
}

#[derive(Deserialize)]
struct PaygData {
    currency: String,
    total_credits: f64,
}

#[derive(Deserialize)]
struct PaygEnvelope {
    success: bool,
    data: Option<PaygData>,
}

fn parse_window(w: &QuotaWindow, minutes: u64) -> RateWindow {
    let resets_at = w
        .resets_at
        .as_deref()
        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());
    RateWindow {
        used_percent: (w.usage_percentage * 100.0).clamp(0.0, 100.0),
        window_minutes: minutes,
        resets_at,
        reset_description: Some(format!("{:.0}/{:.0} flows", w.used_flows, w.max_flows)),
    }
}

/// Fetch subscription quota windows and PAYG balance from ZenMux's Management API.
pub async fn fetch() -> Result<FetchResult> {
    let api_key = std::env::var("ZENMUX_MANAGEMENT_API_KEY")
        .context("ZENMUX_MANAGEMENT_API_KEY env var not set")?;

    let client = reqwest::Client::new();
    let response = client
        .get(SUBSCRIPTION_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to send request to ZenMux API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Authentication rejected — check ZENMUX_MANAGEMENT_API_KEY (must be a Management API key)");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let envelope: SubscriptionEnvelope = response
        .json()
        .await
        .context("Failed to parse ZenMux subscription response")?;

    if !envelope.success {
        anyhow::bail!("ZenMux subscription response reported failure");
    }
    let sub_data = envelope
        .data
        .context("ZenMux subscription response missing data")?;

    let primary = sub_data.quota_5_hour.as_ref().map(|w| parse_window(w, 300));
    let secondary = sub_data.quota_7_day.as_ref().map(|w| parse_window(w, 10080));

    let identity = Some(crate::core::models::usage::ProviderIdentity {
        email: None,
        organization: None,
        plan: Some(format!(
            "{} plan · {}",
            capitalize(&sub_data.plan.tier),
            capitalize(&sub_data.account_status)
        )),
    });

    let credits = match client
        .get(PAYG_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.json::<PaygEnvelope>().await {
            Ok(payg) if payg.success => payg.data.map(|d| CreditsSnapshot {
                remaining: d.total_credits.max(0.0),
                has_credits: d.total_credits > 0.0,
                unlimited: false,
                used: None,
                limit: None,
                currency: Some(d.currency.to_lowercase()),
                period: None,
            }),
            _ => None,
        },
        _ => None,
    };

    let usage = UsageSnapshot {
        provider: Provider::ZenMux,
        source: "api".to_string(),
        primary,
        secondary,
        tertiary: None,
        identity,
    };

    Ok(FetchResult { usage, credits })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_subscription_response() {
        let json = r#"{
            "success": true,
            "data": {
                "plan": { "tier": "ultra" },
                "account_status": "healthy",
                "quota_5_hour": { "usage_percentage": 0.0715, "resets_at": "2026-03-24T08:35:09.000Z", "max_flows": 800, "used_flows": 57.2 },
                "quota_7_day": { "usage_percentage": 0.0673, "resets_at": "2026-03-26T02:15:05.000Z", "max_flows": 6182, "used_flows": 416.11 }
            }
        }"#;
        let data: SubscriptionEnvelope = serde_json::from_str(json).unwrap();
        assert!(data.success);
        assert_eq!(data.data.unwrap().plan.tier, "ultra");
    }

    #[test]
    fn deserialize_payg_response() {
        let json = r#"{ "success": true, "data": { "currency": "usd", "total_credits": 482.74, "top_up_credits": 35, "bonus_credits": 447.74 } }"#;
        let data: PaygEnvelope = serde_json::from_str(json).unwrap();
        assert!((data.data.unwrap().total_credits - 482.74).abs() < 1e-9);
    }

    #[test]
    fn parse_window_converts_fraction_to_percent() {
        let w = QuotaWindow {
            usage_percentage: 0.5,
            resets_at: None,
            max_flows: 100.0,
            used_flows: 50.0,
        };
        let window = parse_window(&w, 300);
        assert!((window.used_percent - 50.0).abs() < 1e-9);
    }

    #[test]
    fn capitalize_first_letter() {
        assert_eq!(capitalize("ultra"), "Ultra");
        assert_eq!(capitalize(""), "");
    }
}
