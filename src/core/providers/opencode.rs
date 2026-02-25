use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// OpenCode usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("OpenCode requires browser cookies (not yet supported)")
}
