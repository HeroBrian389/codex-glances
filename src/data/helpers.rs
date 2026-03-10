use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::{FileStamp, SessionKind, SessionMeta};

pub(super) fn parse_session_meta(path: &Path, thread_id: &str) -> Option<SessionMeta> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    for (line_idx, line) in reader.lines().enumerate() {
        if line_idx > 160 {
            break;
        }
        let Ok(line) = line else {
            continue;
        };
        if !line.contains("\"type\":\"session_meta\"") {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = value.get("payload")?;
        let source = payload.get("source");
        let (kind, parent_thread_id) = match source {
            Some(Value::String(raw)) if raw == "cli" => (SessionKind::Cli, String::new()),
            Some(Value::Object(_)) => {
                let parent_thread_id = source
                    .and_then(|value| value.get("subagent"))
                    .and_then(|value| value.get("thread_spawn"))
                    .and_then(|value| value.get("parent_thread_id"))
                    .and_then(Value::as_str)
                    .filter(|candidate| is_thread_id(candidate))
                    .unwrap_or_default()
                    .to_string();
                (SessionKind::Subagent, parent_thread_id)
            }
            _ => (SessionKind::Unknown, String::new()),
        };
        return Some(SessionMeta {
            thread_id: thread_id.to_string(),
            cwd: payload.get("cwd")?.as_str()?.to_string(),
            kind,
            parent_thread_id,
        });
    }
    None
}

pub(super) fn read_process_env(pid: u32) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(raw) = fs::read(format!("/proc/{pid}/environ")) else {
        return out;
    };

    for kv in raw.split(|b| *b == 0u8) {
        if kv.is_empty() {
            continue;
        }
        let Ok(kv_str) = std::str::from_utf8(kv) else {
            continue;
        };
        if let Some((k, v)) = kv_str.split_once('=') {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

pub(super) fn extract_declared_var(content: &str, key: &str) -> String {
    let prefix = format!("declare -x {key}=\"");
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(&prefix)
            && let Some(end) = rest.find('"')
        {
            return rest[..end].to_string();
        }
    }
    String::new()
}

pub(super) fn thread_id_from_snapshot_filename(path: &Path) -> Option<String> {
    let stem = path.file_name()?.to_str()?.strip_suffix(".sh")?;
    is_thread_id(stem).then(|| stem.to_string())
}

pub(super) fn thread_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_name()?.to_str()?.strip_suffix(".jsonl")?;
    if stem.len() < 36 {
        return None;
    }
    let candidate = &stem[stem.len() - 36..];
    is_thread_id(candidate).then(|| candidate.to_string())
}

pub(super) fn is_thread_id(candidate: &str) -> bool {
    if candidate.len() != 36 {
        return false;
    }
    candidate.chars().enumerate().all(|(idx, c)| {
        if matches!(idx, 8 | 13 | 18 | 23) {
            c == '-'
        } else {
            c.is_ascii_hexdigit()
        }
    })
}

pub(super) fn parse_rfc3339_to_utc(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

pub(super) fn extract_response_message_text(payload: &Value) -> Option<String> {
    let content = payload.get("content")?.as_array()?;
    let mut parts = Vec::new();
    for item in content {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
        let text = match item_type {
            "output_text" | "input_text" => item.get("text").and_then(Value::as_str),
            _ => None,
        };
        if let Some(text) = text
            && !text.trim().is_empty()
        {
            parts.push(text.trim().to_string());
        }
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

pub(super) fn looks_like_user_action_needed(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("i need your direction")
        || lower.contains("choose one")
        || lower.contains("waiting for your input")
        || lower.contains("how would you like to proceed")
        || lower.contains("can you confirm")
        || lower.contains("please approve")
        || lower.contains("please confirm")
}

pub(super) fn process_looks_like_codex(args: &str) -> bool {
    args.split_whitespace().any(|token| {
        token == "codex"
            || Path::new(token)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "codex")
    })
}

pub(super) fn is_codex_exec_subcommand(args: &str) -> bool {
    args.split_whitespace().any(|token| token == "exec")
}

pub(super) fn is_response_turn_activity(payload_type: &str) -> bool {
    matches!(
        payload_type,
        "function_call"
            | "function_call_output"
            | "reasoning"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "web_search_call"
    )
}

pub(super) fn normalize_event_name(event_type: &str) -> String {
    match event_type {
        "turn_started" => "task_started".to_string(),
        "turn_complete" => "task_complete".to_string(),
        _ => event_type.to_string(),
    }
}

pub(super) fn max_datetime(
    current: Option<DateTime<Utc>>,
    candidate: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (current, candidate) {
        (Some(existing), Some(next)) => Some(existing.max(next)),
        (None, Some(next)) => Some(next),
        (Some(existing), None) => Some(existing),
        (None, None) => None,
    }
}

pub(super) fn file_stamp_from_metadata(meta: fs::Metadata) -> FileStamp {
    let modified_ns = meta
        .modified()
        .ok()
        .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    FileStamp {
        modified_ns,
        len: meta.len(),
    }
}

pub(crate) fn stamp_datetime(stamp: FileStamp) -> Option<DateTime<Utc>> {
    let seconds = (stamp.modified_ns / 1_000_000_000) as i64;
    DateTime::<Utc>::from_timestamp(seconds, 0)
}

pub(super) fn read_git_head_marker(repo_path: &Path) -> Option<String> {
    let git_dir = resolve_git_dir(repo_path)?;
    fs::read_to_string(git_dir.join("HEAD"))
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

pub(crate) fn find_git_root(path: &Path) -> Option<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        fs::canonicalize(path).ok()?
    };

    absolute
        .ancestors()
        .find(|candidate| resolve_git_dir(candidate).is_some())
        .map(Path::to_path_buf)
}

pub(crate) fn normalize_workspace_path(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    Ok(find_git_root(&canonical).unwrap_or(canonical))
}

pub(crate) fn workspace_key_for_path(raw_path: &str) -> String {
    let path = Path::new(raw_path);
    find_git_root(path)
        .unwrap_or_else(|| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn resolve_git_dir(repo_path: &Path) -> Option<PathBuf> {
    let git_path = repo_path.join(".git");
    if git_path.is_dir() {
        return Some(git_path);
    }

    let content = fs::read_to_string(&git_path).ok()?;
    let git_dir = content.strip_prefix("gitdir: ")?.trim();
    let git_dir_path = Path::new(git_dir);
    if git_dir_path.is_absolute() {
        Some(git_dir_path.to_path_buf())
    } else {
        Some(repo_path.join(git_dir_path))
    }
}

pub(super) fn branch_from_head_marker(head_marker: &str) -> String {
    if let Some(reference) = head_marker.strip_prefix("ref: ") {
        return reference
            .strip_prefix("refs/heads/")
            .unwrap_or(reference)
            .to_string();
    }
    truncate(head_marker, 12)
}

pub(super) fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = String::new();
    for (idx, c) in text.chars().enumerate() {
        if idx >= max_chars.saturating_sub(1) {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

pub(super) fn read_last_bytes(path: &Path, max_bytes: usize) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(String::new());
    }

    let offset = len.saturating_sub(max_bytes as u64);
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if offset > 0
        && let Some(pos) = bytes.iter().position(|b| *b == b'\n')
    {
        bytes = bytes[pos + 1..].to_vec();
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
