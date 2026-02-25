use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;

use crate::core::models::usage::{RateWindow, UsageSnapshot};
use crate::core::providers::fetch::FetchResult;
use crate::core::providers::Provider;

// --- XML / config discovery ---

/// Search directories that may contain JetBrains AI quota config files.
fn candidate_dirs() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let mut dirs = Vec::new();

    // ~/.config/JetBrains/*/options/
    let config_jb = home.join(".config").join("JetBrains");
    collect_option_dirs(&config_jb, &mut dirs);

    // ~/.local/share/JetBrains/*/options/
    let share_jb = home.join(".local").join("share").join("JetBrains");
    collect_option_dirs(&share_jb, &mut dirs);

    // ~/.config/Google/*/options/  (Android Studio)
    let config_google = home.join(".config").join("Google");
    collect_option_dirs(&config_google, &mut dirs);

    dirs
}

fn collect_option_dirs(parent: &PathBuf, dirs: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let options = entry.path().join("options");
            if options.is_dir() {
                dirs.push(options);
            }
        }
    }
}

fn find_quota_file() -> Option<PathBuf> {
    for dir in candidate_dirs() {
        let path = dir.join("AIAssistantQuotaManager2.xml");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

// --- HTML entity decoding ---

fn decode_xml_entities(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

// --- Parsed JSON from XML attributes ---

#[derive(Deserialize)]
struct QuotaInfo {
    current: Option<f64>,
    maximum: Option<f64>,
}

#[derive(Deserialize)]
struct NextRefill {
    next: Option<i64>,
}

fn extract_attribute_value<'a>(content: &'a str, attr_name: &str) -> Option<&'a str> {
    let search = format!("{}=\"", attr_name);
    let start = content.find(&search)?;
    let value_start = start + search.len();
    let rest = &content[value_start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_quota_xml(content: &str) -> Result<(Option<QuotaInfo>, Option<NextRefill>)> {
    let quota_info = extract_attribute_value(content, "quotaInfo")
        .map(|raw| {
            let decoded = decode_xml_entities(raw);
            serde_json::from_str::<QuotaInfo>(&decoded).context("Failed to parse quotaInfo JSON")
        })
        .transpose()?;

    let next_refill = extract_attribute_value(content, "nextRefill")
        .map(|raw| {
            let decoded = decode_xml_entities(raw);
            serde_json::from_str::<NextRefill>(&decoded).context("Failed to parse nextRefill JSON")
        })
        .transpose()?;

    Ok((quota_info, next_refill))
}

/// Fetch usage data from JetBrains AI Assistant quota config files.
pub async fn fetch() -> Result<FetchResult> {
    let path = find_quota_file().context("No JetBrains AI config found")?;

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let (quota_info, next_refill) = parse_quota_xml(&content)?;

    let primary = quota_info.map(|qi| {
        let used_percent = match (qi.current, qi.maximum) {
            (Some(current), Some(maximum)) if maximum > 0.0 => current / maximum * 100.0,
            _ => 0.0,
        };

        let resets_at = next_refill
            .as_ref()
            .and_then(|nr| nr.next)
            .and_then(|millis| Utc.timestamp_millis_opt(millis).single());

        RateWindow {
            used_percent,
            window_minutes: 0,
            resets_at,
            reset_description: None,
        }
    });

    let usage = UsageSnapshot {
        provider: Provider::JetBrains,
        source: "file".to_string(),
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
    fn decode_xml_entities_all() {
        let input = "&quot;key&quot;: &lt;value&gt; &amp; more";
        let decoded = decode_xml_entities(input);
        assert_eq!(decoded, "\"key\": <value> & more");
    }

    #[test]
    fn extract_attribute_value_found() {
        let xml = r#"<component quotaInfo="some_value" nextRefill="other_value" />"#;
        assert_eq!(
            extract_attribute_value(xml, "quotaInfo"),
            Some("some_value")
        );
        assert_eq!(
            extract_attribute_value(xml, "nextRefill"),
            Some("other_value")
        );
    }

    #[test]
    fn extract_attribute_value_missing() {
        let xml = r#"<component quotaInfo="some_value" />"#;
        assert_eq!(extract_attribute_value(xml, "nextRefill"), None);
    }

    #[test]
    fn parse_quota_xml_full() {
        let xml = r#"<component name="AIAssistantQuotaManager2"
            quotaInfo="{&quot;current&quot;:42,&quot;maximum&quot;:100}"
            nextRefill="{&quot;next&quot;:1709251200000}" />"#;
        let (qi, nr) = parse_quota_xml(xml).unwrap();
        let qi = qi.unwrap();
        assert!((qi.current.unwrap() - 42.0).abs() < f64::EPSILON);
        assert!((qi.maximum.unwrap() - 100.0).abs() < f64::EPSILON);
        let nr = nr.unwrap();
        assert_eq!(nr.next, Some(1709251200000));
    }

    #[test]
    fn parse_quota_xml_missing_attributes() {
        let xml = r#"<component name="AIAssistantQuotaManager2" />"#;
        let (qi, nr) = parse_quota_xml(xml).unwrap();
        assert!(qi.is_none());
        assert!(nr.is_none());
    }

    #[test]
    fn used_percent_calculation() {
        let qi = QuotaInfo {
            current: Some(42.0),
            maximum: Some(100.0),
        };
        let percent = qi.current.unwrap() / qi.maximum.unwrap() * 100.0;
        assert!((percent - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn used_percent_zero_maximum() {
        let qi = QuotaInfo {
            current: Some(10.0),
            maximum: Some(0.0),
        };
        // Should not divide by zero
        let percent = match (qi.current, qi.maximum) {
            (Some(current), Some(maximum)) if maximum > 0.0 => current / maximum * 100.0,
            _ => 0.0,
        };
        assert!((percent - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reset_time_from_epoch_millis() {
        let millis: i64 = 1709251200000; // 2024-03-01T00:00:00Z
        let dt = Utc.timestamp_millis_opt(millis).single();
        assert!(dt.is_some());
    }

    #[test]
    fn deserialize_quota_info() {
        let json = r#"{"current": 50, "maximum": 200}"#;
        let qi: QuotaInfo = serde_json::from_str(json).unwrap();
        assert!((qi.current.unwrap() - 50.0).abs() < f64::EPSILON);
        assert!((qi.maximum.unwrap() - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn deserialize_next_refill() {
        let json = r#"{"next": 1709251200000}"#;
        let nr: NextRefill = serde_json::from_str(json).unwrap();
        assert_eq!(nr.next, Some(1709251200000));
    }

    #[test]
    fn deserialize_partial_quota_info() {
        let json = r#"{}"#;
        let qi: QuotaInfo = serde_json::from_str(json).unwrap();
        assert!(qi.current.is_none());
        assert!(qi.maximum.is_none());
    }
}
