use super::worker::spawn_refresh_worker;
use super::{
    ActionPaletteItem, ActionPaletteOverlay, App, AppAction, BrowserMode, ConfirmOverlay,
    FocusPane, InputMode, InputOverlay, InputOverlayKind, InspectorTab, OverlayState,
    PaletteCommand, ScreenRef, SearchOverlay, SearchResult, SearchTarget, WorktreeOverlay,
};
use crate::data::DataCollector;
use crate::types::{DashboardData, SessionRow, WorkspaceRow};
use crossterm::event::{KeyCode, KeyModifiers};
use std::cmp::Ordering;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

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
            focus: FocusPane::Browser,
            mode: InputMode::Normal,
            overlay: None,
            command: String::new(),
            browser_mode: BrowserMode::Workspaces,
            inspector_tab: InspectorTab::Summary,
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

    pub fn refresh(&mut self) {
        if self.refresh_in_flight {
            self.refresh_queued = true;
            return;
        }

        if self.refresh_tx.send(()).is_ok() {
            self.refresh_in_flight = true;
        } else {
            self.set_error("refresh worker is unavailable".to_string());
        }
    }

    pub fn wait_for_refresh(&mut self, timeout: Duration) {
        if !self.refresh_in_flight {
            return;
        }

        match self.refresh_rx.recv_timeout(timeout) {
            Ok(result) => {
                self.refresh_in_flight = false;
                self.apply_refresh_result(result);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                self.refresh_in_flight = false;
                self.set_error("refresh worker disconnected".to_string());
            }
        }

        if !self.refresh_in_flight && self.refresh_queued {
            self.refresh_queued = false;
            self.refresh();
        }
    }

    pub fn poll_refresh(&mut self) {
        if !self.refresh_in_flight {
            return;
        }

        match self.refresh_rx.try_recv() {
            Ok(result) => {
                self.refresh_in_flight = false;
                self.apply_refresh_result(result);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.refresh_in_flight = false;
                self.set_error("refresh worker disconnected".to_string());
            }
        }

        if !self.refresh_in_flight && self.refresh_queued {
            self.refresh_queued = false;
            self.refresh();
        }
    }

    fn apply_refresh_result(&mut self, result: Result<DashboardData, String>) {
        match result {
            Ok(data) => {
                self.data = data;
                self.recompute_visible();
            }
            Err(err) => {
                self.set_error(err);
            }
        }
    }

    pub fn recompute_visible(&mut self) {
        self.visible_workspace_indices = self.compute_visible_workspace_indices();
        self.visible_screen_refs = self.compute_visible_screen_refs();

        if !self.visible_workspace_indices.is_empty() {
            let browser_index = if let Some(selected_key) = self.selected_workspace_key.as_deref() {
                self.visible_workspace_indices
                    .iter()
                    .position(|idx| self.data.workspaces[*idx].key == selected_key)
                    .unwrap_or(
                        self.browser_index
                            .min(self.visible_workspace_indices.len() - 1),
                    )
            } else {
                self.browser_index
                    .min(self.visible_workspace_indices.len() - 1)
            };
            self.browser_index = browser_index;
            self.selected_workspace_key = self
                .visible_workspace_indices
                .get(self.browser_index)
                .map(|idx| self.data.workspaces[*idx].key.clone());
            self.browser_table_state.select(Some(self.browser_index));
        } else {
            self.browser_index = 0;
            self.selected_workspace_key = None;
            self.browser_table_state.select(None);
        }

        if !self.visible_screen_refs.is_empty() {
            let browser_index =
                if let Some(selected_screen_id) = self.selected_browser_screen_id.as_deref() {
                    self.visible_screen_refs
                        .iter()
                        .position(|screen_ref| {
                            self.screen_by_ref(*screen_ref)
                                .is_some_and(|row| row.screen_id == selected_screen_id)
                        })
                        .unwrap_or(self.browser_index.min(self.visible_screen_refs.len() - 1))
                } else {
                    self.browser_index.min(self.visible_screen_refs.len() - 1)
                };

            if self.browser_mode == BrowserMode::Screens {
                self.browser_index = browser_index;
                self.browser_table_state.select(Some(self.browser_index));
            }

            self.selected_browser_screen_id = self
                .visible_screen_refs
                .get(browser_index)
                .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
                .map(|row| row.screen_id.clone());
        } else {
            self.selected_browser_screen_id = None;
            if self.browser_mode == BrowserMode::Screens {
                self.browser_index = 0;
                self.browser_table_state.select(None);
            }
        }

        self.recompute_context_selection();

        if let Some(OverlayState::Search(_)) = self.overlay {
            self.rebuild_search_results();
        }
    }

    fn compute_visible_workspace_indices(&self) -> Vec<usize> {
        let mut indices = self
            .data
            .workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                self.browser_mode
                    .matches_workspace(workspace)
                    .then_some(idx)
            })
            .collect::<Vec<_>>();

        if self.browser_mode == BrowserMode::Recent {
            indices.sort_by(|left, right| {
                compare_optional_desc(
                    self.data.workspaces[*left].last_update,
                    self.data.workspaces[*right].last_update,
                )
                .then_with(|| {
                    self.data.workspaces[*left]
                        .display_name
                        .to_lowercase()
                        .cmp(&self.data.workspaces[*right].display_name.to_lowercase())
                })
            });
        }

        indices
    }

    fn compute_visible_screen_refs(&self) -> Vec<ScreenRef> {
        let mut refs = self
            .data
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(workspace_idx, workspace)| {
                workspace
                    .sessions
                    .iter()
                    .enumerate()
                    .map(move |(session_idx, _)| ScreenRef {
                        workspace_idx,
                        session_idx,
                    })
            })
            .collect::<Vec<_>>();

        refs.sort_by(|left, right| {
            let Some(left_row) = self.screen_by_ref(*left) else {
                return Ordering::Equal;
            };
            let Some(right_row) = self.screen_by_ref(*right) else {
                return Ordering::Equal;
            };

            (
                !left_row.needs_attention,
                left_row.status.rank(),
                compare_optional_desc(left_row.last_update, right_row.last_update),
                left_row.screen_name.to_lowercase(),
            )
                .cmp(&(
                    !right_row.needs_attention,
                    right_row.status.rank(),
                    compare_optional_desc(right_row.last_update, left_row.last_update),
                    right_row.screen_name.to_lowercase(),
                ))
        });

        refs
    }

    fn recompute_context_selection(&mut self) {
        let context_refs = self.context_screen_refs();
        if context_refs.is_empty() {
            self.context_index = 0;
            self.selected_context_screen_id = None;
            self.context_table_state.select(None);
            return;
        }

        let default_screen_id = self.selected_browser_screen_id.clone();
        let new_index = self
            .selected_context_screen_id
            .as_deref()
            .and_then(|screen_id| {
                context_refs.iter().position(|screen_ref| {
                    self.screen_by_ref(*screen_ref)
                        .is_some_and(|row| row.screen_id == screen_id)
                })
            })
            .or_else(|| {
                default_screen_id.as_deref().and_then(|screen_id| {
                    context_refs.iter().position(|screen_ref| {
                        self.screen_by_ref(*screen_ref)
                            .is_some_and(|row| row.screen_id == screen_id)
                    })
                })
            })
            .unwrap_or(self.context_index.min(context_refs.len() - 1));

        self.context_index = new_index;
        self.selected_context_screen_id = context_refs
            .get(self.context_index)
            .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
            .map(|row| row.screen_id.clone());
        self.context_table_state.select(Some(self.context_index));
    }

    pub(super) fn browser_workspace(&self) -> Option<&WorkspaceRow> {
        match self.browser_mode {
            BrowserMode::Screens => self
                .selected_browser_screen_ref()
                .and_then(|screen_ref| self.data.workspaces.get(screen_ref.workspace_idx)),
            _ => self.selected_workspace(),
        }
    }

    pub(super) fn context_screen_refs(&self) -> Vec<ScreenRef> {
        let workspace_idx = match self.browser_mode {
            BrowserMode::Screens => self
                .selected_browser_screen_ref()
                .map(|screen_ref| screen_ref.workspace_idx),
            _ => self
                .visible_workspace_indices
                .get(self.browser_index)
                .copied(),
        };

        workspace_idx
            .and_then(|idx| {
                self.data
                    .workspaces
                    .get(idx)
                    .map(|workspace| (idx, workspace))
            })
            .map(|(workspace_idx, workspace)| {
                workspace
                    .sessions
                    .iter()
                    .enumerate()
                    .map(|(session_idx, _)| ScreenRef {
                        workspace_idx,
                        session_idx,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    pub(super) fn selected_context_screen_ref(&self) -> Option<ScreenRef> {
        let screen_id = self.selected_context_screen_id.as_deref()?;
        self.context_screen_refs().into_iter().find(|screen_ref| {
            self.screen_by_ref(*screen_ref)
                .is_some_and(|row| row.screen_id == screen_id)
        })
    }

    pub(super) fn selected_context_screen(&self) -> Option<&SessionRow> {
        self.selected_context_screen_ref()
            .and_then(|screen_ref| self.screen_by_ref(screen_ref))
    }

    pub(super) fn subject_workspace(&self) -> Option<&WorkspaceRow> {
        self.selected_context_screen_ref()
            .and_then(|screen_ref| self.data.workspaces.get(screen_ref.workspace_idx))
            .or_else(|| self.browser_workspace())
    }

    pub(super) fn subject_screen(&self) -> Option<&SessionRow> {
        self.selected_context_screen()
            .or_else(|| {
                self.selected_browser_screen_ref()
                    .and_then(|screen_ref| self.screen_by_ref(screen_ref))
            })
            .or_else(|| {
                self.subject_workspace()
                    .and_then(|workspace| workspace.sessions.first())
            })
    }

    pub fn handle_normal_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> AppAction {
        if self.overlay.is_some() {
            return self.handle_overlay_key(code, modifiers);
        }

        match code {
            KeyCode::Char('q') => AppAction::Quit,
            KeyCode::Char(':') => {
                self.mode = InputMode::Command;
                self.command.clear();
                AppAction::None
            }
            KeyCode::Char('/') => {
                self.open_search_overlay();
                AppAction::None
            }
            KeyCode::Char('a') => {
                self.open_action_palette();
                AppAction::None
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::Browser => FocusPane::Context,
                    FocusPane::Context => FocusPane::Inspector,
                    FocusPane::Inspector => FocusPane::Browser,
                };
                AppAction::None
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    FocusPane::Browser => FocusPane::Inspector,
                    FocusPane::Context => FocusPane::Browser,
                    FocusPane::Inspector => FocusPane::Context,
                };
                AppAction::None
            }
            KeyCode::Char('1') => {
                self.set_browser_mode(BrowserMode::Workspaces);
                AppAction::None
            }
            KeyCode::Char('2') => {
                self.set_browser_mode(BrowserMode::Attention);
                AppAction::None
            }
            KeyCode::Char('3') => {
                self.set_browser_mode(BrowserMode::Running);
                AppAction::None
            }
            KeyCode::Char('4') => {
                self.set_browser_mode(BrowserMode::Recent);
                AppAction::None
            }
            KeyCode::Char('5') => {
                self.set_browser_mode(BrowserMode::Screens);
                AppAction::None
            }
            KeyCode::Char('[') => {
                self.inspector_tab = self.inspector_tab.prev();
                self.inspector_scroll = 0;
                AppAction::None
            }
            KeyCode::Char(']') => {
                self.inspector_tab = self.inspector_tab.next();
                self.inspector_scroll = 0;
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                AppAction::None
            }
            KeyCode::PageUp => {
                self.scroll_inspector(-8);
                AppAction::None
            }
            KeyCode::PageDown => {
                self.scroll_inspector(8);
                AppAction::None
            }
            KeyCode::Enter => self.activate_focused_selection(),
            KeyCode::Char('N') if modifiers.contains(KeyModifiers::SHIFT) => {
                self.spawn_selected_workspace()
            }
            KeyCode::Char('W') if modifiers.contains(KeyModifiers::SHIFT) => {
                self.open_worktree_overlay();
                AppAction::None
            }
            KeyCode::Char('p') => self
                .subject_workspace()
                .map(|workspace| AppAction::TogglePinWorkspace(workspace.path.clone()))
                .unwrap_or(AppAction::None),
            KeyCode::Char('i') => self
                .subject_screen()
                .map(|screen| AppAction::InterruptScreen(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            KeyCode::Char('K') if modifiers.contains(KeyModifiers::SHIFT) => {
                self.open_kill_confirm();
                AppAction::None
            }
            KeyCode::Char('r') => {
                self.open_rename_overlay();
                AppAction::None
            }
            KeyCode::Char('A') if modifiers.contains(KeyModifiers::SHIFT) => {
                self.open_add_workspace_overlay();
                AppAction::None
            }
            KeyCode::Esc => {
                self.focus = FocusPane::Browser;
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    pub fn handle_command_key(&mut self, code: KeyCode) -> AppAction {
        match code {
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                self.command.clear();
                AppAction::None
            }
            KeyCode::Enter => {
                self.mode = InputMode::Normal;
                let command = self.command.trim().to_string();
                self.command.clear();
                self.run_command(&command)
            }
            KeyCode::Backspace => {
                self.command.pop();
                AppAction::None
            }
            KeyCode::Char(ch) => {
                self.command.push(ch);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    fn handle_overlay_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> AppAction {
        match self.overlay.clone() {
            Some(OverlayState::Search(mut overlay)) => match code {
                KeyCode::Esc => {
                    self.overlay = None;
                    AppAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if !overlay.results.is_empty() {
                        overlay.selected = overlay.selected.saturating_sub(1);
                    }
                    self.overlay = Some(OverlayState::Search(overlay));
                    AppAction::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !overlay.results.is_empty() {
                        overlay.selected = (overlay.selected + 1).min(overlay.results.len() - 1);
                    }
                    self.overlay = Some(OverlayState::Search(overlay));
                    AppAction::None
                }
                KeyCode::Backspace => {
                    overlay.query.pop();
                    self.overlay = Some(OverlayState::Search(overlay));
                    self.rebuild_search_results();
                    AppAction::None
                }
                KeyCode::Enter => {
                    self.overlay = Some(OverlayState::Search(overlay.clone()));
                    self.activate_search_selection()
                }
                KeyCode::Char(ch)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    overlay.query.push(ch);
                    self.overlay = Some(OverlayState::Search(overlay));
                    self.rebuild_search_results();
                    AppAction::None
                }
                _ => {
                    self.overlay = Some(OverlayState::Search(overlay));
                    AppAction::None
                }
            },
            Some(OverlayState::ActionPalette(mut overlay)) => match code {
                KeyCode::Esc => {
                    self.overlay = None;
                    AppAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if !overlay.items.is_empty() {
                        overlay.selected = overlay.selected.saturating_sub(1);
                    }
                    self.overlay = Some(OverlayState::ActionPalette(overlay));
                    AppAction::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !overlay.items.is_empty() {
                        overlay.selected = (overlay.selected + 1).min(overlay.items.len() - 1);
                    }
                    self.overlay = Some(OverlayState::ActionPalette(overlay));
                    AppAction::None
                }
                KeyCode::Enter => {
                    let command = overlay
                        .items
                        .get(overlay.selected)
                        .map(|item| item.command.clone());
                    self.overlay = None;
                    command
                        .map(|command| self.execute_palette_command(command))
                        .unwrap_or(AppAction::None)
                }
                _ => {
                    self.overlay = Some(OverlayState::ActionPalette(overlay));
                    AppAction::None
                }
            },
            Some(OverlayState::Confirm(confirm)) => match code {
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.overlay = None;
                    AppAction::None
                }
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.overlay = None;
                    confirm.action
                }
                _ => {
                    self.overlay = Some(OverlayState::Confirm(confirm));
                    AppAction::None
                }
            },
            Some(OverlayState::Worktree(mut overlay)) => match code {
                KeyCode::Esc => {
                    self.overlay = None;
                    AppAction::None
                }
                KeyCode::Backspace => {
                    overlay.branch_input.pop();
                    overlay.target_preview = super::util::worktree_preview_path(
                        &overlay.source_cwd,
                        &overlay.branch_input,
                    );
                    self.overlay = Some(OverlayState::Worktree(overlay));
                    AppAction::None
                }
                KeyCode::Enter => {
                    let branch = overlay.branch_input.trim();
                    if branch.is_empty() {
                        self.overlay = Some(OverlayState::Worktree(overlay));
                        AppAction::None
                    } else {
                        self.overlay = None;
                        AppAction::SpawnWorktree {
                            source_cwd: overlay.source_cwd,
                            branch: branch.to_string(),
                        }
                    }
                }
                KeyCode::Char(ch)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    overlay.branch_input.push(ch);
                    overlay.target_preview = super::util::worktree_preview_path(
                        &overlay.source_cwd,
                        &overlay.branch_input,
                    );
                    self.overlay = Some(OverlayState::Worktree(overlay));
                    AppAction::None
                }
                _ => {
                    self.overlay = Some(OverlayState::Worktree(overlay));
                    AppAction::None
                }
            },
            Some(OverlayState::Input(mut overlay)) => match code {
                KeyCode::Esc => {
                    self.overlay = None;
                    AppAction::None
                }
                KeyCode::Backspace => {
                    overlay.value.pop();
                    self.overlay = Some(OverlayState::Input(overlay));
                    AppAction::None
                }
                KeyCode::Enter => {
                    let value = overlay.value.trim().to_string();
                    self.overlay = None;
                    if value.is_empty() {
                        AppAction::None
                    } else {
                        match overlay.submit {
                            InputOverlayKind::RenameScreen { screen_id } => {
                                AppAction::RenameScreen {
                                    screen_id,
                                    new_name: value,
                                }
                            }
                            InputOverlayKind::AddWorkspace => AppAction::AddWorkspace(value),
                        }
                    }
                }
                KeyCode::Char(ch)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    overlay.value.push(ch);
                    self.overlay = Some(OverlayState::Input(overlay));
                    AppAction::None
                }
                _ => {
                    self.overlay = Some(OverlayState::Input(overlay));
                    AppAction::None
                }
            },
            None => AppAction::None,
        }
    }

    fn activate_focused_selection(&mut self) -> AppAction {
        match self.focus {
            FocusPane::Browser => {
                if self.browser_mode == BrowserMode::Screens {
                    self.subject_screen()
                        .map(|screen| AppAction::Attach(screen.screen_id.clone()))
                        .unwrap_or(AppAction::None)
                } else if self
                    .subject_workspace()
                    .is_some_and(|workspace| !workspace.sessions.is_empty())
                {
                    self.focus = FocusPane::Context;
                    AppAction::None
                } else {
                    self.spawn_selected_workspace()
                }
            }
            FocusPane::Context => self
                .selected_context_screen()
                .map(|screen| AppAction::Attach(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            FocusPane::Inspector => match self.inspector_tab {
                InspectorTab::Actions => {
                    self.open_action_palette();
                    AppAction::None
                }
                InspectorTab::Worktree => {
                    self.open_worktree_overlay();
                    AppAction::None
                }
                _ => AppAction::None,
            },
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            FocusPane::Browser => match self.browser_mode {
                BrowserMode::Screens => {
                    move_index(
                        &mut self.browser_index,
                        self.visible_screen_refs.len(),
                        delta,
                    );
                    self.browser_table_state
                        .select(if self.visible_screen_refs.is_empty() {
                            None
                        } else {
                            Some(self.browser_index)
                        });
                    self.selected_browser_screen_id = self
                        .visible_screen_refs
                        .get(self.browser_index)
                        .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
                        .map(|row| row.screen_id.clone());
                    self.inspector_scroll = 0;
                    self.recompute_context_selection();
                }
                _ => {
                    move_index(
                        &mut self.browser_index,
                        self.visible_workspace_indices.len(),
                        delta,
                    );
                    self.browser_table_state
                        .select(if self.visible_workspace_indices.is_empty() {
                            None
                        } else {
                            Some(self.browser_index)
                        });
                    self.selected_workspace_key = self
                        .visible_workspace_indices
                        .get(self.browser_index)
                        .map(|idx| self.data.workspaces[*idx].key.clone());
                    self.inspector_scroll = 0;
                    self.recompute_context_selection();
                }
            },
            FocusPane::Context => {
                let refs = self.context_screen_refs();
                move_index(&mut self.context_index, refs.len(), delta);
                self.context_table_state.select(if refs.is_empty() {
                    None
                } else {
                    Some(self.context_index)
                });
                self.selected_context_screen_id = refs
                    .get(self.context_index)
                    .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
                    .map(|row| row.screen_id.clone());
                self.inspector_scroll = 0;
            }
            FocusPane::Inspector => self.scroll_inspector(delta as i16 * 3),
        }
    }

    fn scroll_inspector(&mut self, delta: i16) {
        self.inspector_scroll = self.inspector_scroll.saturating_add_signed(delta);
    }

    fn set_browser_mode(&mut self, mode: BrowserMode) {
        self.browser_mode = mode;
        self.browser_index = 0;
        self.focus = FocusPane::Browser;
        self.inspector_scroll = 0;
        self.recompute_visible();
    }

    fn open_search_overlay(&mut self) {
        self.overlay = Some(OverlayState::Search(SearchOverlay {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
        }));
        self.rebuild_search_results();
    }

    fn rebuild_search_results(&mut self) {
        let Some(OverlayState::Search(overlay)) = self.overlay.as_mut() else {
            return;
        };

        let query = overlay.query.trim().to_lowercase();
        let mut results = Vec::new();

        for workspace in &self.data.workspaces {
            let haystack = format!(
                "{} {} {} {}",
                workspace.display_name, workspace.path, workspace.branch_label, workspace.key
            )
            .to_lowercase();

            if query.is_empty() || haystack.contains(&query) {
                results.push(SearchResult {
                    label: workspace.display_name.clone(),
                    detail: format!(
                        "{} | {} | {}",
                        workspace_activity_summary(workspace),
                        workspace.branch_label,
                        workspace.path
                    ),
                    target: SearchTarget::Workspace(workspace.key.clone()),
                });
            }

            for session in &workspace.sessions {
                let timeline_text = session
                    .timeline
                    .iter()
                    .rev()
                    .take(4)
                    .map(|event| format!("{} {}", event.title, event.detail))
                    .collect::<Vec<_>>()
                    .join(" ");
                let session_haystack = format!(
                    "{} {} {} {} {} {} {} {}",
                    workspace.display_name,
                    workspace.path,
                    session.screen_id,
                    session.screen_name,
                    session.branch,
                    session.status_reason,
                    session.last_user,
                    timeline_text
                )
                .to_lowercase();

                if query.is_empty() || session_haystack.contains(&query) {
                    results.push(SearchResult {
                        label: format!("{} / {}", workspace.display_name, session.screen_name),
                        detail: format!(
                            "{} | {} | {}",
                            session.status.as_str(),
                            session.branch,
                            session.status_reason
                        ),
                        target: SearchTarget::Screen {
                            screen_id: session.screen_id.clone(),
                            tab: InspectorTab::Summary,
                        },
                    });
                }
            }
        }

        results.sort_by(|left, right| left.label.to_lowercase().cmp(&right.label.to_lowercase()));
        results.truncate(40);
        overlay.results = results;
        overlay.selected = overlay
            .selected
            .min(overlay.results.len().saturating_sub(1));
    }

    fn activate_search_selection(&mut self) -> AppAction {
        let Some(OverlayState::Search(overlay)) = &self.overlay else {
            return AppAction::None;
        };
        let Some(result) = overlay.results.get(overlay.selected) else {
            self.overlay = None;
            return AppAction::None;
        };

        match &result.target {
            SearchTarget::Workspace(key) => {
                self.browser_mode = BrowserMode::Workspaces;
                self.selected_workspace_key = Some(key.clone());
                self.focus = FocusPane::Browser;
            }
            SearchTarget::Screen { screen_id, tab } => {
                self.browser_mode = BrowserMode::Screens;
                self.selected_browser_screen_id = Some(screen_id.clone());
                self.selected_context_screen_id = Some(screen_id.clone());
                self.inspector_tab = *tab;
                self.focus = FocusPane::Context;
            }
        }

        self.overlay = None;
        self.recompute_visible();
        AppAction::None
    }

    fn open_action_palette(&mut self) {
        let mut items = Vec::new();
        if self.subject_screen().is_some() {
            items.push(ActionPaletteItem {
                label: "Attach screen".to_string(),
                detail: "Open the selected screen session".to_string(),
                command: PaletteCommand::AttachSelected,
            });
            items.push(ActionPaletteItem {
                label: "Open worktree".to_string(),
                detail: "Create or reuse a sibling worktree and spawn a screen there".to_string(),
                command: PaletteCommand::OpenWorktree,
            });
            items.push(ActionPaletteItem {
                label: "Interrupt screen".to_string(),
                detail: "Send Ctrl-C to the selected screen".to_string(),
                command: PaletteCommand::InterruptSelected,
            });
            items.push(ActionPaletteItem {
                label: "Rename screen".to_string(),
                detail: "Rename the selected screen session".to_string(),
                command: PaletteCommand::RenameScreen,
            });
            items.push(ActionPaletteItem {
                label: "Close screen".to_string(),
                detail: "Quit the selected screen session".to_string(),
                command: PaletteCommand::ConfirmKill,
            });
        }

        if self.subject_workspace().is_some() {
            items.push(ActionPaletteItem {
                label: "Spawn in workspace".to_string(),
                detail: "Launch a new Codex screen in this workspace".to_string(),
                command: PaletteCommand::SpawnWorkspace,
            });
            items.push(ActionPaletteItem {
                label: "Toggle pin".to_string(),
                detail: "Pin or unpin the selected workspace".to_string(),
                command: PaletteCommand::TogglePinWorkspace,
            });
        }

        items.push(ActionPaletteItem {
            label: "Add workspace".to_string(),
            detail: "Register a repo path in the global workspace index".to_string(),
            command: PaletteCommand::AddWorkspace,
        });

        for mode in [
            BrowserMode::Workspaces,
            BrowserMode::Attention,
            BrowserMode::Running,
            BrowserMode::Recent,
            BrowserMode::Screens,
        ] {
            items.push(ActionPaletteItem {
                label: format!("View {}", mode.label()),
                detail: format!("Switch the browser pane to {}", mode.as_str()),
                command: PaletteCommand::SwitchBrowser(mode),
            });
        }

        for tab in [
            InspectorTab::Summary,
            InspectorTab::Timeline,
            InspectorTab::Actions,
            InspectorTab::Worktree,
            InspectorTab::Logs,
        ] {
            items.push(ActionPaletteItem {
                label: format!("Inspector {}", tab.label()),
                detail: format!("Switch the inspector to {}", tab.label()),
                command: PaletteCommand::SwitchInspector(tab),
            });
        }

        self.overlay = Some(OverlayState::ActionPalette(ActionPaletteOverlay {
            title: "Actions".to_string(),
            items,
            selected: 0,
        }));
    }

    fn execute_palette_command(&mut self, command: PaletteCommand) -> AppAction {
        match command {
            PaletteCommand::AttachSelected => self
                .subject_screen()
                .map(|screen| AppAction::Attach(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            PaletteCommand::SpawnWorkspace => self.spawn_selected_workspace(),
            PaletteCommand::OpenWorktree => {
                self.open_worktree_overlay();
                AppAction::None
            }
            PaletteCommand::InterruptSelected => self
                .subject_screen()
                .map(|screen| AppAction::InterruptScreen(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            PaletteCommand::ConfirmKill => {
                self.open_kill_confirm();
                AppAction::None
            }
            PaletteCommand::TogglePinWorkspace => self
                .subject_workspace()
                .map(|workspace| AppAction::TogglePinWorkspace(workspace.path.clone()))
                .unwrap_or(AppAction::None),
            PaletteCommand::RenameScreen => {
                self.open_rename_overlay();
                AppAction::None
            }
            PaletteCommand::AddWorkspace => {
                self.open_add_workspace_overlay();
                AppAction::None
            }
            PaletteCommand::SwitchBrowser(mode) => {
                self.set_browser_mode(mode);
                AppAction::None
            }
            PaletteCommand::SwitchInspector(tab) => {
                self.inspector_tab = tab;
                self.inspector_scroll = 0;
                AppAction::None
            }
        }
    }

    fn open_kill_confirm(&mut self) {
        let Some(screen) = self.subject_screen() else {
            return;
        };
        self.overlay = Some(OverlayState::Confirm(ConfirmOverlay {
            title: "Close screen".to_string(),
            body: format!("Quit {} ({})?", screen.screen_name, screen.screen_id),
            action: AppAction::KillScreen(screen.screen_id.clone()),
        }));
    }

    fn open_worktree_overlay(&mut self) {
        let Some(screen) = self.subject_screen() else {
            return;
        };
        self.overlay = Some(OverlayState::Worktree(WorktreeOverlay {
            source_cwd: screen.cwd.clone(),
            branch_input: screen.branch.clone(),
            target_preview: super::util::worktree_preview_path(&screen.cwd, &screen.branch),
        }));
    }

    fn open_rename_overlay(&mut self) {
        let Some(screen) = self.subject_screen() else {
            return;
        };
        self.overlay = Some(OverlayState::Input(InputOverlay {
            title: "Rename screen".to_string(),
            hint: "Type the new screen session name and press Enter".to_string(),
            value: screen.screen_name.clone(),
            submit: InputOverlayKind::RenameScreen {
                screen_id: screen.screen_id.clone(),
            },
        }));
    }

    fn open_add_workspace_overlay(&mut self) {
        self.overlay = Some(OverlayState::Input(InputOverlay {
            title: "Add workspace".to_string(),
            hint: "Register an absolute repo path in the global workspace list".to_string(),
            value: String::new(),
            submit: InputOverlayKind::AddWorkspace,
        }));
    }

    fn run_command(&mut self, command: &str) -> AppAction {
        if command.is_empty() {
            return AppAction::None;
        }

        if let Some(index) = parse_prefixed_index(command, 'w') {
            self.browser_mode = BrowserMode::Workspaces;
            self.recompute_visible();
            if !self.visible_workspace_indices.is_empty() {
                self.browser_index = index.min(self.visible_workspace_indices.len() - 1);
                self.selected_workspace_key = self
                    .visible_workspace_indices
                    .get(self.browser_index)
                    .map(|idx| self.data.workspaces[*idx].key.clone());
                self.recompute_visible();
            }
            return AppAction::None;
        }

        if let Some(index) = parse_prefixed_index(command, 's') {
            let refs = self.context_screen_refs();
            if !refs.is_empty() {
                self.context_index = index.min(refs.len() - 1);
                self.selected_context_screen_id = refs
                    .get(self.context_index)
                    .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
                    .map(|row| row.screen_id.clone());
                self.recompute_visible();
            }
            return AppAction::None;
        }

        if let Some(index) = parse_prefixed_index(command, 'n') {
            if self.visible_workspace_indices.is_empty() {
                return AppAction::None;
            }
            let idx = index.min(self.visible_workspace_indices.len() - 1);
            let workspace_idx = self.visible_workspace_indices[idx];
            return AppAction::SpawnWorkspace(self.data.workspaces[workspace_idx].path.clone());
        }

        let mut parts = command.split_whitespace();
        let verb = parts.next().unwrap_or_default();
        match verb {
            "q" | "quit" | "exit" => AppAction::Quit,
            "workspaces" => {
                self.set_browser_mode(BrowserMode::Workspaces);
                AppAction::None
            }
            "attention" => {
                self.set_browser_mode(BrowserMode::Attention);
                AppAction::None
            }
            "running" => {
                self.set_browser_mode(BrowserMode::Running);
                AppAction::None
            }
            "recent" => {
                self.set_browser_mode(BrowserMode::Recent);
                AppAction::None
            }
            "screens" => {
                self.set_browser_mode(BrowserMode::Screens);
                AppAction::None
            }
            "spawn" | "new" => self.spawn_selected_workspace(),
            "attach" => self
                .subject_screen()
                .map(|screen| AppAction::Attach(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            "wt" | "worktree" => {
                let branch = parts.collect::<Vec<_>>().join(" ");
                if branch.is_empty() {
                    self.open_worktree_overlay();
                    AppAction::None
                } else if let Some(screen) = self.subject_screen() {
                    AppAction::SpawnWorktree {
                        source_cwd: screen.cwd.clone(),
                        branch,
                    }
                } else {
                    AppAction::None
                }
            }
            "pin" => self
                .subject_workspace()
                .map(|workspace| AppAction::TogglePinWorkspace(workspace.path.clone()))
                .unwrap_or(AppAction::None),
            "interrupt" => self
                .subject_screen()
                .map(|screen| AppAction::InterruptScreen(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            "kill" | "close" => self
                .subject_screen()
                .map(|screen| AppAction::KillScreen(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            "rename" => {
                let value = parts.collect::<Vec<_>>().join(" ");
                self.subject_screen()
                    .filter(|_| !value.is_empty())
                    .map(|screen| AppAction::RenameScreen {
                        screen_id: screen.screen_id.clone(),
                        new_name: value,
                    })
                    .unwrap_or(AppAction::None)
            }
            "add" => {
                let value = parts.collect::<Vec<_>>().join(" ");
                if value.is_empty() {
                    self.open_add_workspace_overlay();
                    AppAction::None
                } else {
                    AppAction::AddWorkspace(value)
                }
            }
            _ => {
                self.set_error(format!("unknown command: {command}"));
                AppAction::None
            }
        }
    }

    fn spawn_selected_workspace(&self) -> AppAction {
        self.subject_workspace()
            .map(|workspace| AppAction::SpawnWorkspace(workspace.path.clone()))
            .unwrap_or(AppAction::None)
    }
}

fn parse_prefixed_index(input: &str, prefix: char) -> Option<usize> {
    input
        .strip_prefix(prefix)
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
}

fn move_index(index: &mut usize, len: usize, delta: isize) {
    if len == 0 {
        *index = 0;
        return;
    }

    let current = *index as isize;
    let max = len.saturating_sub(1) as isize;
    *index = (current + delta).clamp(0, max) as usize;
}

fn compare_optional_desc<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    right.cmp(&left)
}

fn workspace_activity_summary(workspace: &WorkspaceRow) -> String {
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
