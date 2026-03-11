use super::{DataCollector, WorkspaceCandidate, compare_screen_rows};
use crate::types::{SessionRow, SessionStatus, WorkspaceRow};
use std::collections::{BTreeSet, HashMap};

use super::super::helpers::{
    is_thread_id, looks_like_user_action_needed, stamp_datetime, workspace_key_for_path,
};
use super::super::{ProcInfo, SessionFile, SessionSummary};
use crate::data::load_workspace_registry;

impl DataCollector {
    pub(super) fn row_summary(
        &mut self,
        thread_id: &str,
        session_files: &HashMap<String, SessionFile>,
    ) -> SessionSummary {
        if thread_id.is_empty() {
            return SessionSummary::unknown();
        }

        session_files
            .get(thread_id)
            .map(|file| self.parse_session_summary_cached(thread_id, file))
            .unwrap_or_else(SessionSummary::unknown)
    }

    pub(super) fn resolve_last_user(
        &self,
        summary: &SessionSummary,
        history_last_user: &HashMap<String, String>,
        thread_id: &str,
    ) -> String {
        if summary.last_user != "-" {
            summary.last_user.clone()
        } else if let Some(history_msg) = history_last_user.get(thread_id) {
            history_msg.clone()
        } else {
            "-".to_string()
        }
    }

    pub(super) fn resolve_status(
        &self,
        summary: &SessionSummary,
        thread_id: &str,
        process_info: Option<&ProcInfo>,
    ) -> SessionStatus {
        let has_actionable_agent_prompt = looks_like_user_action_needed(&summary.last_agent)
            && !summary.in_turn
            && summary.last_agent_ts.is_some()
            && summary
                .last_agent_ts
                .zip(summary.last_user_ts)
                .is_none_or(|(agent, user)| agent > user);
        let has_exec_process = process_info.is_some_and(|p| p.has_exec_process);

        let mut status = if thread_id.is_empty() {
            if has_exec_process {
                SessionStatus::Running
            } else {
                SessionStatus::Unknown
            }
        } else {
            self.status_for_thread(summary, has_actionable_agent_prompt, has_exec_process)
        };

        if status == SessionStatus::Idle && summary.last_event == "-" && process_info.is_none() {
            status = SessionStatus::Unknown;
        }

        status
    }

    pub(crate) fn status_for_thread(
        &self,
        summary: &SessionSummary,
        has_actionable_agent_prompt: bool,
        has_exec_process: bool,
    ) -> SessionStatus {
        if summary.waiting_on_approval
            || summary.waiting_on_user_input
            || has_actionable_agent_prompt
        {
            SessionStatus::WaitingInput
        } else if summary.in_turn || has_exec_process {
            SessionStatus::Running
        } else if summary.last_event == "-" {
            SessionStatus::Unknown
        } else {
            SessionStatus::Idle
        }
    }

    pub(crate) fn resolve_screen_thread(
        &mut self,
        screen_id: &str,
        process_info: Option<&ProcInfo>,
        snapshot_threads: &HashMap<String, String>,
        session_files: &HashMap<String, SessionFile>,
    ) -> String {
        let process_thread = process_info
            .map(|p| p.thread_id.clone())
            .filter(|id| is_thread_id(id));
        let process_fallback_thread = process_info
            .map(|p| p.fallback_thread_id.clone())
            .filter(|id| is_thread_id(id));
        let snapshot_thread = snapshot_threads
            .get(screen_id)
            .cloned()
            .filter(|id| is_thread_id(id));
        let cached_thread = self
            .screen_thread_cache
            .get(screen_id)
            .cloned()
            .filter(|id| session_files.contains_key(id));

        let thread_id = process_thread
            .or(process_fallback_thread)
            .or(snapshot_thread)
            .or(cached_thread)
            .unwrap_or_default();

        if thread_id.is_empty() {
            self.screen_thread_cache.remove(screen_id);
        } else {
            self.screen_thread_cache
                .insert(screen_id.to_string(), thread_id.clone());
        }

        thread_id
    }

