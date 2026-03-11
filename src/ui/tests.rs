use super::{App, AppAction, BrowserMode, FocusPane, InputMode, OverlayState};
use crate::data::DataCollector;
use crate::types::{
    DashboardData, SessionRow, SessionStatus, SessionTimelineEvent, TimelineEventKind, WorkspaceRow,
};
use chrono::{TimeZone, Utc};
use crossterm::event::{KeyCode, KeyModifiers};

fn fixture_path(name: &str) -> String {
    format!("/tmp/{name}")
}

fn session(screen_id: &str, screen_name: &str, branch: &str, updated_at: i64) -> SessionRow {
    SessionRow {
        screen_id: screen_id.to_string(),
        screen_name: screen_name.to_string(),
        branch: branch.to_string(),
        cwd: fixture_path(screen_name),
        thread_id: format!("{screen_id}-thread"),
        status: SessionStatus::Idle,
        needs_attention: false,
        scheduled_follow_ups: 0,
        last_event: "task_complete".to_string(),
        status_reason: "last event: task_complete".to_string(),
        last_user: "user message".to_string(),
        last_agent: "agent message".to_string(),
        last_update: Utc.timestamp_opt(updated_at, 0).single(),
        timeline: vec![SessionTimelineEvent {
            timestamp: Utc.timestamp_opt(updated_at, 0).single(),
            kind: TimelineEventKind::Status,
            title: "task_complete".to_string(),
            detail: "done".to_string(),
            emphasis: false,
        }],
        raw_log: vec!["raw log".to_string()],
    }
}

fn session_with_status(
    screen_id: &str,
    screen_name: &str,
    branch: &str,
    updated_at: i64,
    status: SessionStatus,
) -> SessionRow {
    let mut session = session(screen_id, screen_name, branch, updated_at);
    session.status = status;
    session.needs_attention = status == SessionStatus::WaitingInput;
    session
}

fn workspace(name: &str, updated_at: i64, sessions: Vec<SessionRow>) -> WorkspaceRow {
    WorkspaceRow {
        key: fixture_path(name),
        path: fixture_path(name),
        display_name: name.to_string(),
        branch_label: sessions
            .first()
            .map(|session| session.branch.clone())
            .unwrap_or_else(|| "-".to_string()),
        pinned: false,
        tags: Vec::new(),
        session_count: sessions.len(),
        waiting_sessions: sessions
            .iter()
            .filter(|session| session.status == SessionStatus::WaitingInput)
            .count(),
        running_sessions: sessions
            .iter()
            .filter(|session| session.status == SessionStatus::Running)
            .count(),
        follow_ups: sessions
            .iter()
            .map(|session| session.scheduled_follow_ups)
            .sum(),
        last_update: Utc.timestamp_opt(updated_at, 0).single(),
        last_user: "user message".to_string(),
        last_agent: "agent message".to_string(),
        sessions,
    }
}

#[test]
fn recompute_visible_preserves_selected_workspace_across_recent_mode() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![
            workspace(
                "beta",
                1_700_000_000,
                vec![session("200.beta", "beta", "feature/b", 1_700_000_000)],
            ),
            workspace(
                "alpha",
                1_800_000_000,
                vec![session("100.alpha", "alpha", "feature/a", 1_800_000_000)],
            ),
        ],
    };
    app.selected_workspace_key = Some(fixture_path("beta"));
    app.recompute_visible();

    app.browser_mode = BrowserMode::Recent;
    app.recompute_visible();

    assert_eq!(
        app.selected_workspace()
            .map(|workspace| workspace.display_name.as_str()),
        Some("beta")
    );
}

#[test]
fn screen_browser_preserves_selected_screen_across_refresh() {
    let mut app = App::new(DataCollector::empty_for_test());
    let alpha_screen = session("100.alpha", "alpha", "feature/a", 1_800_000_000);
    let beta_screen = session("200.beta", "beta", "feature/b", 1_700_000_000);
    app.data = DashboardData {
        workspaces: vec![
            workspace("alpha", 1_800_000_000, vec![alpha_screen.clone()]),
            workspace("beta", 1_700_000_000, vec![beta_screen.clone()]),
        ],
    };
    app.browser_mode = BrowserMode::Screens;
    app.selected_browser_screen_id = Some(beta_screen.screen_id.clone());
    app.recompute_visible();

    app.data = DashboardData {
        workspaces: vec![
            workspace("beta", 1_900_000_000, vec![beta_screen.clone()]),
            workspace("alpha", 1_800_000_000, vec![alpha_screen]),
        ],
    };
    app.recompute_visible();

    assert_eq!(
        app.subject_screen().map(|screen| screen.screen_id.as_str()),
        Some("200.beta")
    );
    assert_eq!(app.browser_mode, BrowserMode::Screens);
}

