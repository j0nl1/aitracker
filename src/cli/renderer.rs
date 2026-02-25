use colored::{control, ColoredString, Colorize};

use crate::core::formatter::{
    format_credits, format_remaining_percent, format_reset_countdown, format_reset_datetime,
    format_usage_bar,
};
use crate::core::models::cost::CostSummary;
use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::status::{StatusIndicator, StatusInfo};
use crate::core::models::usage::{RateWindow, UsageSnapshot};

const BAR_WIDTH: usize = 12;

/// Render a full provider block as a colored (or plain) string.
///
/// Layout:
/// ```text
///  Claude (oauth)
///   Session   72% remaining [████████░░░░]
///             Resets in 2h 15m
///   Weekly    41% remaining [█████░░░░░░░]
///             Resets Tomorrow at 1:00 AM
///   Sonnet    88% remaining [██████████░░]
///   Account   user@example.com
///   Plan      Pro
///   Status    Operational
/// ```
fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        format!("{}", count)
    }
}

pub fn render_provider(
    snapshot: &UsageSnapshot,
    credits: Option<&CreditsSnapshot>,
    cost: Option<&CostSummary>,
    status: Option<&StatusInfo>,
    show_detailed_cost: bool,
    use_color: bool,
) -> String {
    control::set_override(use_color);

    let mut lines: Vec<String> = Vec::new();

    // Header: " Claude (oauth)"
    let header = format!(
        " {} ({})",
        snapshot.provider.display_name(),
        snapshot.source
    );
    lines.push(header.bold().to_string());

    // Rate windows — insert a blank line before a window when the previous
    // one had no sub-line (e.g. no "Resets in …") to keep visual spacing even.
    let windows: [Option<(&str, &RateWindow)>; 3] = [
        snapshot
            .primary
            .as_ref()
            .map(|w| (snapshot.provider.session_label(), w)),
        snapshot
            .secondary
            .as_ref()
            .map(|w| (snapshot.provider.weekly_label(), w)),
        snapshot
            .tertiary
            .as_ref()
            .map(|w| (snapshot.provider.tertiary_label(), w)),
    ];

    let mut prev_had_subline = true;
    for entry in windows.into_iter().flatten() {
        let (label, window) = entry;
        if !prev_had_subline {
            lines.push(String::new());
        }
        render_rate_window(&mut lines, label, window);
        prev_had_subline = window.resets_at.is_some();
    }

    // Identity lines
    if let Some(identity) = &snapshot.identity {
        if let Some(email) = &identity.email {
            lines.push(format!(
                "  {}   {}",
                "Account".cyan(),
                email
            ));
        }
        if let Some(plan) = &identity.plan {
            lines.push(format!(
                "  {}      {}",
                "Plan".cyan(),
                plan
            ));
        }
    }

    // Credits
    if let Some(credits) = credits {
        let credits_str = if credits.unlimited {
            "Unlimited".to_string()
        } else if let (Some(used), Some(limit)) = (credits.used, credits.limit) {
            let period_suffix = credits
                .period
                .as_deref()
                .map(|p| format!(" ({})", p))
                .unwrap_or_default();
            format!("${:.2} / ${:.2} used{}", used, limit, period_suffix)
        } else if credits.has_credits {
            format_credits(credits.remaining)
        } else {
            "No credits".to_string()
        };
        lines.push(format!("  {}   {}", "Credits".cyan(), credits_str));
    }

    // Cost
    if let Some(cost) = cost {
        if show_detailed_cost {
            // Detailed: header + totals + by-model + recent days
            lines.push(format!(
                "  {} ${:.2}",
                format!("Cost({}d)", cost.days).cyan(),
                cost.total_cost
            ));
            lines.push(format!(
                "  {}     ${:.2}",
                "Today".cyan(),
                cost.today_cost
            ));

            if !cost.by_model.is_empty() {
                lines.push(format!("  {}:", "By Model".cyan()));
                for model in &cost.by_model {
                    let in_tok = format_tokens(model.input_tokens);
                    let out_tok = format_tokens(model.output_tokens);
                    lines.push(format!(
                        "    {:<24} ${:<8.2} ({} in / {} out)",
                        model.model, model.total_cost, in_tok, out_tok
                    ));
                }
            }

            if !cost.daily.is_empty() {
                lines.push(format!("  {}:", "Recent Days".cyan()));
                for day in cost.daily.iter().take(10) {
                    lines.push(format!(
                        "    {:<12} ${:.2}",
                        day.date.format("%b %d"),
                        day.total_cost
                    ));
                }
            }
        } else {
            // Compact one-liner
            let cost_str = format!(
                "${:.2} total, ${:.2} today",
                cost.total_cost, cost.today_cost
            );
            lines.push(format!(
                "  {} {}",
                format!("Cost({}d)", cost.days).cyan(),
                cost_str
            ));
        }
    }

    // Status
    if let Some(status) = status {
        let status_text = status.indicator.to_string();
        let colored_status: ColoredString = match status.indicator {
            StatusIndicator::Operational => status_text.green(),
            StatusIndicator::Minor => status_text.yellow(),
            StatusIndicator::Major | StatusIndicator::Critical => status_text.red(),
            StatusIndicator::Maintenance => status_text.blue(),
            StatusIndicator::Unknown => status_text.dimmed(),
        };
        lines.push(format!("  {}    {}", "Status".cyan(), colored_status));
    }

    lines.join("\n")
}

