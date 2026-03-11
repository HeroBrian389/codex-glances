mod processes;
mod workspace;

use crate::types::{DashboardData, SessionRow, SessionStatus, WorkspaceRow};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::load_workspace_registry;
use super::{
    CachedBranch, CachedHistory, CachedSessionMeta, CachedSummary, ProcInfo, ScreenSession,
    SessionFile,
};

#[derive(Debug, Clone)]
struct WorkspaceCandidate {
    key: String,
    path: String,
    fallback_name: Option<String>,
    last_seen: Option<DateTime<Utc>>,
}

pub struct DataCollector {
    pub(super) snapshots_dir: PathBuf,
    pub(super) sessions_dir: PathBuf,
    pub(super) history_path: PathBuf,
    pub(super) workspace_registry_path: PathBuf,
    pub(super) summary_cache: HashMap<String, CachedSummary>,
    pub(super) session_meta_cache: HashMap<String, CachedSessionMeta>,
    pub(super) history_cache: Option<CachedHistory>,
    pub(super) branch_cache: HashMap<String, CachedBranch>,
    pub(super) screen_thread_cache: HashMap<String, String>,
}

impl DataCollector {
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME").context("HOME is not set")?;
        let codex_dir = Path::new(&home).join(".codex");

        Ok(Self {
            snapshots_dir: codex_dir.join("shell_snapshots"),
            sessions_dir: codex_dir.join("sessions"),
            history_path: codex_dir.join("history.jsonl"),
            workspace_registry_path: super::default_workspace_registry_path()?,
            summary_cache: HashMap::new(),
            session_meta_cache: HashMap::new(),
            history_cache: None,
            branch_cache: HashMap::new(),
            screen_thread_cache: HashMap::new(),
        })
    }

    pub fn collect(&mut self) -> Result<DashboardData> {
        let screens = self
            .list_screen_sessions()
            .context("failed to list screen sessions")?;
        let codex_by_sty = self.codex_processes_by_sty();
        let snapshot_threads = self.snapshot_thread_map();
        let session_files = self.index_session_files();
        let session_meta = self.load_session_meta(&session_files);
        let history_last_user = self.history_last_user();
        let registry = load_workspace_registry(&self.workspace_registry_path);

        let mut active_sessions_by_workspace: HashMap<String, Vec<SessionRow>> = HashMap::new();
        let mut workspace_candidates = self.historical_workspace_candidates(&session_files);

        for session in screens {
            let session_row = self.build_session_row(
                &session,
                codex_by_sty.get(&session.id),
                &snapshot_threads,
                &session_files,
                &session_meta,
                &history_last_user,
            );

            let workspace_candidate = self.workspace_candidate_for_session(&session_row);
            workspace_candidates
                .entry(workspace_candidate.key.clone())
                .and_modify(|candidate| {
                    candidate.last_seen = super::helpers::max_datetime(
                        candidate.last_seen,
                        workspace_candidate.last_seen,
                    );
                    if candidate.fallback_name.is_none() {
                        candidate.fallback_name = workspace_candidate.fallback_name.clone();
                    }
                })
                .or_insert(workspace_candidate.clone());

            active_sessions_by_workspace
                .entry(workspace_candidate.key)
                .or_default()
                .push(session_row);
        }

        let registry_by_path = registry
            .workspaces
            .into_iter()
            .map(|entry| (entry.path.clone(), entry))
            .collect::<HashMap<_, _>>();

        let mut workspaces = workspace_candidates
            .into_values()
            .map(|candidate| {
                let mut sessions = active_sessions_by_workspace
                    .remove(&candidate.key)
                    .unwrap_or_default();
                self.sort_sessions(&mut sessions);
                self.build_workspace_row(candidate, sessions, &registry_by_path)
            })
            .collect::<Vec<_>>();

        self.sort_workspaces(&mut workspaces);
        Ok(DashboardData { workspaces })
    }

    fn build_session_row(
        &mut self,
        session: &ScreenSession,
        process_info: Option<&ProcInfo>,
        snapshot_threads: &HashMap<String, String>,
        session_files: &HashMap<String, SessionFile>,
        session_meta: &super::SessionMetaMaps,
        history_last_user: &HashMap<String, String>,
    ) -> SessionRow {
        let mut cwd = process_info
            .map(|p| p.cwd.clone())
            .unwrap_or_else(|| "-".to_string());
        let thread_id =
            self.resolve_screen_thread(&session.id, process_info, snapshot_threads, session_files);

        if cwd == "-"
            && !thread_id.is_empty()
            && let Some(found_cwd) = session_meta.thread_to_cwd.get(&thread_id)
        {
            cwd = found_cwd.clone();
        }

        let branch = self.git_branch_for_cwd(&cwd);
        let summary = self.row_summary(&thread_id, session_files);
        let last_user = self.resolve_last_user(&summary, history_last_user, &thread_id);
        let status = self.resolve_status(&summary, &thread_id, process_info);
        let needs_attention = status == SessionStatus::WaitingInput;
        let scheduled_follow_ups =
            self.scheduled_follow_up_count(&thread_id, session_meta, session_files);

        SessionRow {
            screen_id: session.id.clone(),
            screen_name: session.name.clone(),
            branch,
            cwd: cwd.clone(),
            thread_id,
            status,
            needs_attention,
            scheduled_follow_ups,
            last_event: summary.last_event,
            status_reason: summary.status_reason,
            last_user,
            last_agent: summary.last_agent,
            last_update: summary.last_update,
            timeline: summary.timeline,
            raw_log: summary.raw_log,
        }
    }

    fn build_workspace_row(
        &mut self,
        candidate: WorkspaceCandidate,
        sessions: Vec<SessionRow>,
        registry_by_path: &HashMap<String, crate::types::StoredWorkspace>,
    ) -> WorkspaceRow {
        let customization = registry_by_path.get(&candidate.path);
        let last_update = sessions
            .iter()
            .fold(candidate.last_seen, |current, session| {
                super::helpers::max_datetime(current, session.last_update)
            });
        let branch_label = self.workspace_branch_label(&candidate.path, &sessions);
        let display_name = workspace_display_name(
            &candidate.path,
            candidate.fallback_name.as_deref(),
            customization.and_then(|entry| entry.display_name.as_deref()),
        );
        let waiting_sessions = sessions
            .iter()
            .filter(|session| session.status == SessionStatus::WaitingInput)
            .count();
        let running_sessions = sessions
            .iter()
            .filter(|session| session.status == SessionStatus::Running)
            .count();
        let follow_ups = sessions
            .iter()
            .map(|session| session.scheduled_follow_ups)
            .sum::<usize>();
        let summary_session = sessions.first();

        WorkspaceRow {
            key: candidate.key,
            path: candidate.path,
            display_name,
            branch_label,
            pinned: customization.is_some_and(|entry| entry.pinned),
            tags: customization
                .map(|entry| entry.tags.clone())
                .unwrap_or_default(),
            session_count: sessions.len(),
            waiting_sessions,
            running_sessions,
            follow_ups,
            last_update,
            last_user: summary_session
                .map(|session| session.last_user.clone())
                .unwrap_or_else(|| "-".to_string()),
            last_agent: summary_session
                .map(|session| session.last_agent.clone())
                .unwrap_or_else(|| "-".to_string()),
            sessions,
        }
    }
}

fn is_live_screen(status: SessionStatus) -> bool {
    matches!(status, SessionStatus::Running | SessionStatus::WaitingInput)
}

fn compare_screen_rows(left: &SessionRow, right: &SessionRow) -> std::cmp::Ordering {
    let left_live = left.needs_attention || is_live_screen(left.status);
    let right_live = right.needs_attention || is_live_screen(right.status);

    left_live
        .cmp(&right_live)
        .then_with(|| right.last_update.cmp(&left.last_update))
        .then_with(|| left.status.rank().cmp(&right.status.rank()))
        .then_with(|| {
            left.screen_name
                .to_lowercase()
                .cmp(&right.screen_name.to_lowercase())
        })
}

fn workspace_display_name(
    path: &str,
    fallback_name: Option<&str>,
    custom_display_name: Option<&str>,
) -> String {
    if let Some(name) = custom_display_name.filter(|name| !name.trim().is_empty()) {
        return name.to_string();
    }

    if path == "-" {
        return fallback_name.unwrap_or("Unlinked session").to_string();
    }

    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}
