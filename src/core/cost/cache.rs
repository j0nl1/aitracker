use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const CACHE_VERSION: u64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRecord {
    pub provider: String,
    pub model: String,
    pub date: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub mtime_ms: u64,
    pub size: u64,
    pub parsed_bytes: u64,
    #[serde(default)]
    pub records: Vec<CachedRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostCache {
    #[serde(default)]
    pub version: u64,
    pub files: HashMap<String, FileEntry>,
}

impl Default for CostCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            files: HashMap::new(),
        }
    }
}

fn cache_path() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".cache")
        });
    base.join("ait").join("cost-cache.json")
}

impl CostCache {
    /// Load the cache from disk, or return an empty cache.
    /// Clears all entries if the on-disk version doesn't match CACHE_VERSION.
    pub fn load() -> Self {
        let path = cache_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let cache: Self = serde_json::from_str(&content).unwrap_or_default();
                if cache.version != CACHE_VERSION {
                    return Self::default();
                }
                cache
            }
            Err(_) => Self::default(),
        }
    }

    /// Check if a warm (non-empty, correct version) cache exists on disk.
    pub fn has_warm_cache() -> bool {
        let path = cache_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(cache) => cache.version == CACHE_VERSION && !cache.files.is_empty(),
                Err(_) => false,
            },
            Err(_) => false,
        }
    }

    /// Save the cache to disk.
    pub fn save(&self) -> Result<()> {
        let path = cache_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create cache directory: {}", parent.display()))?;
        }
        let json = serde_json::to_string(self).context("Failed to serialize cost cache")?;
        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write cache to {}", path.display()))?;
        Ok(())
    }

    /// Check if a file is unchanged (mtime + size match).
    pub fn is_unchanged(&self, path: &str, mtime_ms: u64, size: u64) -> bool {
        if let Some(entry) = self.files.get(path) {
            entry.mtime_ms == mtime_ms && entry.size == size
        } else {
            false
        }
    }

    /// Get the byte offset to resume parsing from for an incremental read.
    /// Returns 0 if file is new or has been modified.
    pub fn resume_offset(&self, path: &str, mtime_ms: u64) -> u64 {
        if let Some(entry) = self.files.get(path) {
            // If mtime changed, we must re-read from start
            // But if only size grew (file appended), we can resume
            if entry.mtime_ms == mtime_ms {
                entry.parsed_bytes
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Get cached records for a file (used when file is unchanged).
    pub fn get_records(&self, path: &str) -> Vec<CachedRecord> {
        self.files
            .get(path)
            .map(|e| e.records.clone())
            .unwrap_or_default()
    }

    /// Update the cache entry for a file, including parsed records.
    pub fn update(
        &mut self,
        path: &str,
        mtime_ms: u64,
        size: u64,
        parsed_bytes: u64,
        records: Vec<CachedRecord>,
    ) {
        self.files.insert(
            path.to_string(),
            FileEntry {
                mtime_ms,
                size,
                parsed_bytes,
                records,
            },
        );
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_default_empty() {
        let cache = CostCache::default();
        assert!(cache.files.is_empty());
    }

    #[test]
    fn cache_unchanged_check() {
        let mut cache = CostCache::default();
        cache.update("/test/file.jsonl", 1000, 5000, 5000, vec![]);
        assert!(cache.is_unchanged("/test/file.jsonl", 1000, 5000));
        assert!(!cache.is_unchanged("/test/file.jsonl", 1001, 5000));
        assert!(!cache.is_unchanged("/test/file.jsonl", 1000, 6000));
        assert!(!cache.is_unchanged("/test/other.jsonl", 1000, 5000));
    }

    #[test]
    fn cache_resume_offset() {
        let mut cache = CostCache::default();
        cache.update("/test/file.jsonl", 1000, 5000, 3000, vec![]);
        // Same mtime -> resume from parsed_bytes
        assert_eq!(cache.resume_offset("/test/file.jsonl", 1000), 3000);
        // Different mtime -> start from 0
        assert_eq!(cache.resume_offset("/test/file.jsonl", 1001), 0);
        // Unknown file -> 0
        assert_eq!(cache.resume_offset("/test/other.jsonl", 1000), 0);
    }

    #[test]
    fn cache_clear() {
        let mut cache = CostCache::default();
        cache.update("/test/file.jsonl", 1000, 5000, 5000, vec![]);
        assert!(!cache.files.is_empty());
        cache.files.clear();
        assert!(cache.files.is_empty());
    }

    #[test]
    fn cache_roundtrip_json() {
        let mut cache = CostCache::default();
        cache.update("/test/file.jsonl", 1000, 5000, 3000, vec![]);
        let json = serde_json::to_string(&cache).unwrap();
        let loaded: CostCache = serde_json::from_str(&json).unwrap();
        assert!(loaded.is_unchanged("/test/file.jsonl", 1000, 5000));
    }
}
