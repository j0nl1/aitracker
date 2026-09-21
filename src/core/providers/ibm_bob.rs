use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const DEFAULT_BASE: &str = "https://api.us-east.bob.ibm.com";

#[derive(Deserialize)]
struct Team {
    id: String,
    budget_limit: Option<f64>,
}

#[derive(Deserialize)]
struct Instance {
    instance_id: String,
    user_id: Option<String>,
    region_domain: Option<String>,
    #[serde(default)]
    teams: Vec<Team>,
}

#[derive(Deserialize)]
struct ProfileResponse {
    #[serde(default)]
    instances: Vec<Instance>,
}

#[derive(Deserialize)]
struct TeamBudgetResponse {
    usage: f64,
    budget_limit: Option<f64>,
}

fn auth_header(token: &str) -> String {
    let is_jwt = token.split('.').count() == 3
        && token
            .split('.')
            .nth(1)
            .and_then(|seg| {
                use base64_url_decode::decode_segment;
                decode_segment(seg)
            })
            .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).is_ok())
            .unwrap_or(false);
    if is_jwt {
        format!("Bearer {}", token)
    } else {
        format!("Apikey {}", token)
    }
}

/// Minimal base64url decoder for the middle segment of a JWT, used only to
/// detect whether a token is a JWT (vs. a plain API key).
mod base64_url_decode {
    pub fn decode_segment(segment: &str) -> Option<Vec<u8>> {
        let mut s = segment.replace('-', "+").replace('_', "/");
        while !s.len().is_multiple_of(4) {
            s.push('=');
        }
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
        let mut buf = 0u32;
        let mut bits = 0u32;
        for b in bytes {
            let val = TABLE.iter().position(|&c| c == b)? as u32;
            buf = (buf << 6) | val;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
            }
        }
        Some(out)
    }
}

fn regional_host(region_domain: &str) -> Option<String> {
    let host = if region_domain.starts_with("api.") {
        region_domain.to_string()
    } else {
        format!("api.{}", region_domain)
    };
    if host == "bob.ibm.com" || host.ends_with(".bob.ibm.com") {
        Some(format!("https://{}", host))
    } else {
        None
    }
}

/// Fetch monthly Bobcoin budget and usage across subscription teams from IBM Bob.
pub async fn fetch() -> Result<FetchResult> {
    let token = std::env::var("BOBSHELL_API_KEY").context("BOBSHELL_API_KEY env var not set")?;
    let auth = auth_header(&token);

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/admin/v1/profile", DEFAULT_BASE))
        .header("Authorization", &auth)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "aitracker")
        .send()
        .await
        .context("Failed to send request to IBM Bob profile API")?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("Invalid credentials — check BOBSHELL_API_KEY");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let profile: ProfileResponse = response
        .json()
        .await
        .context("Failed to parse IBM Bob profile response")?;

    let mut total_used = 0.0;
    let mut total_limit: Option<f64> = Some(0.0);
    let mut any_team = false;

    for instance in &profile.instances {
        let Some(user_id) = &instance.user_id else {
            continue;
        };
        let host = instance
            .region_domain
            .as_deref()
            .and_then(regional_host)
            .unwrap_or_else(|| DEFAULT_BASE.to_string());

        for team in &instance.teams {
            if team.id.is_empty() {
                continue;
            }
            let resp = client
                .get(format!(
                    "{}/admin/v1/teams/{}/users/{}",
                    host, team.id, user_id
                ))
                .header("Authorization", &auth)
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("User-Agent", "aitracker")
                .header("x-instance-id", &instance.instance_id)
                .header("x-team-id", &team.id)
                .send()
                .await
                .context("Failed to send request to IBM Bob team budget API")?;

            if !resp.status().is_success() {
                continue;
            }
            let Ok(budget) = resp.json::<TeamBudgetResponse>().await else {
                continue;
            };

            any_team = true;
            total_used += budget.usage.max(0.0);
            let effective_limit = budget.budget_limit.or(team.budget_limit);
            match (effective_limit, total_limit) {
                (Some(l), Some(acc)) if l >= 0.0 => total_limit = Some(acc + l),
                _ => total_limit = None,
            }
        }
    }

    if !any_team {
        anyhow::bail!("No active IBM Bob subscription found for this key");
    }

    let credits = Some(CreditsSnapshot {
        remaining: total_limit.map(|l| (l - total_used).max(0.0)).unwrap_or(0.0),
        has_credits: true,
        unlimited: total_limit.is_none(),
        used: Some(total_used),
        limit: total_limit,
        currency: Some("bobcoin".to_string()),
        period: None,
    });

    let usage = UsageSnapshot {
        provider: Provider::IbmBob,
        source: "api".to_string(),
        primary: None,
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
    fn deserialize_profile_response() {
        let json = r#"{
            "instances": [
                {
                    "instance_id": "instance-one",
                    "user_id": "user-one",
                    "region_domain": "eu-de.bob.ibm.com",
                    "teams": [{ "id": "team-one", "budget_limit": 40 }]
                }
            ]
        }"#;
        let data: ProfileResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.instances.len(), 1);
        assert_eq!(data.instances[0].teams[0].id, "team-one");
    }

    #[test]
    fn regional_host_accepts_bob_ibm_com() {
        assert_eq!(
            regional_host("eu-de.bob.ibm.com"),
            Some("https://api.eu-de.bob.ibm.com".to_string())
        );
    }

    #[test]
    fn regional_host_rejects_untrusted_domain() {
        assert!(regional_host("evil.com").is_none());
    }

    #[test]
    fn auth_header_plain_key_uses_apikey_scheme() {
        assert_eq!(auth_header("plain-key-123"), "Apikey plain-key-123");
    }

    #[test]
    fn auth_header_jwt_uses_bearer_scheme() {
        let header = base64_url_encode(br#"{"alg":"none"}"#);
        let payload = base64_url_encode(br#"{"sub":"1234"}"#);
        let jwt = format!("{}.{}.sig", header, payload);
        assert_eq!(auth_header(&jwt), format!("Bearer {}", jwt));
    }

    fn base64_url_encode(data: &[u8]) -> String {
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(TABLE[((n >> 18) & 63) as usize] as char);
            out.push(TABLE[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                out.push(TABLE[((n >> 6) & 63) as usize] as char);
            }
            if chunk.len() > 2 {
                out.push(TABLE[(n & 63) as usize] as char);
            }
        }
        out.replace('+', "-").replace('/', "_")
    }
}
