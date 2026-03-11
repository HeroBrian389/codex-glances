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

use crate::types::{SessionTimelineEvent, TimelineEventKind};

pub use collector::DataCollector;
pub(crate) use registry::{
    add_workspace_registry_entry, default_workspace_registry_path, load_workspace_registry,
    toggle_workspace_pinned,
};

const SESSION_TAIL_BYTES: usize = 8_000_000;
const RUNNING_ACTIVITY_GRACE_SECONDS: i64 = 3_600;
const TIMELINE_EVENT_LIMIT: usize = 36;
const RAW_LOG_LIMIT: usize = 80;

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
pub(crate) struct ProcInfo {
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
pub(crate) struct SessionSummary {
    last_event: String,
    status_reason: String,
    last_user: String,
    last_agent: String,
    last_update: Option<DateTime<Utc>>,
    last_user_ts: Option<DateTime<Utc>>,
    last_agent_ts: Option<DateTime<Utc>>,
    in_turn: bool,
    waiting_on_approval: bool,
    waiting_on_user_input: bool,
    timeline: Vec<SessionTimelineEvent>,
    raw_log: Vec<String>,
}

impl SessionSummary {
    fn unknown() -> Self {
        Self {
            last_event: "-".to_string(),
            status_reason: "-".to_string(),
            last_user: "-".to_string(),
            last_agent: "-".to_string(),
            last_update: None,
            last_user_ts: None,
            last_agent_ts: None,
            in_turn: false,
            waiting_on_approval: false,
            waiting_on_user_input: false,
            timeline: Vec::new(),
            raw_log: Vec::new(),
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
pub(crate) struct SessionFile {
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
pub(crate) struct SessionMetaMaps {
    thread_to_cwd: HashMap<String, String>,
    parent_to_children: HashMap<String, Vec<String>>,
}

fn push_timeline_event(summary: &mut SessionSummary, event: SessionTimelineEvent) {
    summary.timeline.push(event);
    if summary.timeline.len() > TIMELINE_EVENT_LIMIT {
        let overflow = summary.timeline.len() - TIMELINE_EVENT_LIMIT;
        summary.timeline.drain(0..overflow);
    }
}

fn push_raw_log(summary: &mut SessionSummary, line: String) {
    summary.raw_log.push(line);
    if summary.raw_log.len() > RAW_LOG_LIMIT {
        let overflow = summary.raw_log.len() - RAW_LOG_LIMIT;
        summary.raw_log.drain(0..overflow);
    }
}

fn update_status_reason(summary: &mut SessionSummary) {
    summary.status_reason = if summary.waiting_on_approval {
        "waiting for command approval".to_string()
    } else if summary.waiting_on_user_input {
        "waiting for user input".to_string()
    } else if summary.in_turn {
        if summary.last_event == "-" {
            "session is active".to_string()
        } else {
            format!("active: {}", summary.last_event)
        }
    } else if summary.last_event != "-" {
        format!("last event: {}", summary.last_event)
    } else {
        "-".to_string()
    };
}

fn timeline_kind_for_event(event_type: &str) -> TimelineEventKind {
    match event_type {
        "user_message" => TimelineEventKind::User,
        "agent_message" | "request_user_input" => TimelineEventKind::Agent,
        "task_started"
        | "task_complete"
        | "turn_started"
        | "turn_complete"
        | "turn_aborted"
        | "exec_approval_request"
        | "error"
        | "stream_error" => TimelineEventKind::Status,
        "exec_command_begin"
        | "exec_command_end"
        | "mcp_tool_call_begin"
        | "mcp_tool_call_end"
        | "web_search_begin"
        | "web_search_end"
        | "dynamic_tool_call_request"
        | "dynamic_tool_call_response"
        | "item_started"
        | "item_completed" => TimelineEventKind::Tool,
        _ => TimelineEventKind::System,
    }
}
