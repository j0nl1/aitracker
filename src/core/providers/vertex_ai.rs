use anyhow::Result;

use crate::core::providers::fetch::FetchResult;

/// Vertex AI monitoring provider (stub).
pub async fn fetch() -> Result<FetchResult> {
    anyhow::bail!("Vertex AI monitoring requires gcloud project setup (not yet supported)")
}
