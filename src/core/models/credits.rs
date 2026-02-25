use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditsSnapshot {
    /// Remaining credit balance in dollars
    pub remaining: f64,
    /// Whether the account has any credits
    pub has_credits: bool,
    /// Whether credits are unlimited
    pub unlimited: bool,
    /// Amount used in current period (dollars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
    /// Spending limit for current period (dollars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    /// Currency code (e.g., "usd")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Billing period (e.g., "Monthly")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
}
