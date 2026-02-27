use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

use crate::core::models::usage::{ProviderIdentity, RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

const KIRO_TIMEOUT: Duration = Duration::from_secs(20);

// --- Parsed output ---

#[derive(Debug, Default, Deserialize)]
struct KiroUsage {
    plan: Option<String>,
    credits_percent: Option<f64>,
    used: Option<f64>,
    total: Option<f64>,
    reset_info: Option<String>,
}

fn parse_kiro_output(stdout: &str) -> KiroUsage {
    let mut usage = KiroUsage::default();

    for line in stdout.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("Plan:") {
            usage.plan = Some(rest.trim().to_string());
            continue;
        }

        if trimmed.contains("Credits:") {
            // Look for percentage like "75%" or "75.5%"
            if let Some(pct) = extract_percentage(trimmed) {
                usage.credits_percent = Some(pct);
            }
            continue;
        }

        // Pattern: "X / Y" or "X of Y"
        if trimmed.contains(" / ") || trimmed.contains(" of ") {
            if let Some((used, total)) = extract_used_total(trimmed) {
                usage.used = Some(used);
                usage.total = Some(total);
            }
            continue;
        }

        if trimmed.to_lowercase().contains("reset") {
            usage.reset_info = Some(trimmed.to_string());
        }
    }

    usage
}

fn extract_percentage(s: &str) -> Option<f64> {
    // Find a number followed by '%'
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'%' && i > 0 {
            // Walk backwards to find the number start
            let end = i;
            let mut start = i - 1;
            while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
                start -= 1;
            }
            if bytes[start].is_ascii_digit() {
                if let Ok(val) = s[start..end].parse::<f64>() {
                    return Some(val);
                }
            }
        }
        i += 1;
    }
    None
}

fn extract_used_total(s: &str) -> Option<(f64, f64)> {
    // Try "X / Y" first, then "X of Y"
    let parts: Vec<&str> = if s.contains(" / ") {
        s.split(" / ").collect()
    } else if s.contains(" of ") {
        s.split(" of ").collect()
    } else {
        return None;
    };

    if parts.len() != 2 {
        return None;
    }

    let used = extract_last_number(parts[0])?;
    let total = extract_first_number(parts[1])?;
    Some((used, total))
}

fn extract_first_number(s: &str) -> Option<f64> {
    let s = s.trim();
    let num_str: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num_str.parse().ok()
}

fn extract_last_number(s: &str) -> Option<f64> {
    let s = s.trim();
    let num_str: String = s
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    num_str.parse().ok()
}

/// Fetch usage data by running the `kiro-cli` command.
pub async fn fetch() -> Result<FetchResult> {
    if crate::core::process::which("kiro-cli").is_none() {
        anyhow::bail!("kiro-cli not found in PATH");
    }

    let output = tokio::process::Command::new("kiro-cli")
        .args(["chat", "--no-interactive", "/usage"])
        .output();

    let output = tokio::time::timeout(KIRO_TIMEOUT, output)
        .await
        .context("kiro-cli timed out after 20 seconds")?
        .context("Failed to execute kiro-cli")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("kiro-cli exited with {}: {}", output.status, stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_kiro_output(&stdout);

    let primary = parsed.credits_percent.map(|pct| RateWindow {
        used_percent: pct,
        window_minutes: 0,
        resets_at: None,
        reset_description: parsed.reset_info.clone(),
    });

    let identity = parsed.plan.map(|plan| ProviderIdentity {
        email: None,
        organization: None,
        plan: Some(plan),
    });

    let usage = UsageSnapshot {
        provider: Provider::Kiro,
        source: "cli".to_string(),
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
    fn parse_kiro_output_full() {
        let stdout = "\
Plan: Pro
Credits: 75% remaining
Used 150 / 200 requests
Resets on March 1, 2026
Bonus credits: 50";
        let parsed = parse_kiro_output(stdout);
        assert_eq!(parsed.plan.as_deref(), Some("Pro"));
        assert!((parsed.credits_percent.unwrap() - 75.0).abs() < f64::EPSILON);
        assert!((parsed.used.unwrap() - 150.0).abs() < f64::EPSILON);
        assert!((parsed.total.unwrap() - 200.0).abs() < f64::EPSILON);
        assert!(parsed.reset_info.is_some());
        assert!(parsed.reset_info.unwrap().contains("Reset"));
    }

    #[test]
    fn parse_kiro_output_percentage_with_decimal() {
        let stdout = "Credits: 42.5% used";
        let parsed = parse_kiro_output(stdout);
        assert!((parsed.credits_percent.unwrap() - 42.5).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_kiro_output_of_pattern() {
        let stdout = "Used 50 of 100 requests";
        let parsed = parse_kiro_output(stdout);
        assert!((parsed.used.unwrap() - 50.0).abs() < f64::EPSILON);
        assert!((parsed.total.unwrap() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_kiro_output_empty() {
        let parsed = parse_kiro_output("");
        assert!(parsed.plan.is_none());
        assert!(parsed.credits_percent.is_none());
        assert!(parsed.used.is_none());
        assert!(parsed.total.is_none());
        assert!(parsed.reset_info.is_none());
    }

    #[test]
    fn parse_kiro_output_no_matching_lines() {
        let stdout = "Welcome to Kiro!\nReady.\n";
        let parsed = parse_kiro_output(stdout);
        assert!(parsed.plan.is_none());
        assert!(parsed.credits_percent.is_none());
    }

    #[test]
    fn extract_percentage_basic() {
        assert!((extract_percentage("75%").unwrap() - 75.0).abs() < f64::EPSILON);
        assert!(
            (extract_percentage("Credits: 42.5% remaining").unwrap() - 42.5).abs() < f64::EPSILON
        );
        assert!(extract_percentage("no percent here").is_none());
    }

    #[test]
    fn extract_used_total_slash() {
        let (used, total) = extract_used_total("150 / 200").unwrap();
        assert!((used - 150.0).abs() < f64::EPSILON);
        assert!((total - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_used_total_of() {
        let (used, total) = extract_used_total("50 of 100").unwrap();
        assert!((used - 50.0).abs() < f64::EPSILON);
        assert!((total - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_used_total_with_prefix() {
        let (used, total) = extract_used_total("Used 30 / 60 requests").unwrap();
        assert!((used - 30.0).abs() < f64::EPSILON);
        assert!((total - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_used_total_no_pattern() {
        assert!(extract_used_total("no pattern here").is_none());
    }
}
