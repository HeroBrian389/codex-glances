mod app;
mod render;
mod util;
mod worker;

#[cfg(test)]
mod tests;

use crate::types::{DashboardData, SessionRow, WorkspaceRow};
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
pub enum BrowserMode {
    Workspaces,
    Attention,
    Running,
    Recent,
    Screens,
}

impl BrowserMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspaces => "workspaces",
            Self::Attention => "attention",
            Self::Running => "running",
            Self::Recent => "recent",
            Self::Screens => "screens",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Workspaces => "Workspaces",
            Self::Attention => "Attention",
            Self::Running => "Running",
            Self::Recent => "Recent",
            Self::Screens => "Screens",
        }
    }

    fn matches_workspace(self, workspace: &WorkspaceRow) -> bool {
        match self {
            Self::Workspaces => true,
            Self::Attention => workspace.waiting_sessions > 0,
            Self::Running => workspace.running_sessions > 0,
            Self::Recent => true,
            Self::Screens => true,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FocusPane {
    Browser,
    Context,
    Inspector,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InputMode {
    Normal,
    Command,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InspectorTab {
    Summary,
    Timeline,
    Actions,
    Worktree,
    Logs,
}

impl InspectorTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Timeline => "Timeline",
            Self::Actions => "Actions",
            Self::Worktree => "Worktree",
            Self::Logs => "Logs",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Summary => Self::Timeline,
            Self::Timeline => Self::Actions,
            Self::Actions => Self::Worktree,
            Self::Worktree => Self::Logs,
            Self::Logs => Self::Summary,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Summary => Self::Logs,
            Self::Timeline => Self::Summary,
            Self::Actions => Self::Timeline,
            Self::Worktree => Self::Actions,
            Self::Logs => Self::Worktree,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ScreenRef {
    pub workspace_idx: usize,
    pub session_idx: usize,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub label: String,
    pub detail: String,
    pub target: SearchTarget,
}

#[derive(Debug, Clone)]
pub enum SearchTarget {
    Workspace(String),
    Screen {
        screen_id: String,
        tab: InspectorTab,
    },
}

#[derive(Debug, Clone)]
pub enum OverlayState {
    Help,
    Search(SearchOverlay),
    ActionPalette(ActionPaletteOverlay),
    Confirm(ConfirmOverlay),
    Worktree(WorktreeOverlay),
    Input(InputOverlay),
}

#[derive(Debug, Clone)]
pub struct SearchOverlay {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct ActionPaletteOverlay {
    pub title: String,
    pub items: Vec<ActionPaletteItem>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct ActionPaletteItem {
    pub label: String,
    pub detail: String,
    pub command: PaletteCommand,
}

#[derive(Debug, Clone)]
pub enum PaletteCommand {
    AttachSelected,
    SpawnWorkspace,
    OpenWorktree,
    InterruptSelected,
    ConfirmKill,
    TogglePinWorkspace,
    RenameScreen,
    AddWorkspace,
    SwitchBrowser(BrowserMode),
    SwitchInspector(InspectorTab),
}

#[derive(Debug, Clone)]
pub struct ConfirmOverlay {
    pub title: String,
    pub body: String,
    pub action: AppAction,
}

#[derive(Debug, Clone)]
pub struct WorktreeOverlay {
    pub source_cwd: String,
    pub branch_input: String,
    pub target_preview: String,
}

#[derive(Debug, Clone)]
pub struct InputOverlay {
    pub title: String,
    pub hint: String,
    pub value: String,
    pub submit: InputOverlayKind,
}

#[derive(Debug, Clone)]
pub enum InputOverlayKind {
    RenameScreen { screen_id: String },
    AddWorkspace,
}

pub struct App {
    pub(super) data: DashboardData,
    pub(super) visible_workspace_indices: Vec<usize>,
    pub(super) visible_screen_refs: Vec<ScreenRef>,
    pub(super) browser_index: usize,
    pub(super) selected_workspace_key: Option<String>,
    pub(super) selected_browser_screen_id: Option<String>,
    pub(super) context_index: usize,
    pub(super) selected_context_screen_id: Option<String>,
    pub(super) browser_table_state: TableState,
    pub(super) context_table_state: TableState,
    pub(super) focus: FocusPane,
    pub(super) mode: InputMode,
    pub(super) overlay: Option<OverlayState>,
    pub(super) command: String,
    pub(super) browser_mode: BrowserMode,
    pub(super) inspector_tab: InspectorTab,
    pub(super) inspector_scroll: u16,
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

    pub(super) fn selected_workspace(&self) -> Option<&WorkspaceRow> {
        let idx = self.visible_workspace_indices.get(self.browser_index)?;
        self.data.workspaces.get(*idx)
    }

    pub(super) fn selected_browser_screen_ref(&self) -> Option<ScreenRef> {
        let screen_id = self.selected_browser_screen_id.as_deref()?;
        self.visible_screen_refs.iter().copied().find(|screen_ref| {
            self.screen_by_ref(*screen_ref)
                .is_some_and(|row| row.screen_id == screen_id)
        })
    }

    pub(super) fn screen_by_ref(&self, screen_ref: ScreenRef) -> Option<&SessionRow> {
        self.data
            .workspaces
            .get(screen_ref.workspace_idx)?
            .sessions
            .get(screen_ref.session_idx)
    }
}
