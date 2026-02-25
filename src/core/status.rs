use anyhow::{Context, Result};
use serde::Deserialize;

use crate::core::models::status::{StatusIndicator, StatusInfo};
use crate::core::providers::Provider;

#[derive(Deserialize)]
struct StatusPageResponse {
    status: StatusPageStatus,
}

#[derive(Deserialize)]
struct StatusPageStatus {
    indicator: String,
    description: Option<String>,
}

fn parse_indicator(indicator: &str) -> StatusIndicator {
    match indicator {
        "none" => StatusIndicator::Operational,
        "minor" => StatusIndicator::Minor,
        "major" => StatusIndicator::Major,
        "critical" => StatusIndicator::Critical,
        "maintenance" => StatusIndicator::Maintenance,
        _ => StatusIndicator::Unknown,
    }
}

/// Fetch status from a provider's statuspage.io endpoint.
pub async fn fetch_status(provider: &Provider) -> Result<StatusInfo> {
    let base_url = provider
        .status_page_url()
        .context("Provider has no status page URL")?;

    let url = format!("{}/api/v2/status.json", base_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to fetch status page")?;

    if !response.status().is_success() {
        anyhow::bail!("Status page returned HTTP {}", response.status().as_u16());
    }

    let data: StatusPageResponse = response
        .json()
        .await
        .context("Failed to parse status page response")?;

    Ok(StatusInfo {
        indicator: parse_indicator(&data.status.indicator),
        description: data.status.description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_indicator_values() {
        assert_eq!(parse_indicator("none"), StatusIndicator::Operational);
        assert_eq!(parse_indicator("minor"), StatusIndicator::Minor);
        assert_eq!(parse_indicator("major"), StatusIndicator::Major);
        assert_eq!(parse_indicator("critical"), StatusIndicator::Critical);
        assert_eq!(parse_indicator("maintenance"), StatusIndicator::Maintenance);
        assert_eq!(parse_indicator("something_else"), StatusIndicator::Unknown);
    }

    #[test]
    fn deserialize_status_page_response() {
        let json = r#"{
            "page": { "id": "test", "name": "Test" },
            "status": {
                "indicator": "none",
                "description": "All Systems Operational"
            }
        }"#;
        let data: StatusPageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(data.status.indicator, "none");
        assert_eq!(data.status.description.as_deref(), Some("All Systems Operational"));
    }

    #[test]
    fn provider_status_page_urls() {
        assert!(Provider::Claude.status_page_url().is_some());
        assert!(Provider::Codex.status_page_url().is_some());
        assert!(Provider::Copilot.status_page_url().is_some());
        assert!(Provider::Warp.status_page_url().is_none());
    }
}
