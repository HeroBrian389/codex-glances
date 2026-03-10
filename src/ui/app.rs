use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
use std::time::Duration;

use crate::data::DataCollector;
use crate::types::{DashboardData, SessionRow, WorkspaceRow};

use super::util::workspace_matches_query;
use super::worker::spawn_refresh_worker;
use super::{App, AppAction, FocusPane, InputMode, ViewMode};

impl App {
    pub fn new(collector: DataCollector) -> Self {
        let (refresh_tx, refresh_rx) = spawn_refresh_worker(collector);
        Self {
            data: DashboardData {
                workspaces: Vec::new(),
            },
            visible_workspace_indices: Vec::new(),
            selected_workspace: 0,
            selected_workspace_key: None,
            selected_session: 0,
            selected_session_screen_id: None,
            workspace_table_state: ratatui::widgets::TableState::default(),
            session_table_state: ratatui::widgets::TableState::default(),
            focus: FocusPane::Workspaces,
            mode: InputMode::Normal,
            search_query: String::new(),
            command: String::new(),
            view_mode: ViewMode::Workspaces,
            last_error: None,
            last_info: None,
            refresh_tx,
            refresh_rx,
            refresh_in_flight: false,
            refresh_queued: false,
        }
    }

    pub fn refresh(&mut self) {
        if self.refresh_in_flight {
            self.refresh_queued = true;
            return;
        }

        match self.refresh_tx.send(()) {
            Ok(()) => self.refresh_in_flight = true,
            Err(_) => {
                self.last_error = Some("refresh worker is unavailable".to_string());
                self.refresh_in_flight = false;
                self.refresh_queued = false;
            }
        }
    }

