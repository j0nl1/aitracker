use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::usage::UsageSnapshot;

/// Result of a provider fetch operation.
pub struct FetchResult {
    pub usage: UsageSnapshot,
    pub credits: Option<CreditsSnapshot>,
}

/// Validate that a resolved endpoint URL uses HTTPS.
///
/// All providers that allow endpoint overrides must call this before sending
/// credentials, to prevent exfiltration over plain HTTP or other schemes.
pub fn validate_endpoint(url: &str, provider_name: &str) -> anyhow::Result<()> {
    if !url.starts_with("https://") {
        anyhow::bail!(
            "{}: endpoint must use HTTPS, got: {}",
            provider_name,
            url
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_endpoint_accepts_https() {
        assert!(validate_endpoint("https://api.example.com/v1", "Test").is_ok());
    }

    #[test]
    fn validate_endpoint_rejects_http() {
        let err = validate_endpoint("http://evil.com", "Test").unwrap_err();
        assert!(err.to_string().contains("must use HTTPS"));
    }

    #[test]
    fn validate_endpoint_rejects_empty() {
        assert!(validate_endpoint("", "Test").is_err());
    }

    #[test]
    fn validate_endpoint_rejects_file_scheme() {
        assert!(validate_endpoint("file:///etc/passwd", "Test").is_err());
    }

    #[test]
    fn validate_endpoint_rejects_no_scheme() {
        assert!(validate_endpoint("api.example.com/v1", "Test").is_err());
    }
}
