use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use super::helpers::{
    extract_response_message_text, file_stamp_from_metadata, max_datetime, normalize_event_name,
    parse_rfc3339_to_utc, parse_session_meta, read_last_bytes, thread_id_from_filename, truncate,
};
use super::{
    CachedHistory, CachedSessionMeta, CachedSummary, DataCollector, HistoryRow, RAW_LOG_LIMIT,
    RUNNING_ACTIVITY_GRACE_SECONDS, SESSION_TAIL_BYTES, SessionFile, SessionKind, SessionMeta,
    SessionMetaMaps, SessionSummary, TimelineEventKind, push_raw_log, push_timeline_event,
    timeline_kind_for_event, update_status_reason,
};

impl DataCollector {
    pub(super) fn load_session_meta(
        &mut self,
        session_files: &HashMap<String, SessionFile>,
    ) -> SessionMetaMaps {
        let mut thread_to_cwd = HashMap::new();
        let mut parent_to_children: HashMap<String, Vec<String>> = HashMap::new();

        for (thread_id, file) in session_files {
            if let Some(meta) = self.parse_session_meta_cached(thread_id, file) {
                thread_to_cwd.insert(meta.thread_id.clone(), meta.cwd.clone());
                if meta.kind == SessionKind::Subagent && !meta.parent_thread_id.is_empty() {
                    parent_to_children
                        .entry(meta.parent_thread_id.clone())
                        .or_default()
                        .push(meta.thread_id.clone());
                }
            }
        }

        for children in parent_to_children.values_mut() {
            children.sort();
        }

        SessionMetaMaps {
            thread_to_cwd,
            parent_to_children,
        }
    }

    pub(super) fn index_session_files(&self) -> HashMap<String, SessionFile> {
        let mut latest: HashMap<String, SessionFile> = HashMap::new();
        for entry in WalkDir::new(&self.sessions_dir)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(thread_id) = thread_id_from_filename(path) else {
                continue;
            };
            let stamp = entry
                .metadata()
                .ok()
                .map(file_stamp_from_metadata)
                .unwrap_or_else(super::FileStamp::zero);

            match latest.get(&thread_id) {
                Some(existing) if existing.stamp >= stamp => {}
                _ => {
                    latest.insert(
                        thread_id,
                        SessionFile {
                            path: path.to_path_buf(),
                            stamp,
                        },
                    );
                }
            }
        }
        latest
    }

    pub(super) fn history_last_user(&mut self) -> HashMap<String, String> {
        let stamp = match fs::metadata(&self.history_path) {
            Ok(meta) => file_stamp_from_metadata(meta),
            Err(_) => return HashMap::new(),
        };
        if let Some(cache) = &self.history_cache
            && cache.stamp == stamp
        {
            return cache.by_thread.clone();
        }

        let Ok(tail) = read_last_bytes(&self.history_path, 1_200_000) else {
            return HashMap::new();
        };
        let mut by_thread = HashMap::new();
        for line in tail.lines() {
            let Ok(row) = serde_json::from_str::<HistoryRow>(line) else {
                continue;
            };
            by_thread.insert(row.session_id, truncate(&row.text, 280).replace('\n', " "));
        }

        self.history_cache = Some(CachedHistory {
            stamp,
            by_thread: by_thread.clone(),
        });
        by_thread
    }

    pub(super) fn parse_session_summary(&self, session_file: &Path) -> Result<SessionSummary> {
        let mut summary = SessionSummary::unknown();
        let tail = read_last_bytes(session_file, SESSION_TAIL_BYTES)?;
        let mut saw_turn_terminal = false;
        let mut saw_turn_activity = false;

        for line in tail.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            push_raw_log(
                &mut summary,
                truncate(line, RAW_LOG_LIMIT.saturating_mul(8)).replace('\n', " "),
            );
            let record_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !matches!(record_type, "event_msg" | "response_item" | "turn_context") {
                continue;
            }

            let timestamp = value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339_to_utc);
            summary.last_update = max_datetime(summary.last_update, timestamp);

            if record_type == "turn_context" {
                saw_turn_activity = true;
                summary.last_event = "turn_context".to_string();
                push_timeline_event(
                    &mut summary,
                    crate::types::SessionTimelineEvent {
                        timestamp,
                        kind: TimelineEventKind::System,
                        title: "turn context".to_string(),
                        detail: "context synchronized".to_string(),
                        emphasis: false,
                    },
                );
                continue;
            }

            if record_type == "response_item" {
                saw_turn_activity |= handle_response_item(&mut summary, &value, timestamp);
                continue;
            }

            let (turn_terminal, turn_activity) = handle_event_msg(&mut summary, &value, timestamp);
            saw_turn_terminal |= turn_terminal;
            saw_turn_activity |= turn_activity;
        }

        if !summary.in_turn
            && !saw_turn_terminal
            && saw_turn_activity
            && summary.last_update.is_some_and(|ts| {
                Utc::now().signed_duration_since(ts).num_seconds() <= RUNNING_ACTIVITY_GRACE_SECONDS
            })
        {
            summary.in_turn = true;
        }

        update_status_reason(&mut summary);
        Ok(summary)
    }

    pub(super) fn parse_session_summary_cached(
        &mut self,
        thread_id: &str,
        session_file: &SessionFile,
    ) -> SessionSummary {
        if let Some(cache) = self.summary_cache.get(thread_id)
            && cache.stamp == session_file.stamp
        {
            return cache.summary.clone();
        }

        let summary = self
            .parse_session_summary(&session_file.path)
            .unwrap_or_else(|_| SessionSummary::unknown());
        self.summary_cache.insert(
            thread_id.to_string(),
            CachedSummary {
                stamp: session_file.stamp,
                summary: summary.clone(),
            },
        );
        summary
    }

    pub(super) fn parse_session_meta_cached(
        &mut self,
        thread_id: &str,
        session_file: &SessionFile,
    ) -> Option<SessionMeta> {
        if let Some(cache) = self.session_meta_cache.get(thread_id)
            && cache.stamp == session_file.stamp
        {
            return cache.meta.clone();
        }

        let meta = parse_session_meta(&session_file.path, thread_id);
        self.session_meta_cache.insert(
            thread_id.to_string(),
            CachedSessionMeta {
                stamp: session_file.stamp,
                meta: meta.clone(),
            },
        );
        meta
    }
}

