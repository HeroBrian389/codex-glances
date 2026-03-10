use super::{App, SortMode};
use crate::data::DataCollector;
use crate::types::{SessionRow, SessionStatus};
use chrono::{TimeZone, Utc};

fn row(screen_id: &str, screen_name: &str, branch: &str, updated_at: i64) -> SessionRow {
    SessionRow {
        screen_id: screen_id.to_string(),
        screen_name: screen_name.to_string(),
        branch: branch.to_string(),
        cwd: format!("/home/ubuntu/{screen_name}"),
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

#[test]
fn recompute_visible_preserves_selected_screen_across_resort() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.rows = vec![
        row("200.beta", "beta", "feature/b", 1_700_000_000),
        row("100.alpha", "alpha", "feature/a", 1_800_000_000),
    ];
    app.sort_mode = SortMode::Screen;
    app.recompute_visible();

    app.handle_normal_key(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(
        app.selected_row().map(|row| row.screen_id.as_str()),
        Some("200.beta")
    );

    app.sort_mode = SortMode::Updated;
    app.recompute_visible();
    assert_eq!(
        app.selected_row().map(|row| row.screen_id.as_str()),
        Some("200.beta")
    );
}

#[test]
fn recompute_visible_clears_selected_identity_when_filter_removes_row() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.rows = vec![
        row("100.alpha", "alpha", "feature/a", 1_800_000_000),
        row("200.beta", "beta", "feature/b", 1_700_000_000),
    ];
    app.sort_mode = SortMode::Screen;
    app.recompute_visible();

    app.handle_normal_key(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );
    app.search_query = "alpha".to_string();
    app.recompute_visible();

    assert_eq!(
        app.selected_row().map(|row| row.screen_id.as_str()),
        Some("100.alpha")
    );
    assert_eq!(app.selected_screen_id.as_deref(), Some("100.alpha"));
}

#[test]
fn command_mode_spawn_shortcut_uses_visible_row_folder() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.rows = vec![
        row("100.alpha", "alpha", "feature/a", 1_800_000_000),
        row("200.beta", "beta", "feature/b", 1_700_000_000),
    ];
    app.sort_mode = SortMode::Screen;
    app.recompute_visible();
    app.command = "n2".to_string();

    assert_eq!(
        app.handle_command_key(crossterm::event::KeyCode::Enter),
        super::AppAction::Spawn("/home/ubuntu/beta".to_string())
    );
}

#[test]
fn normal_mode_shift_n_spawns_selected_row_folder() {
    let mut app = App::new(DataCollector::empty_for_test());
    app.rows = vec![
        row("100.alpha", "alpha", "feature/a", 1_800_000_000),
        row("200.beta", "beta", "feature/b", 1_700_000_000),
    ];
    app.sort_mode = SortMode::Screen;
    app.recompute_visible();
    app.handle_normal_key(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );

    assert_eq!(
        app.handle_normal_key(
            crossterm::event::KeyCode::Char('N'),
            crossterm::event::KeyModifiers::SHIFT,
        ),
        super::AppAction::Spawn("/home/ubuntu/beta".to_string())
    );
}
