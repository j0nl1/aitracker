use std::path::PathBuf;

/// Check if a binary exists in PATH. Returns the full path if found.
pub fn which(binary: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(binary))
            .find(|p| p.is_file())
    })
}
