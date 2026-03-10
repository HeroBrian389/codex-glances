use crate::types::SessionRow;
use chrono::{DateTime, Utc};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub(super) fn row_matches_query(row: &SessionRow, query: &str) -> bool {
    row.screen_id.to_lowercase().contains(query)
        || row.screen_name.to_lowercase().contains(query)
        || row.branch.to_lowercase().contains(query)
        || row.cwd.to_lowercase().contains(query)
        || row.thread_id.to_lowercase().contains(query)
        || row.last_event.to_lowercase().contains(query)
        || row.last_user.to_lowercase().contains(query)
        || row.last_agent.to_lowercase().contains(query)
        || row.scheduled_follow_ups.to_string().contains(query)
        || row.status.as_str().to_lowercase().contains(query)
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
    path.strip_prefix("/home/ubuntu/")
        .map_or_else(|| path.to_string(), |p| p.to_string())
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
