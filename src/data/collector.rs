use crate::types::{SessionRow, SessionStatus};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::helpers::{
    branch_from_head_marker, extract_declared_var, file_stamp_from_metadata,
    is_codex_exec_subcommand, is_thread_id, looks_like_user_action_needed,
    process_looks_like_codex, read_git_head_marker, read_process_env,
    thread_id_from_snapshot_filename,
};
use super::{
    CachedBranch, CachedHistory, CachedSessionMeta, CachedSummary, FileStamp, ProcCandidate,
    ProcInfo, ScreenSession, SessionFile, SessionSummary,
};

pub struct DataCollector {
    pub(super) snapshots_dir: PathBuf,
    pub(super) sessions_dir: PathBuf,
    pub(super) history_path: PathBuf,
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
            summary_cache: HashMap::new(),
            session_meta_cache: HashMap::new(),
            history_cache: None,
            branch_cache: HashMap::new(),
            screen_thread_cache: HashMap::new(),
        })
    }

    pub fn collect(&mut self) -> Result<Vec<SessionRow>> {
        let screens = self
            .list_screen_sessions()
            .context("failed to list screen sessions")?;
        let codex_by_sty = self.codex_processes_by_sty();
        let snapshot_threads = self.snapshot_thread_map();
        let session_files = self.index_session_files();
        let session_meta = self.load_session_meta(&session_files);
        let history_last_user = self.history_last_user();

        let mut rows = Vec::with_capacity(screens.len());
        for session in screens {
            let process_info = codex_by_sty.get(&session.id);
            let mut cwd = process_info
                .map(|p| p.cwd.clone())
                .unwrap_or_else(|| "-".to_string());
            let thread_id = self.resolve_screen_thread(
                &session.id,
                process_info,
                &snapshot_threads,
                &session_files,
            );

            if cwd == "-"
                && !thread_id.is_empty()
                && let Some(found_cwd) = session_meta.thread_to_cwd.get(&thread_id)
            {
                cwd = found_cwd.clone();
            }

            let branch = self.git_branch_for_cwd(&cwd);
            let summary = self.row_summary(&thread_id, &session_files);
            let last_user = self.resolve_last_user(&summary, &history_last_user, &thread_id);
            let status = self.resolve_status(&summary, &thread_id, process_info);
            let needs_attention = status == SessionStatus::WaitingInput;
            let scheduled_follow_ups =
                self.scheduled_follow_up_count(&thread_id, &session_meta, &session_files);

            rows.push(SessionRow {
                screen_id: session.id,
                screen_name: session.name,
                branch,
                cwd,
                thread_id,
                status,
                needs_attention,
                scheduled_follow_ups,
                last_event: summary.last_event,
                last_user,
                last_agent: summary.last_agent,
                last_update: summary.last_update,
            });
        }

        rows.sort_by(|a, b| a.screen_id.cmp(&b.screen_id));
        Ok(rows)
    }

    fn row_summary(
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

    fn resolve_last_user(
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

    fn resolve_status(
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

    pub(super) fn status_for_thread(
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

    pub(super) fn resolve_screen_thread(
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

    pub(super) fn scheduled_follow_up_count(
        &mut self,
        thread_id: &str,
        session_meta: &super::SessionMetaMaps,
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

    fn list_screen_sessions(&self) -> Result<Vec<ScreenSession>> {
        let output = Command::new("screen")
            .arg("-ls")
            .output()
            .context("failed to run screen -ls")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut sessions = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || (!trimmed.contains("(Detached)") && !trimmed.contains("(Attached)"))
            {
                continue;
            }

            let Some(first_token) = trimmed.split_whitespace().next() else {
                continue;
            };
            let Some(first_char) = first_token.chars().next() else {
                continue;
            };
            if !first_token.contains('.') || !first_char.is_ascii_digit() {
                continue;
            }

            let name = first_token
                .split_once('.')
                .map(|(_, suffix)| suffix.to_string())
                .unwrap_or_else(|| first_token.to_string());
            sessions.push(ScreenSession {
                id: first_token.to_string(),
                name,
            });
        }

        Ok(sessions)
    }

    fn codex_processes_by_sty(&self) -> HashMap<String, ProcInfo> {
        let mut grouped: HashMap<String, Vec<ProcCandidate>> = HashMap::new();
        let output = match Command::new("ps")
            .args(["-eo", "pid,args", "--no-headers"])
            .output()
        {
            Ok(o) => o,
            Err(_) => return HashMap::new(),
        };

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }

            let mut parts = trimmed.split_whitespace();
            let Some(pid_token) = parts.next() else {
                continue;
            };
            let Ok(pid) = pid_token.parse::<u32>() else {
                continue;
            };
            let args = parts.collect::<Vec<_>>().join(" ");
            if !process_looks_like_codex(&args) {
                continue;
            }

            let env = read_process_env(pid);
            let sty = env.get("STY").cloned().unwrap_or_default();
            if sty.is_empty() {
                continue;
            }

            let cwd = fs::read_link(format!("/proc/{pid}/cwd"))
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "-".to_string());
            let thread_id = env
                .get("CODEX_THREAD_ID")
                .cloned()
                .filter(|id| is_thread_id(id))
                .unwrap_or_default();

            grouped.entry(sty).or_default().push(ProcCandidate {
                pid,
                args,
                cwd,
                thread_id,
            });
        }

        grouped
            .into_iter()
            .filter_map(|(sty, mut candidates)| {
                if candidates.is_empty() {
                    return None;
                }
                candidates.sort_by_key(|candidate| {
                    (is_codex_exec_subcommand(&candidate.args), candidate.pid)
                });
                let primary = candidates.first()?.clone();
                let fallback_thread_id = candidates
                    .iter()
                    .find_map(|candidate| {
                        (!candidate.thread_id.is_empty()).then(|| candidate.thread_id.clone())
                    })
                    .unwrap_or_default();

                Some((
                    sty,
                    ProcInfo {
                        cwd: primary.cwd,
                        thread_id: primary.thread_id,
                        fallback_thread_id,
                        has_exec_process: candidates
                            .iter()
                            .any(|candidate| is_codex_exec_subcommand(&candidate.args)),
                    },
                ))
            })
            .collect()
    }

    pub(super) fn snapshot_thread_map(&self) -> HashMap<String, String> {
        let mut latest: HashMap<String, (FileStamp, String)> = HashMap::new();
        let entries = match fs::read_dir(&self.snapshots_dir) {
            Ok(entries) => entries,
            Err(_) => return HashMap::new(),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sh") {
                continue;
            }

            let stamp = entry
                .metadata()
                .ok()
                .map(file_stamp_from_metadata)
                .unwrap_or_else(FileStamp::zero);
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };

            let sty = extract_declared_var(&content, "STY");
            if sty.is_empty() {
                continue;
            }

            let thread_id = extract_declared_var(&content, "CODEX_THREAD_ID");
            let thread_id = if is_thread_id(&thread_id) {
                thread_id
            } else if let Some(from_name) = thread_id_from_snapshot_filename(&path) {
                from_name
            } else {
                continue;
            };

            match latest.get(&sty) {
                Some((prev_ts, _)) if *prev_ts >= stamp => {}
                _ => {
                    latest.insert(sty, (stamp, thread_id));
                }
            }
        }

        latest.into_iter().map(|(k, (_, v))| (k, v)).collect()
    }

    fn git_branch_for_cwd(&mut self, cwd: &str) -> String {
        if cwd == "-" {
            return "-".to_string();
        }
        let path = Path::new(cwd);
        if !path.exists() {
            return "-".to_string();
        }

        let Some(head_marker) = read_git_head_marker(path) else {
            self.branch_cache.remove(cwd);
            return "-".to_string();
        };
        if let Some(cache) = self.branch_cache.get(cwd)
            && cache.head_marker == head_marker
        {
            return cache.branch.clone();
        }

        let branch = branch_from_head_marker(&head_marker);
        self.branch_cache.insert(
            cwd.to_string(),
            CachedBranch {
                head_marker,
                branch: branch.clone(),
            },
        );
        branch
    }
}
