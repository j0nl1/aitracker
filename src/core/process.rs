use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

/// Run a command with arguments and a timeout, returning stdout as a String.
pub async fn run_command(cmd: &str, args: &[&str], timeout: Duration) -> Result<String> {
    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new(cmd)
            .args(args)
            .output(),
    )
    .await
    .context(format!("Command `{}` timed out", cmd))?
    .context(format!("Failed to execute `{}`", cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "`{}` exited with {}: {}",
            cmd,
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context(format!("Non-UTF8 output from `{}`", cmd))?;
    Ok(stdout.trim().to_string())
}

/// Check if a binary exists in PATH. Returns the full path if found.
pub fn which(binary: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(binary))
            .find(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_existing_binary() {
        // "ls" should exist on any Linux system
        assert!(which("ls").is_some());
    }

    #[test]
    fn which_returns_none_for_nonexistent() {
        assert!(which("totally_nonexistent_binary_xyz").is_none());
    }

    #[tokio::test]
    async fn run_command_echo() {
        let result = run_command("echo", &["hello"], Duration::from_secs(5)).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn run_command_failure() {
        let result = run_command("false", &[], Duration::from_secs(5)).await;
        assert!(result.is_err());
    }
}
