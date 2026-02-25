use chrono::{DateTime, Utc};

/// Returns "{remaining}% remaining" where remaining = 100 - used, rounded to nearest integer.
pub fn format_remaining_percent(used_percent: f64) -> String {
    let remaining = (100.0 - used_percent).max(0.0).round() as u64;
    format!("{}% remaining", remaining)
}

/// Returns "Resets in Xh Ym" relative to now. If past, returns "Resets now".
/// If more than 24 hours away, includes days.
pub fn format_reset_countdown(resets_at: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = *resets_at - now;
    let total_seconds = duration.num_seconds();

    if total_seconds <= 0 {
        return "Resets now".to_string();
    }

    let total_minutes = total_seconds / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;

    if hours >= 24 {
        let days = hours / 24;
        let remaining_hours = hours % 24;
        if remaining_hours == 0 {
            format!("Resets in {}d", days)
        } else {
            format!("Resets in {}d {}h", days, remaining_hours)
        }
    } else if hours > 0 {
        format!("Resets in {}h {}m", hours, minutes)
    } else {
        format!("Resets in {}m", total_minutes.max(1))
    }
}

/// Returns "Resets {description}" like "Tomorrow at 1:00 AM", "Today at 5:30 PM", or "Wed at 3:00 PM".
pub fn format_reset_datetime(resets_at: &DateTime<Utc>) -> String {
    use chrono::{Datelike, Local, Timelike};

    let local_reset = resets_at.with_timezone(&Local);
    let now_local = Local::now();

    let reset_date = local_reset.date_naive();
    let today = now_local.date_naive();
    let tomorrow = today + chrono::Duration::days(1);

    let hour = local_reset.hour();
    let minute = local_reset.minute();
    let am_pm = if hour < 12 { "AM" } else { "PM" };
    let hour_12 = match hour % 12 {
        0 => 12,
        h => h,
    };

    let time_str = if minute == 0 {
        format!("{}:00 {}", hour_12, am_pm)
    } else {
        format!("{}:{:02} {}", hour_12, minute, am_pm)
    };

    let day_str = if reset_date == today {
        "Today".to_string()
    } else if reset_date == tomorrow {
        "Tomorrow".to_string()
    } else {
        let weekday = match local_reset.weekday() {
            chrono::Weekday::Mon => "Mon",
            chrono::Weekday::Tue => "Tue",
            chrono::Weekday::Wed => "Wed",
            chrono::Weekday::Thu => "Thu",
            chrono::Weekday::Fri => "Fri",
            chrono::Weekday::Sat => "Sat",
            chrono::Weekday::Sun => "Sun",
        };
        weekday.to_string()
    };

    format!("Resets {} at {}", day_str, time_str)
}

/// Returns "[████████░░░░]" where █ = remaining portion, ░ = used portion.
/// Width is the number of block characters inside the brackets (default 12).
pub fn format_usage_bar(used_percent: f64, width: usize) -> String {
    let used_percent = used_percent.clamp(0.0, 100.0);
    let used_blocks = ((used_percent / 100.0) * width as f64).round() as usize;
    let remaining_blocks = width.saturating_sub(used_blocks);

    let filled: String = "█".repeat(remaining_blocks);
    let empty: String = "░".repeat(used_blocks);

    format!("[{}{}]", filled, empty)
}

/// Returns "$123.45 remaining".
pub fn format_credits(remaining: f64) -> String {
    format!("${:.2} remaining", remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn format_remaining_percent_rounds() {
        assert_eq!(format_remaining_percent(28.4), "72% remaining");
        assert_eq!(format_remaining_percent(0.0), "100% remaining");
        assert_eq!(format_remaining_percent(100.0), "0% remaining");
        assert_eq!(format_remaining_percent(110.0), "0% remaining");
    }

    #[test]
    fn format_reset_countdown_past() {
        let past = Utc::now() - Duration::seconds(10);
        assert_eq!(format_reset_countdown(&past), "Resets now");
    }

    #[test]
    fn format_reset_countdown_minutes() {
        let future = Utc::now() + Duration::minutes(45);
        let result = format_reset_countdown(&future);
        assert!(result.starts_with("Resets in "));
        assert!(result.contains('m'));
    }

    #[test]
    fn format_reset_countdown_hours_and_minutes() {
        let future = Utc::now() + Duration::minutes(135); // 2h 15m
        let result = format_reset_countdown(&future);
        assert!(result.contains('h'));
        assert!(result.contains('m'));
    }

    #[test]
    fn format_reset_countdown_days() {
        let future = Utc::now() + Duration::hours(25);
        let result = format_reset_countdown(&future);
        assert!(result.contains('d'));
    }

    #[test]
    fn format_usage_bar_width() {
        // 0% used — all filled
        let bar = format_usage_bar(0.0, 12);
        assert_eq!(bar, "[████████████]");

        // 100% used — all empty
        let bar = format_usage_bar(100.0, 12);
        assert_eq!(bar, "[░░░░░░░░░░░░]");

        // 50% used — half filled, half empty
        let bar = format_usage_bar(50.0, 12);
        assert_eq!(bar, "[██████░░░░░░]");
    }

    #[test]
    fn format_credits_two_decimals() {
        assert_eq!(format_credits(123.45), "$123.45 remaining");
        assert_eq!(format_credits(0.0), "$0.00 remaining");
        assert_eq!(format_credits(5.0), "$5.00 remaining");
    }
}
