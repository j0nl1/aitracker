use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Azure OpenAI usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Azure OpenAI read-only usage monitoring is not yet implemented")
}
