use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Factory usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Factory requires browser cookies (not yet supported)")
}
