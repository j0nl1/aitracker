use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Cursor usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Cursor usage requires browser cookies (not yet supported on Linux)")
}
