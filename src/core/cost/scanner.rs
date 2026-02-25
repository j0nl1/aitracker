use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, Seek, SeekFrom};
use std::path::PathBuf;

use crate::core::cost::cache::{CachedRecord, CostCache};
use crate::core::cost::pricing;
use crate::core::models::cost::{CostSummary, DailyReport, TokenCostSnapshot};
use crate::core::providers::Provider;

/// Convert ParsedRecords to CachedRecords for cache storage.
fn to_cached(records: &[ParsedRecord]) -> Vec<CachedRecord> {
    records
        .iter()
        .map(|r| CachedRecord {
            provider: r.provider.id().to_string(),
            model: r.model.clone(),
            date: r.date.format("%Y-%m-%d").to_string(),
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            cache_read_tokens: r.cache_read_tokens,
            cache_creation_tokens: r.cache_creation_tokens,
        })
        .collect()
}

/// Convert CachedRecords back to ParsedRecords.
fn from_cached(cached: Vec<CachedRecord>) -> Vec<ParsedRecord> {
    cached
        .into_iter()
        .filter_map(|c| {
            let provider = Provider::from_id(&c.provider)?;
            let date = NaiveDate::parse_from_str(&c.date, "%Y-%m-%d").ok()?;
            Some(ParsedRecord {
                provider,
                model: c.model,
                input_tokens: c.input_tokens,
                output_tokens: c.output_tokens,
                cache_read_tokens: c.cache_read_tokens,
                cache_creation_tokens: c.cache_creation_tokens,
                date,
            })
        })
        .collect()
}

// ── Claude JSONL structs ──────────────────────────────────────────────

#[derive(Deserialize)]
struct JsonlMessage {
    model: Option<String>,
    usage: Option<JsonlUsage>,
    id: Option<String>,
}

#[derive(Deserialize)]
struct JsonlUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct JsonlLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    message: Option<JsonlMessage>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    timestamp: Option<String>,
}

// ── Codex JSONL structs ───────────────────────────────────────────────

#[derive(Deserialize)]
struct CodexLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    timestamp: Option<String>,
    payload: Option<CodexPayload>,
}

#[derive(Deserialize)]
struct CodexPayload {
    #[serde(rename = "type")]
    payload_type: Option<String>,
    model: Option<String>,
    info: Option<CodexTokenInfo>,
}

#[derive(Deserialize)]
struct CodexTokenInfo {
    total_token_usage: Option<CodexTokenUsage>,
    last_token_usage: Option<CodexTokenUsage>,
    model_name: Option<String>,
}

#[derive(Deserialize)]
struct CodexTokenUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
}

// ── Shared record ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ParsedRecord {
    provider: Provider,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    date: NaiveDate,
}

// ── Claude file discovery ─────────────────────────────────────────────

