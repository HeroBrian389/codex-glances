use super::{App, AppAction, FocusPane, ViewMode};
use crate::data::DataCollector;
use crate::types::{DashboardData, SessionRow, SessionStatus, WorkspaceRow};
use chrono::{TimeZone, Utc};

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
        last_user: "-".to_string(),
        last_agent: "-".to_string(),
        last_update: Utc.timestamp_opt(updated_at, 0).single(),
    }
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
        waiting_sessions: 0,
        running_sessions: 0,
        follow_ups: 0,
        last_update: Utc.timestamp_opt(updated_at, 0).single(),
        last_user: "-".to_string(),
        last_agent: "-".to_string(),
        sessions,
    }
}

#[test]
fn recompute_visible_preserves_selected_workspace_across_reorder() {
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
    app.recompute_visible();

    app.handle_command_key(crossterm::event::KeyCode::Char('w'));
    app.command = "w2".to_string();
    let _ = app.handle_command_key(crossterm::event::KeyCode::Enter);
    assert_eq!(
        app.selected_workspace()
            .map(|workspace| workspace.display_name.as_str()),
        Some("alpha")
    );

    app.view_mode = ViewMode::Recent;
    app.recompute_visible();
    assert_eq!(
        app.selected_workspace()
            .map(|workspace| workspace.display_name.as_str()),
        Some("alpha")
    );
}

#[test]
fn recompute_visible_clamps_workspace_when_filter_removes_selection() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![
            workspace(
                "alpha",
                1_800_000_000,
                vec![session("100.alpha", "alpha", "feature/a", 1_800_000_000)],
            ),
            workspace(
                "beta",
                1_700_000_000,
                vec![session("200.beta", "beta", "feature/b", 1_700_000_000)],
            ),
        ],
    };
    app.recompute_visible();
    app.command = "w2".to_string();
    let _ = app.handle_command_key(crossterm::event::KeyCode::Enter);

    app.search_query = "alpha".to_string();
    app.recompute_visible();

    assert_eq!(
        app.selected_workspace()
            .map(|workspace| workspace.display_name.as_str()),
        Some("alpha")
    );
}

#[test]
fn command_mode_workspace_spawn_shortcut_uses_visible_workspace() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![
            workspace(
                "alpha",
                1_800_000_000,
                vec![session("100.alpha", "alpha", "feature/a", 1_800_000_000)],
            ),
            workspace(
                "beta",
                1_700_000_000,
                vec![session("200.beta", "beta", "feature/b", 1_700_000_000)],
            ),
        ],
    };
    app.recompute_visible();
    app.command = "n2".to_string();

    assert_eq!(
        app.handle_command_key(crossterm::event::KeyCode::Enter),
        AppAction::SpawnWorkspace(fixture_path("beta"))
    );
}

#[test]
fn normal_mode_shift_n_spawns_selected_workspace() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![
            workspace(
                "alpha",
                1_800_000_000,
                vec![session("100.alpha", "alpha", "feature/a", 1_800_000_000)],
            ),
            workspace(
                "beta",
                1_700_000_000,
                vec![session("200.beta", "beta", "feature/b", 1_700_000_000)],
            ),
        ],
    };
    app.recompute_visible();
    app.command = "w2".to_string();
    let _ = app.handle_command_key(crossterm::event::KeyCode::Enter);

    assert_eq!(
        app.handle_normal_key(
            crossterm::event::KeyCode::Char('N'),
            crossterm::event::KeyModifiers::SHIFT,
        ),
        AppAction::SpawnWorkspace(fixture_path("beta"))
    );
}

#[test]
fn session_focus_enter_attaches_selected_session() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![workspace(
            "alpha",
            1_800_000_000,
            vec![
                session("100.alpha", "alpha-1", "feature/a", 1_800_000_000),
                session("101.alpha", "alpha-2", "feature/b", 1_700_000_000),
            ],
        )],
    };
    app.recompute_visible();
    app.focus = FocusPane::Sessions;
    app.handle_normal_key(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );

    assert_eq!(
        app.handle_normal_key(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE
        ),
        AppAction::Attach("101.alpha".to_string())
    );
}

#[test]
fn session_focus_shift_w_spawns_worktree_from_selected_branch() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.data = DashboardData {
        workspaces: vec![workspace(
            "alpha",
            1_800_000_000,
            vec![
                session("100.alpha", "alpha-1", "feature/a", 1_800_000_000),
                session("101.alpha", "alpha-2", "feature/b", 1_700_000_000),
            ],
        )],
    };
    app.recompute_visible();
    app.focus = FocusPane::Sessions;
    app.handle_normal_key(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );

    assert_eq!(
        app.handle_normal_key(
            crossterm::event::KeyCode::Char('W'),
            crossterm::event::KeyModifiers::SHIFT,
        ),
        AppAction::SpawnWorktree {
            source_cwd: fixture_path("alpha-2"),
            branch: "feature/b".to_string(),
        }
    );
}
