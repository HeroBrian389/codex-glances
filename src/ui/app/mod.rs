mod commands;
mod overlays;
mod refresh;
mod selection;

use super::worker::spawn_refresh_worker;
use super::{App, InputMode};
use crate::data::DataCollector;
use crate::types::{DashboardData, SessionRow, SessionStatus, WorkspaceRow};
use std::cmp::Ordering;

impl App {
    pub fn new(collector: DataCollector) -> Self {
        let (refresh_tx, refresh_rx) = spawn_refresh_worker(collector);
        let mut app = Self {
            data: DashboardData {
                workspaces: Vec::new(),
            },
            visible_workspace_indices: Vec::new(),
            visible_screen_refs: Vec::new(),
            browser_index: 0,
            selected_workspace_key: None,
            selected_browser_screen_id: None,
            context_index: 0,
            selected_context_screen_id: None,
            browser_table_state: Default::default(),
            context_table_state: Default::default(),
            focus: super::FocusPane::Browser,
            mode: InputMode::Normal,
            overlay: None,
            command: String::new(),
            browser_mode: super::BrowserMode::Workspaces,
            inspector_tab: super::InspectorTab::Summary,
            inspector_scroll: 0,
            last_error: None,
            last_info: None,
            refresh_tx,
            refresh_rx,
            refresh_in_flight: false,
            refresh_queued: false,
        };
        app.recompute_visible();
        app
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn is_refresh_in_flight(&self) -> bool {
        self.refresh_in_flight
    }

    pub fn set_error(&mut self, message: String) {
        self.last_info = None;
        self.last_error = Some(message);
    }

    pub fn set_info(&mut self, message: String) {
        self.last_error = None;
        self.last_info = Some(message);
    }
}

pub(super) fn parse_prefixed_index(input: &str, prefix: char) -> Option<usize> {
    input
        .strip_prefix(prefix)
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
}

pub(super) fn move_index(index: &mut usize, len: usize, delta: isize) {
    if len == 0 {
        *index = 0;
        return;
    }

    let current = *index as isize;
    let max = len.saturating_sub(1) as isize;
    *index = (current + delta).clamp(0, max) as usize;
}

pub(super) fn compare_optional_desc<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    right.cmp(&left)
}

pub(super) fn is_live_screen(status: SessionStatus) -> bool {
    matches!(status, SessionStatus::Running | SessionStatus::WaitingInput)
}

pub(super) fn compare_screen_rows(left: &SessionRow, right: &SessionRow) -> Ordering {
    let left_live = left.needs_attention || is_live_screen(left.status);
    let right_live = right.needs_attention || is_live_screen(right.status);

    (!left_live)
        .cmp(&!right_live)
        .then_with(|| compare_optional_desc(left.last_update, right.last_update))
        .then_with(|| left.status.rank().cmp(&right.status.rank()))
        .then_with(|| {
            left.screen_name
                .to_lowercase()
                .cmp(&right.screen_name.to_lowercase())
        })
}

pub(super) fn workspace_activity_summary(workspace: &WorkspaceRow) -> String {
    if workspace.waiting_sessions > 0 {
        format!(
            "{} waiting, {} running, {} screens",
            workspace.waiting_sessions, workspace.running_sessions, workspace.session_count
        )
    } else if workspace.running_sessions > 0 {
        format!(
            "{} running, {} screens",
            workspace.running_sessions, workspace.session_count
        )
    } else if workspace.session_count > 0 {
        format!("{} screens", workspace.session_count)
    } else {
        "saved workspace".to_string()
    }
}