fn handle_response_item(
    summary: &mut SessionSummary,
    value: &Value,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    let payload = value.get("payload").unwrap_or(&Value::Null);
    let payload_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if payload_type == "message" {
        let role = payload
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if role == "assistant" {
            if let Some(message) = extract_response_message_text(payload) {
                summary.last_agent = truncate(&message, 280).replace('\n', " ");
                summary.last_agent_ts = timestamp;
                summary.last_event = "agent_message".to_string();
                push_timeline_event(
                    summary,
                    crate::types::SessionTimelineEvent {
                        timestamp,
                        kind: TimelineEventKind::Agent,
                        title: "assistant".to_string(),
                        detail: truncate(&message, 220).replace('\n', " "),
                        emphasis: false,
                    },
                );
            }
        } else if role == "user"
            && let Some(message) = extract_response_message_text(payload)
        {
            summary.last_user = truncate(&message, 280).replace('\n', " ");
            summary.last_user_ts = timestamp;
            summary.last_event = "user_message".to_string();
            summary.waiting_on_user_input = false;
            push_timeline_event(
                summary,
                crate::types::SessionTimelineEvent {
                    timestamp,
                    kind: TimelineEventKind::User,
                    title: "user".to_string(),
                    detail: truncate(&message, 220).replace('\n', " "),
                    emphasis: false,
                },
            );
        }
        return false;
    }

    if super::helpers::is_response_turn_activity(payload_type) {
        summary.last_event = payload_type.to_string();
        push_timeline_event(
            summary,
            crate::types::SessionTimelineEvent {
                timestamp,
                kind: TimelineEventKind::Tool,
                title: payload_type.to_string(),
                detail: "turn activity".to_string(),
                emphasis: false,
            },
        );
        return true;
    }

    false
}

