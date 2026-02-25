use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

// --- Process discovery ---

fn detect_language_server() -> Result<(String, u16)> {
    let process_name = if cfg!(target_os = "macos") {
        "language_server_macos"
    } else {
        "language_server_linux"
    };

    let output = std::process::Command::new("pgrep")
        .args(["-a", "language_server"])
        .output()
        .context("Failed to run pgrep")?;

    if !output.status.success() {
        anyhow::bail!("Antigravity language server not running");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if !line.contains(process_name) {
            continue;
        }

        let csrf_token = extract_arg(line, "--csrf_token")
            .context("No --csrf_token found in language server process args")?;
        let port_str = extract_arg(line, "--api_server_port")
            .or_else(|| extract_port_from_line(line))
            .context("No port found in language server process args")?;
        let port: u16 = port_str
            .parse()
            .with_context(|| format!("Invalid port number: {}", port_str))?;

        return Ok((csrf_token, port));
    }

    anyhow::bail!("Antigravity language server not running");
}

fn extract_arg(line: &str, flag: &str) -> Option<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        // Handle --flag value
        if *part == flag {
            return parts.get(i + 1).map(|s| s.to_string());
        }
        // Handle --flag=value
        if let Some(rest) = part.strip_prefix(flag) {
            if let Some(val) = rest.strip_prefix('=') {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn extract_port_from_line(line: &str) -> Option<String> {
    // Fallback: look for a port-like number after "port" keyword
    let lower = line.to_lowercase();
    if let Some(idx) = lower.find("port") {
        let rest = &line[idx + 4..];
        let num: String = rest
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !num.is_empty() {
            return Some(num);
        }
    }
    None
}

// --- API response ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatusResponse {
    cascade_model_config_data: Option<CascadeModelConfigData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CascadeModelConfigData {
    #[serde(default)]
    client_model_configs: Vec<ClientModelConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientModelConfig {
    quota_info: Option<QuotaInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaInfo {
    remaining_fraction: Option<f64>,
}

/// Fetch usage data from the Antigravity language server.
pub async fn fetch() -> Result<FetchResult> {
    let (csrf_token, port) = detect_language_server()?;

    let url = format!(
        "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus",
        port
    );

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-csrf-token", &csrf_token)
        .body("{}")
        .send()
        .await
        .context("Failed to connect to Antigravity language server")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let data: UserStatusResponse = response
        .json()
        .await
        .context("Failed to parse Antigravity user status response")?;

    let primary = data
        .cascade_model_config_data
        .as_ref()
        .and_then(|d| d.client_model_configs.first())
        .and_then(|c| c.quota_info.as_ref())
        .and_then(|qi| qi.remaining_fraction)
        .map(|frac| {
            let used_percent = (1.0 - frac) * 100.0;
            RateWindow {
                used_percent,
                window_minutes: 0,
                resets_at: None,
                reset_description: None,
            }
        });

    let usage = UsageSnapshot {
        provider: Provider::Antigravity,
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
    fn deserialize_user_status_response() {
        let json = r#"{
            "cascadeModelConfigData": {
                "clientModelConfigs": [
                    {
                        "quotaInfo": {
                            "remainingFraction": 0.75
                        }
                    },
                    {
                        "quotaInfo": {
                            "remainingFraction": 0.50
                        }
                    }
                ]
            }
        }"#;
        let data: UserStatusResponse = serde_json::from_str(json).unwrap();
        let configs = &data.cascade_model_config_data.unwrap().client_model_configs;
        assert_eq!(configs.len(), 2);
        assert!(
            (configs[0]
                .quota_info
                .as_ref()
                .unwrap()
                .remaining_fraction
                .unwrap()
                - 0.75)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{}"#;
        let data: UserStatusResponse = serde_json::from_str(json).unwrap();
        assert!(data.cascade_model_config_data.is_none());
    }

    #[test]
    fn deserialize_empty_configs() {
        let json = r#"{ "cascadeModelConfigData": { "clientModelConfigs": [] } }"#;
        let data: UserStatusResponse = serde_json::from_str(json).unwrap();
        assert!(data
            .cascade_model_config_data
            .unwrap()
            .client_model_configs
            .is_empty());
    }

    #[test]
    fn deserialize_missing_quota_info() {
        let json = r#"{
            "cascadeModelConfigData": {
                "clientModelConfigs": [{}]
            }
        }"#;
        let data: UserStatusResponse = serde_json::from_str(json).unwrap();
        let configs = &data.cascade_model_config_data.unwrap().client_model_configs;
        assert!(configs[0].quota_info.is_none());
    }

    #[test]
    fn used_percent_from_fraction() {
        let frac: f64 = 0.75;
        let used_percent = (1.0 - frac) * 100.0;
        assert!((used_percent - 25.0).abs() < 1e-10);
    }

    #[test]
    fn used_percent_fully_used() {
        let frac: f64 = 0.0;
        let used_percent = (1.0 - frac) * 100.0;
        assert!((used_percent - 100.0).abs() < 1e-10);
    }

    #[test]
    fn extract_arg_space_separated() {
        let line =
            "12345 /usr/bin/language_server_linux --csrf_token abc123 --api_server_port 8080";
        assert_eq!(
            extract_arg(line, "--csrf_token"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_arg(line, "--api_server_port"),
            Some("8080".to_string())
        );
    }

    #[test]
    fn extract_arg_equals_separated() {
        let line =
            "12345 /usr/bin/language_server_linux --csrf_token=abc123 --api_server_port=9090";
        assert_eq!(
            extract_arg(line, "--csrf_token"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_arg(line, "--api_server_port"),
            Some("9090".to_string())
        );
    }

    #[test]
    fn extract_arg_missing() {
        let line = "12345 /usr/bin/language_server_linux --other flag";
        assert_eq!(extract_arg(line, "--csrf_token"), None);
    }

    #[test]
    fn extract_port_from_line_fallback() {
        let line = "12345 language_server_linux port 4321";
        assert_eq!(extract_port_from_line(line), Some("4321".to_string()));
    }

    #[test]
    fn extract_port_from_line_no_port() {
        let line = "12345 language_server_linux --csrf_token abc";
        assert_eq!(extract_port_from_line(line), None);
    }
}
