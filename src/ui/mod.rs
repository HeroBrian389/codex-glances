mod app;
mod render;
mod util;
mod worker;

#[cfg(test)]
mod tests;

use crate::types::{DashboardData, WorkspaceRow};
use ratatui::widgets::TableState;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AppAction {
    None,
    Quit,
    Attach(String),
    SpawnWorkspace(String),
    SpawnWorktree { source_cwd: String, branch: String },
    AddWorkspace(String),
    TogglePinWorkspace(String),
    KillScreen(String),
    InterruptScreen(String),
    RenameScreen { screen_id: String, new_name: String },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FocusPane {
    Workspaces,
    Sessions,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
    Command,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ViewMode {
    Workspaces,
    Attention,
    Running,
    Recent,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspaces => "workspaces",
            Self::Attention => "attention",
            Self::Running => "running",
            Self::Recent => "recent",
        }
    }

    fn matches(self, workspace: &WorkspaceRow) -> bool {
        match self {
            Self::Workspaces => true,
            Self::Attention => workspace.waiting_sessions > 0,
            Self::Running => workspace.running_sessions > 0,
            Self::Recent => true,
        }
    }
}

pub struct App {
    pub(super) data: DashboardData,
    pub(super) visible_workspace_indices: Vec<usize>,
    pub(super) selected_workspace: usize,
    pub(super) selected_workspace_key: Option<String>,
    pub(super) selected_session: usize,
    pub(super) selected_session_screen_id: Option<String>,
    pub(super) workspace_table_state: TableState,
    pub(super) session_table_state: TableState,
    pub(super) focus: FocusPane,
    pub(super) mode: InputMode,
    pub(super) search_query: String,
    pub(super) command: String,
    pub(super) view_mode: ViewMode,
    pub(super) last_error: Option<String>,
    pub(super) last_info: Option<String>,
    pub(super) refresh_tx: Sender<()>,
    pub(super) refresh_rx: Receiver<Result<DashboardData, String>>,
    pub(super) refresh_in_flight: bool,
    pub(super) refresh_queued: bool,
}

impl App {
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_secs(3)
    }
}