fn handle_event_msg(
    summary: &mut SessionSummary,
    value: &Value,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
) -> (bool, bool) {
    let payload = value.get("payload").unwrap_or(&Value::Null);
    let event_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if event_type != "token_count" && !event_type.is_empty() {
        summary.last_event = normalize_event_name(event_type);
    }

    match event_type {
        "user_message" => {
            if let Some(message) = payload.get("message").and_then(Value::as_str) {
                summary.last_user = truncate(message, 280).replace('\n', " ");
                push_timeline_event(
                    summary,
                    crate::types::SessionTimelineEvent {
                        timestamp,
                        kind: TimelineEventKind::User,
                        title: "user".to_string(),
                        detail: truncate(message, 220).replace('\n', " "),
                        emphasis: false,
                    },
                );
            }
            summary.last_user_ts = timestamp;
            summary.waiting_on_user_input = false;
            (false, false)
        }
        "agent_message" => {
            if let Some(message) = payload.get("message").and_then(Value::as_str) {
                summary.last_agent = truncate(message, 280).replace('\n', " ");
                push_timeline_event(
                    summary,
                    crate::types::SessionTimelineEvent {
                        timestamp,
                        kind: TimelineEventKind::Agent,
                        title: "assistant".to_string(),
                        detail: truncate(message, 220).replace('\n', " "),
                        emphasis: false,
                    },
                );
            }
            summary.last_agent_ts = timestamp;
            (false, true)
        }
        "task_started" | "turn_started" => {
            summary.in_turn = true;
            summary.waiting_on_approval = false;
            summary.waiting_on_user_input = false;
            push_named_event(summary, timestamp, event_type, "task started", true);
            (false, true)
        }
        "task_complete" | "turn_complete" => {
            if let Some(message) = payload.get("last_agent_message").and_then(Value::as_str) {
                summary.last_agent = truncate(message, 280).replace('\n', " ");
                summary.last_agent_ts = timestamp;
            }
            summary.in_turn = false;
            summary.waiting_on_approval = false;
            summary.waiting_on_user_input = false;
            push_named_event(summary, timestamp, event_type, "task completed", true);
            (true, false)
        }
        "turn_aborted" | "error" | "stream_error" => {
            summary.in_turn = false;
            summary.waiting_on_approval = false;
            summary.waiting_on_user_input = false;
            push_named_event(summary, timestamp, event_type, event_type, true);
            (true, false)
        }
        "exec_approval_request" => {
            summary.in_turn = true;
            summary.waiting_on_approval = true;
            push_named_event(summary, timestamp, event_type, "approval requested", true);
            (false, true)
        }
        "request_user_input" => {
            summary.in_turn = true;
            summary.waiting_on_user_input = true;
            push_named_event(summary, timestamp, event_type, "user input requested", true);
            (false, true)
        }
        "exec_command_begin" => {
            summary.in_turn = true;
            summary.waiting_on_approval = false;
            push_named_event(summary, timestamp, event_type, "command started", false);
            (false, true)
        }
        "exec_command_end"
        | "exec_command_output_delta"
        | "terminal_interaction"
        | "agent_reasoning"
        | "agent_reasoning_delta"
        | "agent_reasoning_raw_content"
        | "agent_reasoning_raw_content_delta"
        | "agent_reasoning_section_break"
        | "agent_message_delta"
        | "item_started"
        | "item_completed"
        | "dynamic_tool_call_request"
        | "dynamic_tool_call_response"
        | "web_search_begin"
        | "web_search_end"
        | "mcp_tool_call_begin"
        | "mcp_tool_call_end"
        | "token_count" => {
            if event_type != "token_count" {
                push_named_event(summary, timestamp, event_type, event_type, false);
            }
            (false, true)
        }
        _ => (false, false),
    }
}

fn push_named_event(
    summary: &mut SessionSummary,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
    event_type: &str,
    detail: &str,
    emphasis: bool,
) {
    push_timeline_event(
        summary,
        crate::types::SessionTimelineEvent {
            timestamp,
            kind: timeline_kind_for_event(event_type),
            title: normalize_event_name(event_type),
            detail: detail.to_string(),
            emphasis,
        },
    );
}
