use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

// --- Claude credentials ---

#[derive(Deserialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOAuthEntry>,
}

#[derive(Deserialize)]
struct ClaudeOAuthEntry {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
}

#[derive(Debug)]
pub struct ClaudeCredentials {
    pub access_token: String,
}

/// Read Claude OAuth credentials from ~/.claude/.credentials.json
pub fn read_claude_credentials() -> Result<ClaudeCredentials> {
    let path = claude_credentials_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let file: ClaudeCredentialsFile = serde_json::from_str(&content)
        .with_context(|| "Failed to parse Claude credentials JSON")?;
    let oauth = file
        .claude_ai_oauth
        .context("Missing 'claudeAiOauth' in credentials file")?;
    let token = oauth
        .access_token
        .context("Missing 'accessToken' in credentials")?;
    if token.is_empty() {
        anyhow::bail!("Empty access token in Claude credentials");
    }
    Ok(ClaudeCredentials { access_token: token })
}

fn claude_credentials_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".claude")
        .join(".credentials.json")
}

// --- Codex credentials ---

#[derive(Deserialize)]
struct CodexAuthFile {
    tokens: Option<CodexTokens>,
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
}

#[derive(Deserialize)]
struct CodexTokens {
    access_token: Option<String>,
    #[allow(dead_code)]
    refresh_token: Option<String>,
    #[allow(dead_code)]
    id_token: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug)]
pub struct CodexCredentials {
    pub access_token: String,
    pub account_id: Option<String>,
}

/// Read Codex OAuth credentials from ~/.codex/auth.json
pub fn read_codex_credentials() -> Result<CodexCredentials> {
    let path = codex_auth_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let file: CodexAuthFile = serde_json::from_str(&content)
        .with_context(|| "Failed to parse Codex auth JSON")?;

    // Try tokens first, fall back to OPENAI_API_KEY
    if let Some(tokens) = file.tokens {
        let token = tokens
            .access_token
            .context("Missing 'access_token' in Codex tokens")?;
        if token.is_empty() {
            anyhow::bail!("Empty access token in Codex credentials");
        }
        return Ok(CodexCredentials {
            access_token: token,
            account_id: tokens.account_id,
        });
    }

    if let Some(api_key) = file.openai_api_key {
        if !api_key.is_empty() {
            return Ok(CodexCredentials {
                access_token: api_key,
                account_id: None,
            });
        }
    }

    anyhow::bail!("No valid credentials found in Codex auth file")
}

fn codex_auth_path() -> PathBuf {
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".codex")
        })
        .join("auth.json")
}

/// Decode a JWT payload without signature verification.
/// Returns the decoded JSON claims as a serde_json::Value.
pub fn decode_jwt_claims(token: &str) -> Result<serde_json::Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid JWT: expected 3 parts, got {}", parts.len());
    }
    let payload = parts[1];
    use base64::Engine;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .with_context(|| "Failed to base64url decode JWT payload")?;
    let claims: serde_json::Value =
        serde_json::from_slice(&decoded).with_context(|| "Failed to parse JWT payload as JSON")?;
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_jwt_claims_valid_token() {
        // Header: {"alg":"HS256","typ":"JWT"}
        // Payload: {"sub":"1234567890","name":"Test User","iat":1516239022}
        // Signature: dummy
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IlRlc3QgVXNlciIsImlhdCI6MTUxNjIzOTAyMn0.dummy_sig_ignored";
        let claims = decode_jwt_claims(token).unwrap();
        assert_eq!(claims["sub"], "1234567890");
        assert_eq!(claims["name"], "Test User");
    }

    #[test]
    fn decode_jwt_claims_wrong_part_count() {
        let err = decode_jwt_claims("only.two").unwrap_err();
        assert!(err.to_string().contains("expected 3 parts"));
    }

    #[test]
    fn decode_jwt_claims_invalid_base64() {
        let err = decode_jwt_claims("header.!!!invalid!!!.sig").unwrap_err();
        assert!(err.to_string().contains("base64") || err.to_string().contains("decode"));
    }

    #[test]
    fn decode_jwt_claims_invalid_json_payload() {
        // base64url encode "not json"
        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"not json");
        let token = format!("header.{}.sig", payload);
        let err = decode_jwt_claims(&token).unwrap_err();
        assert!(err.to_string().contains("JSON") || err.to_string().contains("parse"));
    }

    #[test]
    fn read_claude_credentials_missing_file() {
        // The default path won't exist in CI; we just verify it errors gracefully.
        // We can't easily test the happy path without a real credentials file.
        // This test ensures the error is descriptive rather than a panic.
        let result = read_claude_credentials();
        // In CI the file won't exist, so we expect an error about reading the file.
        // If it happens to exist on the dev machine, that's fine too.
        if result.is_err() {
            let msg = result.unwrap_err().to_string();
            assert!(!msg.is_empty(), "Error message should not be empty");
        }
    }

    #[test]
    fn read_codex_credentials_uses_codex_home_env() {
        std::env::set_var("CODEX_HOME", "/nonexistent/path");
        let result = read_codex_credentials();
        std::env::remove_var("CODEX_HOME");
        // Should fail trying to read /nonexistent/path/auth.json
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("nonexistent") || msg.contains("Failed to read"));
    }

    #[test]
    fn parse_claude_credentials_json_happy_path() {
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "tok_abc123"
            }
        }"#;
        let file: ClaudeCredentialsFile = serde_json::from_str(json).unwrap();
        let oauth = file.claude_ai_oauth.unwrap();
        assert_eq!(oauth.access_token.unwrap(), "tok_abc123");
    }

    #[test]
    fn parse_claude_credentials_missing_oauth_key() {
        let json = r#"{}"#;
        let file: ClaudeCredentialsFile = serde_json::from_str(json).unwrap();
        assert!(file.claude_ai_oauth.is_none());
    }

    #[test]
    fn parse_codex_auth_tokens_path() {
        let json = r#"{
            "tokens": {
                "access_token": "at_xyz",
                "refresh_token": "rt_xyz",
                "id_token": "it_xyz",
                "account_id": "acc_123"
            }
        }"#;
        let file: CodexAuthFile = serde_json::from_str(json).unwrap();
        let tokens = file.tokens.unwrap();
        assert_eq!(tokens.access_token.unwrap(), "at_xyz");
        assert_eq!(tokens.account_id.unwrap(), "acc_123");
    }

    #[test]
    fn parse_codex_auth_api_key_fallback() {
        let json = r#"{"OPENAI_API_KEY": "sk-abc"}"#;
        let file: CodexAuthFile = serde_json::from_str(json).unwrap();
        assert!(file.tokens.is_none());
        assert_eq!(file.openai_api_key.unwrap(), "sk-abc");
    }
}
