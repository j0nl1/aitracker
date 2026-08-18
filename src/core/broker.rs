use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use subtle::ConstantTimeEq;

use crate::core::models::usage::RateWindow;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrokerWindow {
    pub name: String,
    pub remaining_percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
    pub window_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrokerCredits {
    pub remaining: f64,
    pub unlimited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrokerProviderUsage {
    pub provider: String,
    pub available: bool,
    pub observed_at: DateTime<Utc>,
    pub windows: Vec<BrokerWindow>,
    pub credits: Option<BrokerCredits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrokerResponse {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub age_seconds: u64,
    pub stale: bool,
    pub providers: Vec<BrokerProviderUsage>,
}

pub fn build_response(
    providers: Vec<BrokerProviderUsage>,
    generated_at: DateTime<Utc>,
    now: DateTime<Utc>,
    max_age_seconds: u64,
) -> BrokerResponse {
    let age_seconds = (now - generated_at).num_seconds().max(0) as u64;
    BrokerResponse {
        schema_version: 1,
        generated_at,
        age_seconds,
        stale: age_seconds > max_age_seconds,
        providers,
    }
}

pub fn authorize_bearer(header: Option<&str>, expected_token: &str) -> bool {
    let Some(provided_token) = header.and_then(|value| value.strip_prefix("Bearer ")) else {
        return false;
    };
    provided_token
        .as_bytes()
        .ct_eq(expected_token.as_bytes())
        .into()
}

pub fn validate_broker_token(token: &str) -> anyhow::Result<()> {
    if token.len() < 32 {
        anyhow::bail!("AIT_BROKER_TOKEN must contain at least 32 bytes");
    }
    Ok(())
}

pub fn parse_loopback_bind(bind: &str) -> anyhow::Result<SocketAddr> {
    let address: SocketAddr = bind
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid broker bind address: {}", bind))?;
    if !address.ip().is_loopback() {
        anyhow::bail!(
            "Broker must bind to a loopback address, got {}",
            address.ip()
        );
    }
    Ok(address)
}

pub fn parse_loopback_url(url: &str) -> anyhow::Result<reqwest::Url> {
    let mut parsed =
        reqwest::Url::parse(url).map_err(|_| anyhow::anyhow!("Invalid broker URL: {}", url))?;
    if parsed.scheme() != "http" {
        anyhow::bail!("Broker client URL must use HTTP over loopback");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("Broker client URL must not contain credentials");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        anyhow::bail!("Broker client URL must not contain a query or fragment");
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        anyhow::bail!("Broker client URL must not contain a path");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Broker client URL is missing a host"))?
        .trim_matches(['[', ']']);
    let address: std::net::IpAddr = host
        .parse()
        .map_err(|_| anyhow::anyhow!("Broker client URL host must be a loopback IP address"))?;
    if !address.is_loopback() {
        anyhow::bail!("Broker client URL host must be a loopback IP address");
    }
    parsed.set_path("/");
    Ok(parsed)
}

fn sanitize_window(name: &str, window: RateWindow) -> BrokerWindow {
    BrokerWindow {
        name: name.to_string(),
        remaining_percent: (100.0 - window.used_percent).clamp(0.0, 100.0),
        resets_at: window.resets_at,
        window_minutes: window.window_minutes,
    }
}

pub fn sanitize_success(
    provider: Provider,
    result: FetchResult,
    observed_at: DateTime<Utc>,
) -> BrokerProviderUsage {
    let mut windows = Vec::with_capacity(3);
    if let Some(window) = result.usage.primary {
        windows.push(sanitize_window("primary", window));
    }
    if let Some(window) = result.usage.secondary {
        windows.push(sanitize_window("secondary", window));
    }
    if let Some(window) = result.usage.tertiary {
        windows.push(sanitize_window("tertiary", window));
    }
    let credits = result.credits.map(|credits| BrokerCredits {
        remaining: credits.remaining,
        unlimited: credits.unlimited,
    });

    BrokerProviderUsage {
        provider: provider.id().to_string(),
        available: true,
        observed_at,
        windows,
        credits,
        error_code: None,
    }
}

pub fn sanitize_failure(provider: Provider, observed_at: DateTime<Utc>) -> BrokerProviderUsage {
    BrokerProviderUsage {
        provider: provider.id().to_string(),
        available: false,
        observed_at,
        windows: Vec::new(),
        credits: None,
        error_code: Some("collection_failed".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::credits::CreditsSnapshot;
    use crate::core::models::usage::{ProviderIdentity, RateWindow, UsageSnapshot};

    #[test]
    fn bearer_auth_requires_exact_token() {
        let token = "0123456789abcdef0123456789abcdef";
        assert!(authorize_bearer(Some(&format!("Bearer {}", token)), token));
        assert!(!authorize_bearer(None, token));
        assert!(!authorize_bearer(Some("Bearer wrong"), token));
        assert!(!authorize_bearer(Some(token), token));
    }

    #[test]
    fn broker_token_must_be_at_least_32_bytes() {
        assert!(validate_broker_token("0123456789abcdef0123456789abcdef").is_ok());
        assert!(validate_broker_token("too-short").is_err());
        assert!(validate_broker_token("").is_err());
    }

    #[test]
    fn broker_bind_must_be_loopback() {
        assert!(parse_loopback_bind("127.0.0.1:7843").is_ok());
        assert!(parse_loopback_bind("[::1]:7843").is_ok());
        assert!(parse_loopback_bind("0.0.0.0:7843").is_err());
        assert!(parse_loopback_bind("192.168.1.4:7843").is_err());
    }

    #[test]
    fn broker_client_url_must_be_plain_http_loopback_without_credentials() {
        assert!(parse_loopback_url("http://127.0.0.1:7843").is_ok());
        assert!(parse_loopback_url("http://[::1]:7843").is_ok());
        assert!(parse_loopback_url("https://127.0.0.1:7843").is_err());
        assert!(parse_loopback_url("http://192.168.1.4:7843").is_err());
        assert!(parse_loopback_url("http://user:pass@127.0.0.1:7843").is_err());
    }

    #[test]
    fn sanitized_usage_excludes_identity_source_and_raw_provider_data() {
        let observed_at = "2026-08-18T12:00:00Z".parse().unwrap();
        let result = FetchResult {
            usage: UsageSnapshot {
                provider: Provider::OpenRouter,
                source: "api-secret-source".to_string(),
                primary: Some(RateWindow {
                    used_percent: 27.5,
                    window_minutes: 300,
                    resets_at: Some("2026-08-18T17:00:00Z".parse().unwrap()),
                    reset_description: Some("sensitive provider text".to_string()),
                }),
                secondary: None,
                tertiary: None,
                identity: Some(ProviderIdentity {
                    email: Some("secret@example.com".to_string()),
                    organization: Some("private-org".to_string()),
                    plan: Some("enterprise".to_string()),
                }),
            },
            credits: Some(CreditsSnapshot {
                remaining: 12.5,
                has_credits: true,
                unlimited: false,
                used: Some(2.0),
                limit: Some(14.5),
                currency: Some("usd".to_string()),
                period: Some("monthly".to_string()),
            }),
        };

        let usage = sanitize_success(Provider::OpenRouter, result, observed_at);
        let json = serde_json::to_string(&usage).unwrap();

        assert_eq!(usage.windows[0].remaining_percent, 72.5);
        assert_eq!(usage.credits.unwrap().remaining, 12.5);
        assert!(!json.contains("secret@example.com"));
        assert!(!json.contains("private-org"));
        assert!(!json.contains("api-secret-source"));
        assert!(!json.contains("sensitive provider text"));
    }

    #[test]
    fn failed_collection_exposes_only_generic_error() {
        let observed_at = "2026-08-18T12:00:00Z".parse().unwrap();
        let usage = sanitize_failure(Provider::Copilot, observed_at);
        let json = serde_json::to_string(&usage).unwrap();

        assert!(!usage.available);
        assert_eq!(usage.error_code.as_deref(), Some("collection_failed"));
        assert!(!json.contains("token"));
    }

    #[test]
    fn response_reports_cache_age_and_staleness() {
        let generated_at = "2026-08-18T12:00:00Z".parse().unwrap();
        let fresh_now = "2026-08-18T12:00:30Z".parse().unwrap();
        let stale_now = "2026-08-18T12:02:01Z".parse().unwrap();

        let fresh = build_response(Vec::new(), generated_at, fresh_now, 120);
        let stale = build_response(Vec::new(), generated_at, stale_now, 120);

        assert_eq!(fresh.schema_version, 1);
        assert_eq!(fresh.age_seconds, 30);
        assert!(!fresh.stale);
        assert_eq!(stale.age_seconds, 121);
        assert!(stale.stale);
    }

    #[test]
    fn response_age_never_goes_negative() {
        let generated_at = "2026-08-18T12:00:00Z".parse().unwrap();
        let clock_skewed_now = "2026-08-18T11:59:00Z".parse().unwrap();

        let response = build_response(Vec::new(), generated_at, clock_skewed_now, 120);

        assert_eq!(response.age_seconds, 0);
        assert!(!response.stale);
    }
}
