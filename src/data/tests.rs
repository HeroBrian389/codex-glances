use super::SessionKind;
use super::collector::DataCollector;
use super::helpers::{
    file_stamp_from_metadata, looks_like_user_action_needed, parse_rfc3339_to_utc,
    parse_session_meta, process_looks_like_codex,
};
use crate::types::SessionStatus;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "codex_glances_test_{}_{}_{}",
            label,
            std::process::id(),
            nonce
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, content: &str) {
    let mut file = fs::File::create(path).expect("create file");
    file.write_all(content.as_bytes()).expect("write file");
}

fn test_collector(root: &Path) -> DataCollector {
    DataCollector {
        snapshots_dir: root.join("shell_snapshots"),
        sessions_dir: root.join("sessions"),
        history_path: root.join("history.jsonl"),
        workspace_registry_path: root.join("workspaces.json"),
        summary_cache: HashMap::new(),
        session_meta_cache: HashMap::new(),
        history_cache: None,
        branch_cache: HashMap::new(),
        screen_thread_cache: HashMap::new(),
    }
}

impl DataCollector {
    pub(crate) fn empty_for_test() -> Self {
        let root = std::env::temp_dir().join("codex_glances_empty_for_test");
        test_collector(&root)
    }
}

#[test]
fn snapshot_thread_map_uses_snapshot_filename_thread_id() {
    let dir = TempDirGuard::new("snapshot_map");
    let snapshots_dir = dir.path.join("shell_snapshots");
    let sessions_dir = dir.path.join("sessions");
    fs::create_dir_all(&snapshots_dir).expect("create snapshots dir");
    fs::create_dir_all(&sessions_dir).expect("create sessions dir");

    let older_thread = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let newer_thread = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    write_file(
        &snapshots_dir.join(format!("{older_thread}.sh")),
        "declare -x STY=\"1234.s1\"\ndeclare -x CODEX_THREAD_ID=\"legacy\"\n",
    );
    std::thread::sleep(std::time::Duration::from_millis(1200));
    write_file(
        &snapshots_dir.join(format!("{newer_thread}.sh")),
        "declare -x STY=\"1234.s1\"\n# no CODEX_THREAD_ID line\n",
    );

    let mapping = test_collector(&dir.path).snapshot_thread_map();
    assert_eq!(mapping.get("1234.s1"), Some(&newer_thread.to_string()));
}

#[test]
fn snapshot_thread_map_prefers_declared_codex_thread_id() {
    let dir = TempDirGuard::new("snapshot_declared_thread");
    let snapshots_dir = dir.path.join("shell_snapshots");
    let sessions_dir = dir.path.join("sessions");
    fs::create_dir_all(&snapshots_dir).expect("create snapshots dir");
    fs::create_dir_all(&sessions_dir).expect("create sessions dir");

    let file_thread = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let declared_thread = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    write_file(
        &snapshots_dir.join(format!("{file_thread}.sh")),
        &format!("declare -x STY=\"1234.s2\"\ndeclare -x CODEX_THREAD_ID=\"{declared_thread}\"\n"),
    );

    let mapping = test_collector(&dir.path).snapshot_thread_map();
    assert_eq!(mapping.get("1234.s2"), Some(&declared_thread.to_string()));
}

#[test]
fn parse_session_summary_detects_waiting_approval() {
    let dir = TempDirGuard::new("waiting_approval");
    let session_file = dir
        .path
        .join("rollout-2026-02-27T00-00-00-aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa.jsonl");
    write_file(
        &session_file,
        concat!(
            "{\"timestamp\":\"2026-02-27T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n",
            "{\"timestamp\":\"2026-02-27T00:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"exec_approval_request\"}}\n"
        ),
    );

    let summary = test_collector(&dir.path)
        .parse_session_summary(&session_file)
        .expect("parse session summary");
    assert!(summary.in_turn);
    assert!(summary.waiting_on_approval);
    assert!(!summary.waiting_on_user_input);
    assert_eq!(summary.last_event, "exec_approval_request");
}

#[test]
fn parse_session_summary_task_complete_clears_wait_flags() {
    let dir = TempDirGuard::new("task_complete_clears");
    let session_file = dir
        .path
        .join("rollout-2026-02-27T00-00-00-bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb.jsonl");
    write_file(
        &session_file,
        concat!(
            "{\"timestamp\":\"2026-02-27T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n",
            "{\"timestamp\":\"2026-02-27T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"request_user_input\"}}\n",
            "{\"timestamp\":\"2026-02-27T00:00:03Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"last_agent_message\":\"done\"}}\n"
        ),
    );

    let summary = test_collector(&dir.path)
        .parse_session_summary(&session_file)
        .expect("parse session summary");
    assert!(!summary.in_turn);
    assert!(!summary.waiting_on_approval);
    assert!(!summary.waiting_on_user_input);
    assert_eq!(summary.last_event, "task_complete");
    assert_eq!(summary.last_agent, "done");
}

#[test]
fn parse_session_summary_infers_running_when_start_is_outside_tail() {
    let dir = TempDirGuard::new("infer_running");
    let session_file = dir
        .path
        .join("rollout-2026-02-27T00-00-00-cccccccc-cccc-4ccc-8ccc-cccccccccccc.jsonl");
    let now = Utc::now().to_rfc3339();
    write_file(
        &session_file,
        &format!(
            "{{\"timestamp\":\"{now}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"agent_reasoning\",\"text\":\"working\"}}}}\n"
        ),
    );

    let summary = test_collector(&dir.path)
        .parse_session_summary(&session_file)
        .expect("parse session summary");
    assert!(summary.in_turn);
    assert_eq!(summary.last_event, "agent_reasoning");
}