#[test]
fn screens_view_orders_live_before_inactive_then_latest_to_oldest() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![
            workspace(
                "alpha",
                1_800_000_000,
                vec![
                    session_with_status(
                        "101.alpha",
                        "idle-newer",
                        "feature/a",
                        1_800_000_000,
                        SessionStatus::Idle,
                    ),
                    session_with_status(
                        "100.alpha",
                        "running",
                        "feature/b",
                        1_700_000_000,
                        SessionStatus::Running,
                    ),
                ],
            ),
            workspace(
                "beta",
                1_600_000_000,
                vec![session_with_status(
                    "200.beta",
                    "idle-older",
                    "feature/c",
                    1_600_000_000,
                    SessionStatus::Idle,
                )],
            ),
        ],
    };
    app.browser_mode = BrowserMode::Screens;
    app.recompute_visible();

    let ordered = app
        .visible_screen_refs
        .iter()
        .filter_map(|screen_ref| app.screen_by_ref(*screen_ref))
        .map(|screen| screen.screen_name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ordered, vec!["running", "idle-newer", "idle-older"]);
}

#[test]
fn search_overlay_selects_screen_and_switches_to_screens_mode() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![workspace(
            "alpha",
            1_800_000_000,
            vec![session(
                "100.alpha",
                "alpha-screen",
                "feature/a",
                1_800_000_000,
            )],
        )],
    };
    app.recompute_visible();

    let _ = app.handle_normal_key(KeyCode::Char('/'), KeyModifiers::NONE);
    let _ = app.handle_normal_key(KeyCode::Char('s'), KeyModifiers::NONE);
    let _ = app.handle_normal_key(KeyCode::Char('c'), KeyModifiers::NONE);
    let _ = app.handle_normal_key(KeyCode::Char('r'), KeyModifiers::NONE);
    let _ = app.handle_normal_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(app.browser_mode, BrowserMode::Screens);
    assert_eq!(app.focus, FocusPane::Context);
    assert_eq!(
        app.subject_screen()
            .map(|screen| screen.screen_name.as_str()),
        Some("alpha-screen")
    );
}

#[test]
fn action_palette_attach_returns_attach_action() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![workspace(
            "alpha",
            1_800_000_000,
            vec![session(
                "100.alpha",
                "alpha-screen",
                "feature/a",
                1_800_000_000,
            )],
        )],
    };
    app.recompute_visible();
    app.focus = FocusPane::Context;

    let _ = app.handle_normal_key(KeyCode::Char('a'), KeyModifiers::NONE);
    let action = app.handle_normal_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(action, AppAction::Attach("100.alpha".to_string()));
}

#[test]
fn shift_w_opens_worktree_overlay_and_enter_returns_spawn_action() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![workspace(
            "alpha",
            1_800_000_000,
            vec![session(
                "100.alpha",
                "alpha-screen",
                "feature/a",
                1_800_000_000,
            )],
        )],
    };
    app.recompute_visible();
    app.focus = FocusPane::Context;

    let _ = app.handle_normal_key(KeyCode::Char('W'), KeyModifiers::SHIFT);
    assert!(matches!(app.overlay, Some(OverlayState::Worktree(_))));

    let action = app.handle_normal_key(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        action,
        AppAction::SpawnWorktree {
            source_cwd: fixture_path("alpha-screen"),
            branch: "feature/a".to_string(),
        }
    );
}

#[test]
fn command_mode_supports_screens_view_and_spawn_shortcuts() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![
            workspace(
                "alpha",
                1_800_000_000,
                vec![session(
                    "100.alpha",
                    "alpha-screen",
                    "feature/a",
                    1_800_000_000,
                )],
            ),
            workspace(
                "beta",
                1_700_000_000,
                vec![session(
                    "200.beta",
                    "beta-screen",
                    "feature/b",
                    1_700_000_000,
                )],
            ),
        ],
    };
    app.recompute_visible();

    app.mode = InputMode::Command;
    for ch in "screens".chars() {
        let _ = app.handle_command_key(KeyCode::Char(ch));
    }
    let _ = app.handle_command_key(KeyCode::Enter);
    assert_eq!(app.browser_mode, BrowserMode::Screens);

    app.mode = InputMode::Command;
    for ch in "n2".chars() {
        let _ = app.handle_command_key(KeyCode::Char(ch));
    }
    let action = app.handle_command_key(KeyCode::Enter);
    assert_eq!(action, AppAction::SpawnWorkspace(fixture_path("beta")));
}