fn render_rate_window(lines: &mut Vec<String>, label: &str, window: &RateWindow) {
    let percent_str = format_remaining_percent(window.used_percent);
    let bar_str = format_usage_bar(window.used_percent, BAR_WIDTH);

    let colored_percent = color_by_remaining(window.used_percent, &percent_str);
    let colored_bar = bar_str.magenta();

    // Pad label to 7 chars for alignment
    let padded_label = format!("{:<7}", label);

    lines.push(format!(
        "  {}  {} {}",
        padded_label.cyan(),
        colored_percent,
        colored_bar
    ));

    // Reset line (only when resets_at is present)
    if let Some(resets_at) = &window.resets_at {
        let reset_line = if window.reset_description.is_some() {
            format_reset_datetime(resets_at)
        } else {
            format_reset_countdown(resets_at)
        };
        // 11 spaces to align under the percent/bar values
        lines.push(format!("           {}", reset_line.dimmed()));
    }
}

/// Color the percent string green/yellow/red based on remaining percentage.
fn color_by_remaining(used_percent: f64, text: &str) -> ColoredString {
    let remaining = 100.0 - used_percent;
    if remaining >= 25.0 {
        text.green()
    } else if remaining >= 10.0 {
        text.yellow()
    } else {
        text.red()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::status::StatusIndicator;
    use crate::core::models::usage::{ProviderIdentity, RateWindow, UsageSnapshot};
    use crate::core::providers::Provider;
    use chrono::Utc;

    fn make_window(used_percent: f64) -> RateWindow {
        RateWindow {
            used_percent,
            window_minutes: 300,
            resets_at: Some(Utc::now() + chrono::Duration::hours(2)),
            reset_description: None,
        }
    }

    fn make_snapshot() -> UsageSnapshot {
        UsageSnapshot {
            provider: Provider::Claude,
            source: "oauth".to_string(),
            primary: Some(make_window(28.0)),
            secondary: Some(make_window(59.0)),
            tertiary: None,
            identity: Some(ProviderIdentity {
                email: Some("user@example.com".to_string()),
                organization: None,
                plan: Some("Pro".to_string()),
            }),
        }
    }

    #[test]
    fn render_contains_provider_name() {
        let snapshot = make_snapshot();
        let output = render_provider(&snapshot, None, None, None, false, false);
        assert!(output.contains("Claude"));
        assert!(output.contains("oauth"));
    }

    #[test]
    fn render_contains_labels() {
        let snapshot = make_snapshot();
        let output = render_provider(&snapshot, None, None, None, false, false);
        assert!(output.contains("Session"));
        assert!(output.contains("Weekly"));
    }

    #[test]
    fn render_contains_identity() {
        let snapshot = make_snapshot();
        let output = render_provider(&snapshot, None, None, None, false, false);
        assert!(output.contains("user@example.com"));
        assert!(output.contains("Pro"));
    }

    #[test]
    fn render_contains_status() {
        let snapshot = make_snapshot();
        let status = StatusInfo {
            indicator: StatusIndicator::Operational,
            description: None,
        };
        let output = render_provider(&snapshot, None, None, Some(&status), false, false);
        assert!(output.contains("Operational"));
    }

    #[test]
    fn render_contains_credits() {
        let snapshot = make_snapshot();
        let credits = CreditsSnapshot {
            remaining: 42.50,
            has_credits: true,
            unlimited: false,
            used: None,
            limit: None,
            currency: None,
            period: None,
        };
        let output = render_provider(&snapshot, Some(&credits), None, None, false, false);
        assert!(output.contains("$42.50 remaining"));
    }

    #[test]
    fn render_contains_spend_credits() {
        let snapshot = make_snapshot();
        let credits = CreditsSnapshot {
            remaining: 37.66,
            has_credits: true,
            unlimited: false,
            used: Some(12.34),
            limit: Some(50.00),
            currency: Some("usd".to_string()),
            period: Some("Monthly".to_string()),
        };
        let output = render_provider(&snapshot, Some(&credits), None, None, false, false);
        assert!(output.contains("$12.34 / $50.00 used (Monthly)"));
    }

    #[test]
    fn render_no_ansi_when_color_false() {
        let snapshot = make_snapshot();
        let output = render_provider(&snapshot, None, None, None, false, false);
        // ANSI escape sequences start with ESC (0x1b)
        assert!(!output.contains('\x1b'), "output should not contain ANSI codes");
    }

    #[test]
    fn render_contains_cost() {
        let snapshot = make_snapshot();
        let cost = CostSummary {
            total_cost: 45.67,
            today_cost: 3.21,
            days: 30,
            by_model: vec![],
            daily: vec![],
        };
        // Compact mode (default)
        let output = render_provider(&snapshot, None, Some(&cost), None, false, false);
        assert!(output.contains("Cost(30d)"));
        assert!(output.contains("$45.67 total"));
        assert!(output.contains("$3.21 today"));

        // Detailed mode (--all)
        let output_all = render_provider(&snapshot, None, Some(&cost), None, true, false);
        assert!(output_all.contains("Cost(30d)"));
        assert!(output_all.contains("$45.67"));
        assert!(output_all.contains("Today"));
    }
}