fn discover_claude_files() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".claude"));
    }

    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        roots.push(PathBuf::from(config_dir));
    }

    if let Some(config_home) = dirs::config_dir() {
        roots.push(config_home.join("claude"));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for root in roots {
        let projects_dir = root.join("projects");
        if !projects_dir.is_dir() {
            continue;
        }
        if let Ok(projects) = std::fs::read_dir(&projects_dir) {
            for project_entry in projects.flatten() {
                let project_path = project_entry.path();
                if !project_path.is_dir() {
                    continue;
                }

                // Level 1: {project-dir}/*.jsonl
                if let Ok(entries) = std::fs::read_dir(&project_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                        {
                            files.push(path);
                        }
                    }
                }

                // Level 2: {project-dir}/{uuid-dir}/subagents/*.jsonl
                if let Ok(subdirs) = std::fs::read_dir(&project_path) {
                    for subdir in subdirs.flatten() {
                        let subagents_dir = subdir.path().join("subagents");
                        if !subagents_dir.is_dir() {
                            continue;
                        }
                        if let Ok(sa_entries) = std::fs::read_dir(&subagents_dir) {
                            for sa_entry in sa_entries.flatten() {
                                let path = sa_entry.path();
                                if path.is_file()
                                    && path.extension().and_then(|e| e.to_str())
                                        == Some("jsonl")
                                {
                                    files.push(path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    files
}

// ── Codex file discovery ──────────────────────────────────────────────

fn discover_codex_files() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    // $CODEX_HOME/sessions/
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        roots.push(PathBuf::from(codex_home).join("sessions"));
    }

    // ~/.codex/sessions/ and ~/.codex/archived_sessions/
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".codex").join("sessions"));
        roots.push(home.join(".codex").join("archived_sessions"));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        collect_jsonl_recursive(&root, &mut files, 4); // YYYY/MM/DD depth + files
    }
    files
}

/// Recursively collect *.jsonl files up to `max_depth` levels deep.
fn collect_jsonl_recursive(dir: &PathBuf, files: &mut Vec<PathBuf>, max_depth: u32) {
    if max_depth == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            files.push(path);
        } else if path.is_dir() {
            collect_jsonl_recursive(&path, files, max_depth - 1);
        }
    }
}

// ── Claude parser ─────────────────────────────────────────────────────

/// Fast ASCII check: does this line look like it contains usage data?
fn is_candidate_line(line: &str) -> bool {
    line.contains("\"type\":\"assistant\"") && line.contains("\"usage\"")
}

/// Detect if a Claude log entry is actually Vertex AI traffic.
fn detect_vertex_ai(msg_id: &str, request_id: &str, model: &str) -> bool {
    msg_id.contains("_vrtx_")
        || request_id.contains("_vrtx_")
        || model.contains('@')
}

/// Parse a single Claude/Vertex AI JSONL file, optionally resuming from a byte offset.
fn parse_claude_file(
    path: &PathBuf,
    offset: u64,
) -> Result<(Vec<ParsedRecord>, u64)> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?;
    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

    let mut reader = std::io::BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset))?;
    }

    let mut records: Vec<ParsedRecord> = Vec::new();
    let mut dedup: HashMap<(String, String), usize> = HashMap::new();
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        let bytes_read = reader.read_line(&mut line_buf)?;
        if bytes_read == 0 {
            break;
        }

        let line = line_buf.trim();
        if line.is_empty() || !is_candidate_line(line) {
            continue;
        }

        let parsed: JsonlLine = match serde_json::from_str(line) {
            Ok(p) => p,
            Err(_) => continue,
        };

        if parsed.line_type.as_deref() != Some("assistant") {
            continue;
        }

        let message = match parsed.message {
            Some(m) => m,
            None => continue,
        };

        let model = match message.model {
            Some(m) => m,
            None => continue,
        };

        let usage = match message.usage {
            Some(u) => u,
            None => continue,
        };

        let date = parsed
            .timestamp
            .as_deref()
            .and_then(|ts| {
                chrono::DateTime::parse_from_rfc3339(ts)
                    .map(|dt| dt.date_naive())
                    .ok()
                    .or_else(|| NaiveDate::parse_from_str(&ts[..10], "%Y-%m-%d").ok())
            })
            .unwrap_or_else(|| chrono::Utc::now().date_naive());

        let msg_id = message.id.as_deref().unwrap_or("");
        let req_id = parsed.request_id.as_deref().unwrap_or("");

        let provider = if detect_vertex_ai(msg_id, req_id, &model) {
            Provider::VertexAi
        } else {
            Provider::Claude
        };

        let record = ParsedRecord {
            provider,
            model,
            input_tokens: usage.input_tokens.unwrap_or(0),
            output_tokens: usage.output_tokens.unwrap_or(0),
            cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
            cache_creation_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
            date,
        };

        let msg_id_owned = message.id.unwrap_or_default();
        let req_id_owned = parsed.request_id.unwrap_or_default();
        if !msg_id_owned.is_empty() || !req_id_owned.is_empty() {
            let key = (msg_id_owned, req_id_owned);
            if let Some(idx) = dedup.get(&key) {
                records[*idx] = record;
            } else {
                let idx = records.len();
                dedup.insert(key, idx);
                records.push(record);
            }
        } else {
            records.push(record);
        }
    }

    Ok((records, file_size))
}

