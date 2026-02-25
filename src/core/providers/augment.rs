use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Augment usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Augment requires browser cookies (not yet supported)")
}