    pub fn poll_refresh(&mut self) {
        loop {
            match self.refresh_rx.try_recv() {
                Ok(result) => self.apply_refresh_result(result),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.refresh_in_flight = false;
                    self.refresh_queued = false;
                    self.last_error = Some("refresh worker disconnected".to_string());
                    break;
                }
            }
        }
    }

    pub fn wait_for_refresh(&mut self, timeout: Duration) {
        if !self.refresh_in_flight {
            return;
        }

        match self.refresh_rx.recv_timeout(timeout) {
            Ok(result) => self.apply_refresh_result(result),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                self.refresh_in_flight = false;
                self.refresh_queued = false;
                self.last_error = Some("refresh worker disconnected".to_string());
            }
        }
    }

    fn apply_refresh_result(&mut self, result: Result<DashboardData, String>) {
        self.refresh_in_flight = false;
        match result {
            Ok(data) => {
                self.data = data;
                self.recompute_visible();
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(format!("refresh failed: {err}"));
            }
        }

        if self.refresh_queued {
            self.refresh_queued = false;
            self.refresh();
        }
    }

    pub fn is_refresh_in_flight(&self) -> bool {
        self.refresh_in_flight
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn set_info(&mut self, info: impl Into<String>) {
        self.last_info = Some(info.into());
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.last_error = Some(error.into());
    }

    pub fn handle_normal_key(&mut self, key_code: KeyCode, modifiers: KeyModifiers) -> AppAction {
        match key_code {
            KeyCode::Char('q') => AppAction::Quit,
            KeyCode::Char('r') => {
                self.refresh();
                AppAction::None
            }
            KeyCode::Char('/') => {
                self.mode = InputMode::Search;
                AppAction::None
            }
            KeyCode::Char(':') => {
                self.mode = InputMode::Command;
                self.command.clear();
                AppAction::None
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                self.toggle_focus();
                AppAction::None
            }
            KeyCode::Char('c') => {
                self.search_query.clear();
                self.recompute_visible();
                AppAction::None
            }
            KeyCode::Char('1') => self.set_view_mode(ViewMode::Workspaces),
            KeyCode::Char('2') => self.set_view_mode(ViewMode::Attention),
            KeyCode::Char('3') => self.set_view_mode(ViewMode::Running),
            KeyCode::Char('4') => self.set_view_mode(ViewMode::Recent),
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                AppAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                AppAction::None
            }
            KeyCode::Char('g') => {
                self.select_first();
                AppAction::None
            }
            KeyCode::Char('G') if modifiers.contains(KeyModifiers::SHIFT) => {
                self.select_last();
                AppAction::None
            }
            KeyCode::Char('p') => self.toggle_pin_selected_workspace(),
            KeyCode::Char('N') if modifiers.contains(KeyModifiers::SHIFT) => {
                self.spawn_selected_workspace()
            }
            KeyCode::Char('W') if modifiers.contains(KeyModifiers::SHIFT) => {
                self.spawn_selected_worktree()
            }
            KeyCode::Char('x') => self.kill_selected_session(),
            KeyCode::Char('i') => self.interrupt_selected_session(),
            KeyCode::Enter => self.primary_enter_action(),
            _ => AppAction::None,
        }
    }

    pub fn handle_search_key(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Esc | KeyCode::Enter => self.mode = InputMode::Normal,
            KeyCode::Backspace => {
                self.search_query.pop();
                self.recompute_visible();
            }
            KeyCode::Char(ch) => {
                self.search_query.push(ch);
                self.recompute_visible();
            }
            _ => {}
        }
    }

    pub fn handle_command_key(&mut self, key_code: KeyCode) -> AppAction {
        match key_code {
            KeyCode::Esc => {
                self.command.clear();
                self.mode = InputMode::Normal;
                AppAction::None
            }
            KeyCode::Backspace => {
                self.command.pop();
                AppAction::None
            }
            KeyCode::Char(ch) => {
                self.command.push(ch);
                AppAction::None
            }
            KeyCode::Enter => {
                let action = self.execute_command();
                self.command.clear();
                self.mode = InputMode::Normal;
                action
            }
            _ => AppAction::None,
        }
    }

    fn set_view_mode(&mut self, view_mode: ViewMode) -> AppAction {
        self.view_mode = view_mode;
        self.recompute_visible();
        AppAction::None
    }

    pub(super) fn recompute_visible(&mut self) {
        self.selected_workspace_key = self.selected_workspace().map(|row| row.key.clone());
        self.selected_session_screen_id = self.selected_session().map(|row| row.screen_id.clone());
        let query = self.search_query.to_lowercase();

        self.visible_workspace_indices = self
            .data
            .workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                (self.view_mode.matches(workspace)
                    && (query.is_empty() || workspace_matches_query(workspace, &query)))
                .then_some(idx)
            })
            .collect();

        if self.view_mode == ViewMode::Recent {
            self.visible_workspace_indices.sort_by(|left, right| {
                let left_workspace = &self.data.workspaces[*left];
                let right_workspace = &self.data.workspaces[*right];
                let left_key = (
                    right_workspace.last_update < left_workspace.last_update,
                    left_workspace.display_name.to_lowercase(),
                );
                let right_key = (
                    left_workspace.last_update < right_workspace.last_update,
                    right_workspace.display_name.to_lowercase(),
                );
                left_key.cmp(&right_key)
            });
        }

        if self.visible_workspace_indices.is_empty() {
            self.selected_workspace = 0;
            self.selected_workspace_key = None;
            self.workspace_table_state.select(None);
            self.selected_session = 0;
            self.selected_session_screen_id = None;
            self.session_table_state.select(None);
            self.focus = FocusPane::Workspaces;
            return;
        }

        self.selected_workspace = self
            .selected_workspace_key
            .as_ref()
            .and_then(|workspace_key| {
                self.visible_workspace_indices.iter().position(|idx| {
                    self.data
                        .workspaces
                        .get(*idx)
                        .is_some_and(|workspace| &workspace.key == workspace_key)
                })
            })
            .unwrap_or_else(|| {
                self.selected_workspace
                    .min(self.visible_workspace_indices.len() - 1)
            });
        self.selected_workspace_key = self.selected_workspace().map(|row| row.key.clone());
        self.workspace_table_state
            .select(Some(self.selected_workspace));
        self.recompute_session_selection();
    }

    fn recompute_session_selection(&mut self) {
        let Some(workspace) = self.selected_workspace() else {
            self.selected_session = 0;
            self.selected_session_screen_id = None;
            self.session_table_state.select(None);
            return;
        };

        if workspace.sessions.is_empty() {
            self.selected_session = 0;
            self.selected_session_screen_id = None;
            self.session_table_state.select(None);
            self.focus = FocusPane::Workspaces;
            return;
        }

        self.selected_session = self
            .selected_session_screen_id
            .as_ref()
            .and_then(|screen_id| {
                workspace
                    .sessions
                    .iter()
                    .position(|session| &session.screen_id == screen_id)
            })
            .unwrap_or_else(|| self.selected_session.min(workspace.sessions.len() - 1));
        self.selected_session_screen_id = self.selected_session().map(|row| row.screen_id.clone());
        self.session_table_state.select(Some(self.selected_session));
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            FocusPane::Workspaces => self.move_workspace_selection(delta),
            FocusPane::Sessions => self.move_session_selection(delta),
        }
    }

    fn move_workspace_selection(&mut self, delta: isize) {
        if self.visible_workspace_indices.is_empty() {
            self.selected_workspace = 0;
            self.workspace_table_state.select(None);
            return;
        }
        let len = self.visible_workspace_indices.len() as isize;
        let next = (self.selected_workspace as isize + delta).clamp(0, len - 1);
        self.selected_workspace = next as usize;
        self.workspace_table_state
            .select(Some(self.selected_workspace));
        self.selected_workspace_key = self.selected_workspace().map(|row| row.key.clone());
        self.selected_session = 0;
        self.selected_session_screen_id = None;
        self.recompute_session_selection();
    }

    fn move_session_selection(&mut self, delta: isize) {
        let Some(workspace) = self.selected_workspace() else {
            self.focus = FocusPane::Workspaces;
            return;
        };
        if workspace.sessions.is_empty() {
            self.focus = FocusPane::Workspaces;
            self.session_table_state.select(None);
            return;
        }

        let len = workspace.sessions.len() as isize;
        let next = (self.selected_session as isize + delta).clamp(0, len - 1);
        self.selected_session = next as usize;
        self.session_table_state.select(Some(self.selected_session));
        self.selected_session_screen_id = self.selected_session().map(|row| row.screen_id.clone());
    }

    fn select_first(&mut self) {
        match self.focus {
            FocusPane::Workspaces => {
                if self.visible_workspace_indices.is_empty() {
                    self.workspace_table_state.select(None);
                    return;
                }
                self.selected_workspace = 0;
                self.workspace_table_state.select(Some(0));
                self.selected_workspace_key = self.selected_workspace().map(|row| row.key.clone());
                self.selected_session = 0;
                self.selected_session_screen_id = None;
                self.recompute_session_selection();
            }
            FocusPane::Sessions => {
                if self
                    .selected_workspace()
                    .is_some_and(|workspace| !workspace.sessions.is_empty())
                {
                    self.selected_session = 0;
                    self.session_table_state.select(Some(0));
                    self.selected_session_screen_id =
                        self.selected_session().map(|row| row.screen_id.clone());
                }
            }
        }
    }

    fn select_last(&mut self) {
        match self.focus {
            FocusPane::Workspaces => {
                if self.visible_workspace_indices.is_empty() {
                    self.workspace_table_state.select(None);
                    return;
                }
                self.selected_workspace = self.visible_workspace_indices.len() - 1;
                self.workspace_table_state
                    .select(Some(self.selected_workspace));
                self.selected_workspace_key = self.selected_workspace().map(|row| row.key.clone());
                self.selected_session = 0;
                self.selected_session_screen_id = None;
                self.recompute_session_selection();
            }
            FocusPane::Sessions => {
                if let Some(workspace) = self.selected_workspace() {
                    if workspace.sessions.is_empty() {
                        return;
                    }
                    self.selected_session = workspace.sessions.len() - 1;
                    self.session_table_state.select(Some(self.selected_session));
                    self.selected_session_screen_id =
                        self.selected_session().map(|row| row.screen_id.clone());
                }
            }
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            FocusPane::Workspaces => {
                if self
                    .selected_workspace()
                    .is_some_and(|workspace| !workspace.sessions.is_empty())
                {
                    FocusPane::Sessions
                } else {
                    FocusPane::Workspaces
                }
            }
            FocusPane::Sessions => FocusPane::Workspaces,
        };
    }

    pub(super) fn selected_workspace(&self) -> Option<&WorkspaceRow> {
        let idx = self
            .visible_workspace_indices
            .get(self.selected_workspace)?;
        self.data.workspaces.get(*idx)
    }

    pub(super) fn selected_session(&self) -> Option<&SessionRow> {
        self.selected_workspace()?
            .sessions
            .get(self.selected_session)
    }

    fn selected_or_best_session(&self) -> Option<&SessionRow> {
        if self.focus == FocusPane::Sessions {
            self.selected_session()
        } else {
            self.selected_workspace()?.sessions.first()
        }
    }

    fn primary_enter_action(&mut self) -> AppAction {
        match self.focus {
            FocusPane::Sessions => self.attach_selected_session(),
            FocusPane::Workspaces => {
                if self
                    .selected_workspace()
                    .is_some_and(|workspace| !workspace.sessions.is_empty())
                {
                    self.attach_best_session()
                } else {
                    self.spawn_selected_workspace()
                }
            }
        }
    }

    fn attach_selected_session(&mut self) -> AppAction {
        if let Some(row) = self.selected_session() {
            AppAction::Attach(row.screen_id.clone())
        } else {
            self.last_error = Some("no session selected".to_string());
            AppAction::None
        }
    }

    fn attach_best_session(&mut self) -> AppAction {
        if let Some(row) = self
            .selected_workspace()
            .and_then(|workspace| workspace.sessions.first())
        {
            AppAction::Attach(row.screen_id.clone())
        } else {
            self.last_error = Some("selected workspace has no active sessions".to_string());
            AppAction::None
        }
    }

    fn spawn_selected_workspace(&mut self) -> AppAction {
        let Some(workspace) = self.selected_workspace() else {
            self.last_error = Some("nothing selected".to_string());
            return AppAction::None;
        };

        if workspace.path == "-" {
            self.last_error = Some("selected workspace has no folder".to_string());
            return AppAction::None;
        }

        AppAction::SpawnWorkspace(workspace.path.clone())
    }

    fn toggle_pin_selected_workspace(&mut self) -> AppAction {
        let Some(workspace) = self.selected_workspace() else {
            self.last_error = Some("nothing selected".to_string());
            return AppAction::None;
        };

        if workspace.path == "-" {
            self.last_error = Some("unlinked sessions cannot be pinned".to_string());
            return AppAction::None;
        }

        AppAction::TogglePinWorkspace(workspace.path.clone())
    }

    fn kill_selected_session(&mut self) -> AppAction {
        if let Some(session) = self.selected_or_best_session() {
            AppAction::KillScreen(session.screen_id.clone())
        } else {
            self.last_error = Some("no active session available".to_string());
            AppAction::None
        }
    }

    fn interrupt_selected_session(&mut self) -> AppAction {
        if let Some(session) = self.selected_or_best_session() {
            AppAction::InterruptScreen(session.screen_id.clone())
        } else {
            self.last_error = Some("no active session available".to_string());
            AppAction::None
        }
    }

    fn spawn_selected_worktree(&mut self) -> AppAction {
        let Some((source_cwd, branch)) = self.selected_worktree_source() else {
            return AppAction::None;
        };
        if source_cwd == "-" {
            self.last_error = Some("selected session has no folder".to_string());
            return AppAction::None;
        }
        if branch == "-" {
            self.last_error = Some("selected session has no branch".to_string());
            return AppAction::None;
        }

        AppAction::SpawnWorktree { source_cwd, branch }
    }

    fn spawn_selected_worktree_branch(&mut self, branch: &str) -> AppAction {
        if branch.is_empty() {
            self.last_error = Some("branch is required for wt <branch>".to_string());
            return AppAction::None;
        }

        let Some((source_cwd, _)) = self.selected_worktree_source() else {
            return AppAction::None;
        };
        if source_cwd == "-" {
            self.last_error = Some("selected session has no folder".to_string());
            return AppAction::None;
        }

        AppAction::SpawnWorktree {
            source_cwd,
            branch: branch.to_string(),
        }
    }

    fn selected_worktree_source(&mut self) -> Option<(String, String)> {
        match self.focus {
            FocusPane::Sessions => {
                let selected = self
                    .selected_session()
                    .map(|session| (session.cwd.clone(), session.branch.clone()));
                if selected.is_none() {
                    self.last_error = Some("no session selected".to_string());
                }
                selected
            }
            FocusPane::Workspaces => {
                let Some(workspace) = self.selected_workspace() else {
                    self.last_error = Some("nothing selected".to_string());
                    return None;
                };
                if workspace.sessions.is_empty() {
                    self.last_error = Some(
                        "selected workspace has no active session to source a branch".to_string(),
                    );
                    return None;
                }
                if workspace.sessions.len() > 1 {
                    self.last_error = Some(
                        "focus a specific session before spawning a worktree when multiple sessions are active"
                            .to_string(),
                    );
                    return None;
                }
                workspace
                    .sessions
                    .first()
                    .map(|session| (session.cwd.clone(), session.branch.clone()))
            }
        }
    }

    fn execute_command(&mut self) -> AppAction {
        let normalized_owned = self.command.trim().trim_start_matches(':').to_string();
        let normalized = normalized_owned.as_str();
        if normalized.is_empty() {
            return AppAction::None;
        }

        if let Some(path) = normalized
            .strip_prefix("add ")
            .or_else(|| normalized.strip_prefix("a "))
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            return AppAction::AddWorkspace(path.to_string());
        }

        if normalized == "pin" {
            return self.toggle_pin_selected_workspace();
        }

        if normalized == "n" {
            return self.spawn_selected_workspace();
        }

        if normalized == "wt" {
            return self.spawn_selected_worktree();
        }

        if let Some(branch) = normalized.strip_prefix("wt ").map(str::trim) {
            return self.spawn_selected_worktree_branch(branch);
        }

        if let Some(num_str) = normalized.strip_prefix('w')
            && let Ok(n) = num_str.parse::<usize>()
        {
            if (1..=self.visible_workspace_indices.len()).contains(&n) {
                self.selected_workspace = n - 1;
                self.workspace_table_state
                    .select(Some(self.selected_workspace));
                self.selected_workspace_key = self.selected_workspace().map(|row| row.key.clone());
                self.selected_session = 0;
                self.selected_session_screen_id = None;
                self.recompute_session_selection();
                return AppAction::None;
            }
            self.last_error = Some(format!("workspace shortcut out of range: w{n}"));
            return AppAction::None;
        }

        if let Some(num_str) = normalized.strip_prefix('n')
            && let Ok(n) = num_str.parse::<usize>()
        {
            if (1..=self.visible_workspace_indices.len()).contains(&n) {
                self.selected_workspace = n - 1;
                self.workspace_table_state
                    .select(Some(self.selected_workspace));
                self.selected_workspace_key = self.selected_workspace().map(|row| row.key.clone());
                return self.spawn_selected_workspace();
            }
            self.last_error = Some(format!("workspace shortcut out of range: n{n}"));
            return AppAction::None;
        }

        if let Some(num_str) = normalized.strip_prefix('s')
            && let Ok(n) = num_str.parse::<usize>()
        {
            let Some(workspace) = self.selected_workspace() else {
                self.last_error = Some("no workspace selected".to_string());
                return AppAction::None;
            };
            if (1..=workspace.sessions.len()).contains(&n) {
                self.selected_session = n - 1;
                self.session_table_state.select(Some(self.selected_session));
                self.selected_session_screen_id =
                    self.selected_session().map(|row| row.screen_id.clone());
                return self.attach_selected_session();
            }
            self.last_error = Some(format!("session shortcut out of range: s{n}"));
            return AppAction::None;
        }

        if let Some(num_str) = normalized.strip_prefix('k')
            && let Ok(n) = num_str.parse::<usize>()
        {
            let Some(workspace) = self.selected_workspace() else {
                self.last_error = Some("no workspace selected".to_string());
                return AppAction::None;
            };
            if (1..=workspace.sessions.len()).contains(&n) {
                return AppAction::KillScreen(workspace.sessions[n - 1].screen_id.clone());
            }
            self.last_error = Some(format!("session shortcut out of range: k{n}"));
            return AppAction::None;
        }

        if let Some(num_str) = normalized.strip_prefix('i')
            && let Ok(n) = num_str.parse::<usize>()
        {
            let Some(workspace) = self.selected_workspace() else {
                self.last_error = Some("no workspace selected".to_string());
                return AppAction::None;
            };
            if (1..=workspace.sessions.len()).contains(&n) {
                return AppAction::InterruptScreen(workspace.sessions[n - 1].screen_id.clone());
            }
            self.last_error = Some(format!("session shortcut out of range: i{n}"));
            return AppAction::None;
        }

        if let Some(new_name) = normalized.strip_prefix("rename ").map(str::trim)
            && !new_name.is_empty()
        {
            if let Some(session) = self.selected_or_best_session() {
                return AppAction::RenameScreen {
                    screen_id: session.screen_id.clone(),
                    new_name: new_name.to_string(),
                };
            }
            self.last_error = Some("no active session available".to_string());
            return AppAction::None;
        }

        if let Some(session) = self
            .data
            .workspaces
            .iter()
            .flat_map(|workspace| workspace.sessions.iter())
            .find(|session| session.screen_id == normalized || session.screen_name == normalized)
        {
            return AppAction::Attach(session.screen_id.clone());
        }

        self.last_error = Some(format!("unknown command: {normalized}"));
        AppAction::None
    }
}