// ── Codex parser ──────────────────────────────────────────────────────

/// Fast ASCII check for Codex JSONL lines.
fn is_codex_candidate(line: &str) -> bool {
    line.contains("\"token_count\"") || line.contains("\"turn_context\"")
}

/// Parse a single Codex JSONL session file.
fn parse_codex_file(
    path: &PathBuf,
    offset: u64,
) -> Result<(Vec<ParsedRecord>, u64)> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?;
    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

    let mut reader = std::io::BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset))?;
    }

    // total_token_usage is cumulative per session — only the LAST event per model
    // has the correct total. Using a HashMap ensures we keep only the final value.
    let mut last_per_model: HashMap<String, ParsedRecord> = HashMap::new();
    let mut current_model: Option<String> = None;
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        let bytes_read = reader.read_line(&mut line_buf)?;
        if bytes_read == 0 {
            break;
        }

        let line = line_buf.trim();
        if line.is_empty() || !is_codex_candidate(line) {
            continue;
        }

        let parsed: CodexLine = match serde_json::from_str(line) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let payload = match parsed.payload {
            Some(p) => p,
            None => continue,
        };

        // Track model from turn_context events
        if parsed.line_type.as_deref() == Some("turn_context") {
            if let Some(model) = payload.model {
                current_model = Some(model);
            }
            continue;
        }

        // Process token_count events
        if parsed.line_type.as_deref() != Some("event_msg") {
            continue;
        }
        if payload.payload_type.as_deref() != Some("token_count") {
            continue;
        }

        let info = match payload.info {
            Some(i) => i,
            None => continue,
        };

        // Prefer total_token_usage, fall back to last_token_usage
        let usage = match info.total_token_usage.or(info.last_token_usage) {
            Some(u) => u,
            None => continue,
        };

        // Determine model: info.model_name > current turn_context model
        let model = info
            .model_name
            .or_else(|| current_model.clone())
            .unwrap_or_else(|| "unknown-codex".to_string());

        let date = parsed
            .timestamp
            .as_deref()
            .and_then(|ts| {
                chrono::DateTime::parse_from_rfc3339(ts)
                    .map(|dt| dt.date_naive())
                    .ok()
                    .or_else(|| {
                        if ts.len() >= 10 {
                            NaiveDate::parse_from_str(&ts[..10], "%Y-%m-%d").ok()
                        } else {
                            None
                        }
                    })
            })
            .unwrap_or_else(|| chrono::Utc::now().date_naive());

        last_per_model.insert(
            model.clone(),
            ParsedRecord {
                provider: Provider::Codex,
                model,
                input_tokens: usage.input_tokens.unwrap_or(0),
                output_tokens: usage.output_tokens.unwrap_or(0),
                cache_read_tokens: usage.cached_input_tokens.unwrap_or(0),
                cache_creation_tokens: 0,
                date,
            },
        );
    }

    let records: Vec<ParsedRecord> = last_per_model.into_values().collect();
    Ok((records, file_size))
}

// ── Shared helpers ────────────────────────────────────────────────────

/// Get mtime as milliseconds since epoch.
fn file_mtime_ms(path: &PathBuf) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        })
        .unwrap_or(0)
}

