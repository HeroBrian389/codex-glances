mod collector;
mod helpers;
mod parsing;
mod registry;

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

pub use collector::DataCollector;
pub(crate) use registry::{
    add_workspace_registry_entry, default_workspace_registry_path, load_workspace_registry,
    toggle_workspace_pinned,
};

const SESSION_TAIL_BYTES: usize = 8_000_000;
const RUNNING_ACTIVITY_GRACE_SECONDS: i64 = 3_600;

#[derive(Debug, Clone)]
struct ScreenSession {
    id: String,
    name: String,
}

#[derive(Debug, Clone)]
struct ProcCandidate {
    pid: u32,
    args: String,
    cwd: String,
    thread_id: String,
}

#[derive(Debug, Clone)]
struct ProcInfo {
    cwd: String,
    thread_id: String,
    fallback_thread_id: String,
    has_exec_process: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SessionKind {
    Cli,
    Subagent,
    Unknown,
}

#[derive(Debug, Clone)]
struct SessionSummary {
    last_event: String,
    last_user: String,
    last_agent: String,
    last_update: Option<DateTime<Utc>>,
    last_user_ts: Option<DateTime<Utc>>,
    last_agent_ts: Option<DateTime<Utc>>,
    in_turn: bool,
    waiting_on_approval: bool,
    waiting_on_user_input: bool,
}

impl SessionSummary {
    fn unknown() -> Self {
        Self {
            last_event: "-".to_string(),
            last_user: "-".to_string(),
            last_agent: "-".to_string(),
            last_update: None,
            last_user_ts: None,
            last_agent_ts: None,
            in_turn: false,
            waiting_on_approval: false,
            waiting_on_user_input: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct HistoryRow {
    session_id: String,
    text: String,
}

#[derive(Debug, Clone)]
struct SessionMeta {
    thread_id: String,
    cwd: String,
    kind: SessionKind,
    parent_thread_id: String,
}

#[derive(Debug, Clone)]
struct SessionFile {
    path: PathBuf,
    stamp: FileStamp,
}

#[derive(Debug, Clone)]
struct CachedSummary {
    stamp: FileStamp,
    summary: SessionSummary,
}

#[derive(Debug, Clone)]
struct CachedSessionMeta {
    stamp: FileStamp,
    meta: Option<SessionMeta>,
}

#[derive(Debug, Clone)]
struct CachedHistory {
    stamp: FileStamp,
    by_thread: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct FileStamp {
    modified_ns: u128,
    len: u64,
}

impl FileStamp {
    fn zero() -> Self {
        Self {
            modified_ns: 0,
            len: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct CachedBranch {
    head_marker: String,
    branch: String,
}

#[derive(Debug, Default)]
struct SessionMetaMaps {
    thread_to_cwd: HashMap<String, String>,
    parent_to_children: HashMap<String, Vec<String>>,
}
