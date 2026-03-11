use crate::types::WorkspaceRow;
use chrono::{DateTime, Utc};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Modifier, Style};
use std::path::Path;

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

pub(super) fn highlight_style(is_focused: bool) -> Style {
    if is_focused {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

pub(super) fn browser_mode_style(is_active: bool) -> Style {
    if is_active {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub(super) fn tab_style(is_active: bool) -> Style {
    if is_active {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub(super) fn workspace_activity_badge(workspace: &WorkspaceRow) -> String {
    if workspace.waiting_sessions > 0 {
        format!(
            "W{} R{}",
            workspace.waiting_sessions, workspace.running_sessions
        )
    } else if workspace.session_count > 0 {
        format!("{} live", workspace.session_count)
    } else {
        "saved".to_string()
    }
}

pub(super) fn sanitize_branch_for_path(branch: &str) -> String {
    branch
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
}

pub(super) fn worktree_preview_path(source_cwd: &str, branch: &str) -> String {
    let path = Path::new(source_cwd);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let base = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("workspace");
    parent
        .join(format!("{base}--{}", sanitize_branch_for_path(branch)))
        .to_string_lossy()
        .into_owned()
}
