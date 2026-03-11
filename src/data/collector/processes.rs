use super::DataCollector;
use crate::data::helpers::{
    branch_from_head_marker, extract_declared_var, file_stamp_from_metadata,
    is_codex_exec_subcommand, is_thread_id, process_looks_like_codex, read_git_head_marker,
    read_process_env, thread_id_from_snapshot_filename,
};
use crate::data::{CachedBranch, FileStamp, ProcCandidate, ProcInfo, ScreenSession};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

impl DataCollector {
    pub(super) fn list_screen_sessions(&self) -> Result<Vec<ScreenSession>> {
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

    pub(super) fn codex_processes_by_sty(&self) -> HashMap<String, ProcInfo> {
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

    pub(crate) fn snapshot_thread_map(&self) -> HashMap<String, String> {
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

    pub(super) fn git_branch_for_cwd(&mut self, cwd: &str) -> String {
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
