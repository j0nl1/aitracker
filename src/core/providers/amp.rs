use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Amp usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Amp requires browser cookies (not yet supported)")
}
