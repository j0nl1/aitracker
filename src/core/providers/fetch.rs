use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;

/// Result of a provider fetch operation.
pub struct FetchResult {
    pub usage: UsageSnapshot,
    pub credits: Option<CreditsSnapshot>,
}
