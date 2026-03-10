use crate::types::{SessionRow, WorkspaceRow};
use chrono::{DateTime, Utc};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub(super) fn workspace_matches_query(workspace: &WorkspaceRow, query: &str) -> bool {
    workspace.display_name.to_lowercase().contains(query)
        || workspace.path.to_lowercase().contains(query)
        || workspace.branch_label.to_lowercase().contains(query)
        || workspace
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
        || workspace
            .sessions
            .iter()
            .any(|session| session_matches_query(session, query))
}

pub(super) fn session_matches_query(session: &SessionRow, query: &str) -> bool {
    session.screen_id.to_lowercase().contains(query)
        || session.screen_name.to_lowercase().contains(query)
        || session.branch.to_lowercase().contains(query)
        || session.cwd.to_lowercase().contains(query)
        || session.thread_id.to_lowercase().contains(query)
        || session.last_event.to_lowercase().contains(query)
        || session.last_user.to_lowercase().contains(query)
        || session.last_agent.to_lowercase().contains(query)
        || session.scheduled_follow_ups.to_string().contains(query)
        || session.status.as_str().to_lowercase().contains(query)
}

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub(super) fn age_string(ts: Option<DateTime<Utc>>) -> String {
    let Some(ts) = ts else {
        return "-".to_string();
    };
    let delta = Utc::now().signed_duration_since(ts);
    if delta.num_seconds() < 60 {
        format!("{}s", delta.num_seconds().max(0))
    } else if delta.num_minutes() < 60 {
        format!("{}m", delta.num_minutes())
    } else if delta.num_hours() < 24 {
        format!("{}h", delta.num_hours())
    } else {
        format!("{}d", delta.num_days())
    }
}

pub(super) fn shorten_home(path: &str) -> String {
    let Ok(home) = std::env::var("HOME") else {
        return path.to_string();
    };

    if path == home {
        "~".to_string()
    } else if let Some(rest) = path.strip_prefix(&(home + "/")) {
        format!("~/{rest}")
    } else {
        path.to_string()
    }
}

pub(super) fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out = String::new();
    for (idx, c) in input.chars().enumerate() {
        if idx >= max.saturating_sub(1) {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}
