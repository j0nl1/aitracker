use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Doubao usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Doubao read-only usage monitoring is not yet implemented")
}
