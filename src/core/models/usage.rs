use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::core::providers::Provider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateWindow {
    /// Percentage of the rate limit that has been used (0.0 - 100.0)
    pub used_percent: f64,
    /// Duration of the rate window in minutes
    pub window_minutes: u64,
    /// When the rate window resets
    pub resets_at: Option<DateTime<Utc>>,
    /// Human-readable reset description (e.g., "Tomorrow at 1:00 AM")
    pub reset_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderIdentity {
    pub email: Option<String>,
    pub organization: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub provider: Provider,
    pub source: String, // "oauth", "cli", "api"
    /// Primary rate window (usually session/5-hour)
    pub primary: Option<RateWindow>,
    /// Secondary rate window (usually weekly/7-day)
    pub secondary: Option<RateWindow>,
    /// Tertiary rate window (model-specific, e.g., Sonnet limit)
    pub tertiary: Option<RateWindow>,
    /// Provider identity (email, plan, org)
    pub identity: Option<ProviderIdentity>,
}