    pub(crate) fn scheduled_follow_up_count(
        &mut self,
        thread_id: &str,
        session_meta: &super::super::SessionMetaMaps,
        session_files: &HashMap<String, SessionFile>,
    ) -> usize {
        let Some(children) = session_meta.parent_to_children.get(thread_id) else {
            return 0;
        };

        children
            .iter()
            .filter(|child_id| {
                let summary = self.row_summary(child_id, session_files);
                let status = self.status_for_thread(
                    &summary,
                    looks_like_user_action_needed(&summary.last_agent),
                    false,
                );
                matches!(status, SessionStatus::Running | SessionStatus::WaitingInput)
                    || summary.last_event == "-"
            })
            .count()
    }

    pub(super) fn historical_workspace_candidates(
        &mut self,
        session_files: &HashMap<String, SessionFile>,
    ) -> HashMap<String, WorkspaceCandidate> {
        let mut candidates = HashMap::new();

        for (thread_id, session_file) in session_files {
            let Some(meta) = self.parse_session_meta_cached(thread_id, session_file) else {
                continue;
            };
            if meta.cwd.trim().is_empty() {
                continue;
            }

            let key = workspace_key_for_path(&meta.cwd);
            let candidate = WorkspaceCandidate {
                key: key.clone(),
                path: key.clone(),
                fallback_name: None,
                last_seen: stamp_datetime(session_file.stamp),
            };
            candidates
                .entry(key)
                .and_modify(|existing: &mut WorkspaceCandidate| {
                    existing.last_seen = super::super::helpers::max_datetime(
                        existing.last_seen,
                        candidate.last_seen,
                    );
                })
                .or_insert(candidate);
        }

        let registry = load_workspace_registry(&self.workspace_registry_path);
        for entry in registry.workspaces {
            candidates
                .entry(entry.path.clone())
                .or_insert_with(|| WorkspaceCandidate {
                    key: entry.path.clone(),
                    path: entry.path,
                    fallback_name: None,
                    last_seen: None,
                });
        }

        candidates
    }

    pub(super) fn workspace_candidate_for_session(
        &self,
        session: &SessionRow,
    ) -> WorkspaceCandidate {
        if session.cwd == "-" {
            return WorkspaceCandidate {
                key: format!("screen:{}", session.screen_id),
                path: "-".to_string(),
                fallback_name: Some(session.screen_name.clone()),
                last_seen: session.last_update,
            };
        }

        let path = workspace_key_for_path(&session.cwd);
        WorkspaceCandidate {
            key: path.clone(),
            path,
            fallback_name: None,
            last_seen: session.last_update,
        }
    }

    pub(super) fn workspace_branch_label(&mut self, path: &str, sessions: &[SessionRow]) -> String {
        let branches = sessions
            .iter()
            .map(|session| session.branch.as_str())
            .filter(|branch| *branch != "-")
            .collect::<BTreeSet<_>>();

        match branches.len() {
            0 => self.git_branch_for_cwd(path),
            1 => branches
                .iter()
                .next()
                .map(|branch| (*branch).to_string())
                .unwrap_or_else(|| "-".to_string()),
            count => format!("{count} branches"),
        }
    }

    pub(super) fn sort_sessions(&self, sessions: &mut [SessionRow]) {
        sessions.sort_by(compare_screen_rows);
    }

    pub(super) fn sort_workspaces(&self, workspaces: &mut [WorkspaceRow]) {
        workspaces.sort_by(|left, right| {
            let left_key = (
                !left.pinned,
                left.waiting_sessions == 0,
                left.running_sessions == 0,
                right.last_update < left.last_update,
                left.display_name.to_lowercase(),
            );
            let right_key = (
                !right.pinned,
                right.waiting_sessions == 0,
                right.running_sessions == 0,
                left.last_update < right.last_update,
                right.display_name.to_lowercase(),
            );
            left_key.cmp(&right_key)
        });
    }
}