#[test]
fn parse_session_summary_marks_actionable_prompt_without_user_ts_as_waiting_candidate() {
    let dir = TempDirGuard::new("actionable_prompt");
    let session_file = dir
        .path
        .join("rollout-2026-02-27T00-00-00-dddddddd-dddd-4ddd-8ddd-dddddddddddd.jsonl");
    write_file(
        &session_file,
        concat!(
            "{\"timestamp\":\"2026-02-27T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"Please confirm which path to take.\"}}\n",
            "{\"timestamp\":\"2026-02-27T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n"
        ),
    );

    let summary = test_collector(&dir.path)
        .parse_session_summary(&session_file)
        .expect("parse session summary");
    assert_eq!(summary.last_agent, "Please confirm which path to take.");
    assert_eq!(
        summary.last_agent_ts,
        parse_rfc3339_to_utc("2026-02-27T00:00:00Z")
    );
    assert!(looks_like_user_action_needed(&summary.last_agent));
}

#[test]
fn process_matcher_accepts_current_codex_layouts() {
    assert!(process_looks_like_codex(
        "node /opt/node/bin/codex --search"
    ));
    assert!(process_looks_like_codex(
        "/opt/vendor/codex/codex --model gpt-5.4"
    ));
    assert!(!process_looks_like_codex("bash -lc 'echo codex'"));
}

#[test]
fn file_stamp_changes_when_file_grows() {
    let dir = TempDirGuard::new("file_stamp");
    let path = dir.path.join("history.jsonl");
    write_file(&path, "one\n");
    let first = file_stamp_from_metadata(fs::metadata(&path).expect("first metadata"));

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open append");
    file.write_all(b"two\n").expect("append");
    file.flush().expect("flush");

    let second = file_stamp_from_metadata(fs::metadata(&path).expect("second metadata"));
    assert!(second > first);
}

#[test]
fn parse_session_meta_extracts_subagent_parent() {
    let dir = TempDirGuard::new("session_meta_parent");
    let session_file = dir
        .path
        .join("rollout-2026-03-08T00-00-00-aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa.jsonl");
    let parent_thread = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let session_meta = [
        r#"{"timestamp":"2026-03-08T05:10:39Z","type":"session_meta","payload":{"id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","timestamp":"2026-03-08T05:10:37Z","cwd":"/tmp/worktree","source":{"subagent":{"thread_spawn":{"parent_thread_id":""#,
        parent_thread,
        r#""}}}}}"#,
        "\n",
    ]
    .concat();
    write_file(&session_file, &session_meta);

    let meta = parse_session_meta(&session_file, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
        .expect("parse session meta");
    assert_eq!(meta.kind, SessionKind::Subagent);
    assert_eq!(meta.parent_thread_id, parent_thread);
}

#[test]
fn resolve_screen_thread_reuses_verified_cache() {
    let dir = TempDirGuard::new("screen_thread_cache");
    let sessions_dir = dir.path.join("sessions");
    fs::create_dir_all(&sessions_dir).expect("create sessions dir");

    let thread_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let session_path = sessions_dir.join(format!("rollout-2026-03-08T00-00-00-{thread_id}.jsonl"));
    write_file(
        &session_path,
        "{\"timestamp\":\"2026-03-08T05:10:39Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa\",\"timestamp\":\"2026-03-08T05:10:37Z\",\"cwd\":\"/tmp/worktree\",\"source\":\"cli\"}}\n",
    );

    let mut collector = test_collector(&dir.path);
    collector
        .screen_thread_cache
        .insert("1234.s1".to_string(), thread_id.to_string());

    let session_files = collector.index_session_files();
    let resolved =
        collector.resolve_screen_thread("1234.s1", None, &HashMap::new(), &session_files);
    assert_eq!(resolved, thread_id);
}

#[test]
fn scheduled_follow_up_count_tracks_active_children() {
    let dir = TempDirGuard::new("scheduled_follow_up");
    let sessions_dir = dir.path.join("sessions");
    fs::create_dir_all(&sessions_dir).expect("create sessions dir");

    let parent_thread = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let child_thread = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let parent_path =
        sessions_dir.join(format!("rollout-2026-03-08T00-00-00-{parent_thread}.jsonl"));
    let child_path = sessions_dir.join(format!("rollout-2026-03-08T00-00-01-{child_thread}.jsonl"));

    write_file(
        &parent_path,
        "{\"timestamp\":\"2026-03-08T05:10:39Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa\",\"timestamp\":\"2026-03-08T05:10:37Z\",\"cwd\":\"/tmp/worktree\",\"source\":\"cli\"}}\n",
    );
    let child_content = [
        r#"{"timestamp":"2026-03-08T05:10:39Z","type":"session_meta","payload":{"id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","timestamp":"2026-03-08T05:10:38Z","cwd":"/tmp/worktree","source":{"subagent":{"thread_spawn":{"parent_thread_id":""#,
        parent_thread,
        r#""}}}}}"#,
        "\n",
        r#"{"timestamp":"2026-03-08T05:10:40Z","type":"event_msg","payload":{"type":"task_started"}}"#,
        "\n",
    ]
    .concat();
    write_file(&child_path, &child_content);

    let mut collector = test_collector(&dir.path);
    let session_files = collector.index_session_files();
    let session_meta = collector.load_session_meta(&session_files);
    let scheduled =
        collector.scheduled_follow_up_count(parent_thread, &session_meta, &session_files);

    assert_eq!(scheduled, 1);
}

#[test]
fn status_for_thread_marks_unknown_without_observed_events() {
    let collector = DataCollector::empty_for_test();
    let status = collector.status_for_thread(&super::SessionSummary::unknown(), false, false);
    assert_eq!(status, SessionStatus::Unknown);
}
