use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Ollama cloud usage provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Ollama cloud usage requires browser cookies (not yet supported)")
}