/// Build a `CostSummary` from a set of records for a given date range.
fn build_summary(records: Vec<ParsedRecord>, days: u32, today: NaiveDate) -> CostSummary {
    // Group by date + model
    let mut date_model_map: HashMap<(NaiveDate, String), ParsedRecord> = HashMap::new();
    for record in records {
        let key = (record.date, record.model.clone());
        let entry = date_model_map.entry(key).or_insert(ParsedRecord {
            provider: record.provider,
            model: record.model.clone(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            date: record.date,
        });
        entry.input_tokens += record.input_tokens;
        entry.output_tokens += record.output_tokens;
        entry.cache_read_tokens += record.cache_read_tokens;
        entry.cache_creation_tokens += record.cache_creation_tokens;
    }

    let mut daily_map: HashMap<NaiveDate, Vec<TokenCostSnapshot>> = HashMap::new();
    let mut model_totals: HashMap<String, TokenCostSnapshot> = HashMap::new();

    for ((date, _model), record) in &date_model_map {
        let pricing_entry = pricing::lookup(&record.model);
        let (input_cost, output_cost, cache_read_cost, cache_creation_cost) =
            if let Some(p) = pricing_entry {
                pricing::calculate_cost(
                    p,
                    record.input_tokens,
                    record.output_tokens,
                    record.cache_read_tokens,
                    record.cache_creation_tokens,
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };

        let total_cost = input_cost + output_cost + cache_read_cost + cache_creation_cost;

        let snapshot = TokenCostSnapshot {
            model: record.model.clone(),
            input_tokens: record.input_tokens,
            output_tokens: record.output_tokens,
            cache_read_tokens: record.cache_read_tokens,
            cache_creation_tokens: record.cache_creation_tokens,
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
        };

        daily_map.entry(*date).or_default().push(snapshot.clone());

        let model_entry = model_totals
            .entry(record.model.clone())
            .or_insert(TokenCostSnapshot {
                model: record.model.clone(),
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                input_cost: 0.0,
                output_cost: 0.0,
                cache_read_cost: 0.0,
                cache_creation_cost: 0.0,
                total_cost: 0.0,
            });
        model_entry.input_tokens += record.input_tokens;
        model_entry.output_tokens += record.output_tokens;
        model_entry.cache_read_tokens += record.cache_read_tokens;
        model_entry.cache_creation_tokens += record.cache_creation_tokens;
        model_entry.input_cost += input_cost;
        model_entry.output_cost += output_cost;
        model_entry.cache_read_cost += cache_read_cost;
        model_entry.cache_creation_cost += cache_creation_cost;
        model_entry.total_cost += total_cost;
    }

    let mut daily: Vec<DailyReport> = daily_map
        .into_iter()
        .map(|(date, costs)| {
            let total_cost = costs.iter().map(|c| c.total_cost).sum();
            DailyReport {
                date,
                costs,
                total_cost,
            }
        })
        .collect();
    daily.sort_by(|a, b| b.date.cmp(&a.date));

    let mut by_model: Vec<TokenCostSnapshot> = model_totals.into_values().collect();
    by_model.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_cost: f64 = by_model.iter().map(|m| m.total_cost).sum();
    let today_cost: f64 = daily
        .iter()
        .find(|d| d.date == today)
        .map(|d| d.total_cost)
        .unwrap_or(0.0);

    CostSummary {
        total_cost,
        today_cost,
        days,
        by_model,
        daily,
    }
}

// ── Main scan entry point ─────────────────────────────────────────────

/// Scan all session files and build a cost summary per provider.
pub fn scan(days: u32) -> Result<HashMap<Provider, CostSummary>> {
    let mut cache = CostCache::load();

    let cutoff = chrono::Utc::now().date_naive() - chrono::Duration::days(days as i64);
    let today = chrono::Utc::now().date_naive();

    let mut all_records: Vec<ParsedRecord> = Vec::new();

    // ── Claude / Vertex AI files ──
    let claude_files = discover_claude_files();
    for file_path in &claude_files {
        let path_str = file_path.to_string_lossy().to_string();
        let mtime_ms = file_mtime_ms(file_path);
        let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

        if cache.is_unchanged(&path_str, mtime_ms, file_size) {
            let cached = cache.get_records(&path_str);
            if !cached.is_empty() {
                all_records.extend(from_cached(cached));
                continue;
            }
            // Empty records → stale entry, fall through to re-parse
        }

        let offset = cache.resume_offset(&path_str, mtime_ms);

        match parse_claude_file(file_path, offset) {
            Ok((records, parsed_bytes)) => {
                let cached = to_cached(&records);
                all_records.extend(records);
                cache.update(&path_str, mtime_ms, file_size, parsed_bytes, cached);
            }
            Err(_) => continue,
        }
    }

    // ── Codex files ──
    let codex_files = discover_codex_files();
    for file_path in &codex_files {
        let path_str = file_path.to_string_lossy().to_string();
        let mtime_ms = file_mtime_ms(file_path);
        let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

        if cache.is_unchanged(&path_str, mtime_ms, file_size) {
            let cached = cache.get_records(&path_str);
            if !cached.is_empty() {
                all_records.extend(from_cached(cached));
                continue;
            }
            // Empty records → stale entry, fall through to re-parse
        }

        let offset = cache.resume_offset(&path_str, mtime_ms);

        match parse_codex_file(file_path, offset) {
            Ok((records, parsed_bytes)) => {
                let cached = to_cached(&records);
                all_records.extend(records);
                cache.update(&path_str, mtime_ms, file_size, parsed_bytes, cached);
            }
            Err(_) => continue,
        }
    }

    // Filter to date range
    let all_records: Vec<ParsedRecord> = all_records
        .into_iter()
        .filter(|r| r.date >= cutoff)
        .collect();

    // Group records by provider
    let mut by_provider: HashMap<Provider, Vec<ParsedRecord>> = HashMap::new();
    for record in all_records {
        by_provider
            .entry(record.provider)
            .or_default()
            .push(record);
    }

    // Build a CostSummary per provider
    let mut result: HashMap<Provider, CostSummary> = HashMap::new();
    for (provider, records) in by_provider {
        result.insert(provider, build_summary(records, days, today));
    }

    let _ = cache.save();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_candidate_line_positive() {
        let line = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-5","usage":{"input_tokens":100}}}"#;
        assert!(is_candidate_line(line));
    }

    #[test]
    fn is_candidate_line_negative_no_type() {
        let line = r#"{"message":{"usage":{"input_tokens":100}}}"#;
        assert!(!is_candidate_line(line));
    }

    #[test]
    fn is_candidate_line_negative_no_usage() {
        let line = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-5"}}"#;
        assert!(!is_candidate_line(line));
    }

    #[test]
    fn deserialize_jsonl_line() {
        let json = r#"{
            "type": "assistant",
            "message": {
                "model": "claude-sonnet-4-5",
                "usage": {
                    "input_tokens": 1000,
                    "output_tokens": 200,
                    "cache_read_input_tokens": 500,
                    "cache_creation_input_tokens": 50
                },
                "id": "msg_123"
            },
            "requestId": "req_456",
            "timestamp": "2025-02-24T10:00:00Z"
        }"#;
        let parsed: JsonlLine = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.line_type.as_deref(), Some("assistant"));
        let msg = parsed.message.unwrap();
        assert_eq!(msg.model.as_deref(), Some("claude-sonnet-4-5"));
        let usage = msg.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.output_tokens, Some(200));
        assert_eq!(usage.cache_read_input_tokens, Some(500));
        assert_eq!(usage.cache_creation_input_tokens, Some(50));
    }

    #[test]
    fn parse_claude_file_with_temp_file() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("ait_test_scanner");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_session.jsonl");

        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-sonnet-4-5","usage":{{"input_tokens":1000,"output_tokens":200,"cache_read_input_tokens":500,"cache_creation_input_tokens":50}},"id":"msg_1"}},"requestId":"req_1","timestamp":"2025-02-24T10:00:00Z"}}"#).unwrap();
        writeln!(f, r#"{{"type":"user","message":{{"content":"hello"}}}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-sonnet-4-5","usage":{{"input_tokens":2000,"output_tokens":400}},"id":"msg_2"}},"requestId":"req_2","timestamp":"2025-02-24T11:00:00Z"}}"#).unwrap();
        drop(f);

        let (records, _) = parse_claude_file(&file_path, 0).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].provider, Provider::Claude);
        assert_eq!(records[0].input_tokens, 1000);
        assert_eq!(records[0].cache_read_tokens, 500);
        assert_eq!(records[1].input_tokens, 2000);
        assert_eq!(records[1].cache_read_tokens, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_session_files_layout() {
        let root = std::env::temp_dir().join("ait_test_discover");
        let _ = std::fs::remove_dir_all(&root);

        let project = root.join("projects").join("proj-abc");
        std::fs::create_dir_all(&project).unwrap();
        let main_session = project.join("aaaa-bbbb.jsonl");
        std::fs::File::create(&main_session).unwrap();

        let subagents = project.join("aaaa-bbbb").join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        let sub_session = subagents.join("cccc-dddd.jsonl");
        std::fs::File::create(&sub_session).unwrap();

        let _ = std::fs::File::create(project.join("memory.md"));

        // Manually replicate discovery logic for test isolation
        let projects_dir = root.join("projects");
        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(projects) = std::fs::read_dir(&projects_dir) {
            for project_entry in projects.flatten() {
                let project_path = project_entry.path();
                if !project_path.is_dir() {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(&project_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                        {
                            files.push(path);
                        }
                    }
                }
                if let Ok(subdirs) = std::fs::read_dir(&project_path) {
                    for subdir in subdirs.flatten() {
                        let sa_dir = subdir.path().join("subagents");
                        if !sa_dir.is_dir() {
                            continue;
                        }
                        if let Ok(sa_entries) = std::fs::read_dir(&sa_dir) {
                            for sa_entry in sa_entries.flatten() {
                                let path = sa_entry.path();
                                if path.is_file()
                                    && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                                {
                                    files.push(path);
                                }
                            }
                        }
                    }
                }
            }
        }

        assert_eq!(files.len(), 2);
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"aaaa-bbbb.jsonl".to_string()));
        assert!(names.contains(&"cccc-dddd.jsonl".to_string()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dedup_streaming_chunks() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("ait_test_dedup");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_dedup.jsonl");

        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-sonnet-4-5","usage":{{"input_tokens":100,"output_tokens":10}},"id":"msg_1"}},"requestId":"req_1","timestamp":"2025-02-24T10:00:00Z"}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-sonnet-4-5","usage":{{"input_tokens":100,"output_tokens":50}},"id":"msg_1"}},"requestId":"req_1","timestamp":"2025-02-24T10:00:00Z"}}"#).unwrap();
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-sonnet-4-5","usage":{{"input_tokens":100,"output_tokens":200}},"id":"msg_1"}},"requestId":"req_1","timestamp":"2025-02-24T10:00:00Z"}}"#).unwrap();
        drop(f);

        let (records, _) = parse_claude_file(&file_path, 0).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].output_tokens, 200);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Codex tests ───────────────────────────────────────────────────

    #[test]
    fn discover_codex_files_layout() {
        let root = std::env::temp_dir().join("ait_test_codex_discover");
        let _ = std::fs::remove_dir_all(&root);

        // Create sessions/2026/02/24/*.jsonl
        let day_dir = root.join("sessions").join("2026").join("02").join("24");
        std::fs::create_dir_all(&day_dir).unwrap();
        std::fs::File::create(day_dir.join("session-001.jsonl")).unwrap();
        std::fs::File::create(day_dir.join("session-002.jsonl")).unwrap();

        // Create a flat file in sessions root
        let sessions_dir = root.join("sessions");
        std::fs::File::create(sessions_dir.join("flat.jsonl")).unwrap();

        // Non-jsonl file
        std::fs::File::create(sessions_dir.join("readme.txt")).unwrap();

        // Collect using our recursive helper
        let mut files: Vec<PathBuf> = Vec::new();
        collect_jsonl_recursive(&sessions_dir, &mut files, 4);

        assert_eq!(files.len(), 3);
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"session-001.jsonl".to_string()));
        assert!(names.contains(&"session-002.jsonl".to_string()));
        assert!(names.contains(&"flat.jsonl".to_string()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_codex_file_token_usage() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("ait_test_codex_parse");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("codex_session.jsonl");

        let mut f = std::fs::File::create(&file_path).unwrap();
        // turn_context sets model
        writeln!(f, r#"{{"type":"turn_context","timestamp":"2026-02-24T10:00:00Z","payload":{{"model":"gpt-5.3-codex"}}}}"#).unwrap();
        // First cumulative token_count (intermediate — should be replaced)
        writeln!(f, r#"{{"type":"event_msg","timestamp":"2026-02-24T10:01:00Z","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":500,"output_tokens":100,"cached_input_tokens":200}},"model_name":"gpt-5.3-codex"}}}}}}"#).unwrap();
        // Second cumulative token_count for same model (final — should be kept)
        writeln!(f, r#"{{"type":"event_msg","timestamp":"2026-02-24T10:02:00Z","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":300,"output_tokens":50}}}}}}}}"#).unwrap();
        drop(f);

        let (records, _) = parse_codex_file(&file_path, 0).unwrap();
        // Both events are for "gpt-5.3-codex" — only the last one should survive
        assert_eq!(records.len(), 1);

        assert_eq!(records[0].provider, Provider::Codex);
        assert_eq!(records[0].model, "gpt-5.3-codex");
        assert_eq!(records[0].input_tokens, 300);
        assert_eq!(records[0].output_tokens, 50);
        assert_eq!(records[0].cache_read_tokens, 0);
        assert_eq!(records[0].cache_creation_tokens, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_codex_file_null_info_skipped() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("ait_test_codex_null");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("codex_null.jsonl");

        let mut f = std::fs::File::create(&file_path).unwrap();
        // token_count with null info should be skipped
        writeln!(f, r#"{{"type":"event_msg","timestamp":"2026-02-24T10:00:00Z","payload":{{"type":"token_count","info":null}}}}"#).unwrap();
        // Valid token_count
        writeln!(f, r#"{{"type":"event_msg","timestamp":"2026-02-24T10:01:00Z","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":100,"output_tokens":50}},"model_name":"gpt-5"}}}}}}"#).unwrap();
        drop(f);

        let (records, _) = parse_codex_file(&file_path, 0).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "gpt-5");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Vertex AI detection tests ─────────────────────────────────────

    #[test]
    fn vertex_ai_detection_vrtx_in_msg_id() {
        assert!(detect_vertex_ai("msg_vrtx_abc123", "", "claude-opus-4-5"));
        assert!(detect_vertex_ai("msg_01_vrtx_test", "", "claude-opus-4-5"));
    }

    #[test]
    fn vertex_ai_detection_vrtx_in_request_id() {
        assert!(detect_vertex_ai("", "req_vrtx_xyz", "claude-opus-4-5"));
    }

    #[test]
    fn vertex_ai_detection_at_in_model() {
        assert!(detect_vertex_ai("msg_123", "req_456", "claude-opus-4-5@20251101"));
    }

    #[test]
    fn vertex_ai_detection_normal_claude() {
        assert!(!detect_vertex_ai("msg_123", "req_456", "claude-opus-4-5"));
    }

    #[test]
    fn vertex_ai_in_claude_file() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("ait_test_vertex");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("vertex_test.jsonl");

        let mut f = std::fs::File::create(&file_path).unwrap();
        // Normal Claude entry
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-sonnet-4-5","usage":{{"input_tokens":1000,"output_tokens":200}},"id":"msg_1"}},"requestId":"req_1","timestamp":"2025-02-24T10:00:00Z"}}"#).unwrap();
        // Vertex AI entry (model has @)
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-opus-4-5@20251101","usage":{{"input_tokens":500,"output_tokens":100}},"id":"msg_2"}},"requestId":"req_2","timestamp":"2025-02-24T11:00:00Z"}}"#).unwrap();
        // Vertex AI entry (_vrtx_ in message id)
        writeln!(f, r#"{{"type":"assistant","message":{{"model":"claude-sonnet-4-5","usage":{{"input_tokens":300,"output_tokens":60}},"id":"msg_vrtx_3"}},"requestId":"req_3","timestamp":"2025-02-24T12:00:00Z"}}"#).unwrap();
        drop(f);

        let (records, _) = parse_claude_file(&file_path, 0).unwrap();
        assert_eq!(records.len(), 3);

        assert_eq!(records[0].provider, Provider::Claude);
        assert_eq!(records[1].provider, Provider::VertexAi);
        assert_eq!(records[2].provider, Provider::VertexAi);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
