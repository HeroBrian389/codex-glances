use super::move_index;
use crate::types::{SessionRow, WorkspaceRow};
use crate::ui::{App, AppAction, BrowserMode, FocusPane, InspectorTab, ScreenRef};

impl App {
    pub(super) fn recompute_context_selection(&mut self) {
        let context_refs = self.context_screen_refs();
        if context_refs.is_empty() {
            self.context_index = 0;
            self.selected_context_screen_id = None;
            self.context_table_state.select(None);
            return;
        }

        let default_screen_id = self.selected_browser_screen_id.clone();
        let new_index = if self.browser_mode == BrowserMode::Screens {
            default_screen_id
                .as_deref()
                .and_then(|screen_id| {
                    context_refs.iter().position(|screen_ref| {
                        self.screen_by_ref(*screen_ref)
                            .is_some_and(|row| row.screen_id == screen_id)
                    })
                })
                .unwrap_or(self.context_index.min(context_refs.len() - 1))
        } else {
            self.selected_context_screen_id
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
                .unwrap_or(self.context_index.min(context_refs.len() - 1))
        };

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

    pub(crate) fn context_screen_refs(&self) -> Vec<ScreenRef> {
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

    pub(crate) fn subject_workspace(&self) -> Option<&WorkspaceRow> {
        if self.browser_mode == BrowserMode::Screens {
            self.browser_workspace()
        } else {
            self.selected_context_screen_ref()
                .and_then(|screen_ref| self.data.workspaces.get(screen_ref.workspace_idx))
                .or_else(|| self.browser_workspace())
        }
    }

    pub(crate) fn subject_screen(&self) -> Option<&SessionRow> {
        if self.browser_mode == BrowserMode::Screens {
            self.selected_browser_screen_ref()
                .and_then(|screen_ref| self.screen_by_ref(screen_ref))
                .or_else(|| {
                    self.subject_workspace()
                        .and_then(|workspace| workspace.sessions.first())
                })
        } else {
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
    }

    pub(super) fn activate_focused_selection(&mut self) -> AppAction {
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

    pub(super) fn move_selection(&mut self, delta: isize) {
        match self.focus {
            FocusPane::Browser => self.move_browser_selection(delta),
            FocusPane::Context => self.move_context_selection(delta),
            FocusPane::Inspector => self.scroll_inspector(delta as i16 * 3),
        }
    }

    fn move_browser_selection(&mut self, delta: isize) {
        match self.browser_mode {
            BrowserMode::Screens => {
                move_index(
                    &mut self.browser_index,
                    self.visible_screen_refs.len(),
                    delta,
                );
                self.browser_table_state
                    .select((!self.visible_screen_refs.is_empty()).then_some(self.browser_index));
                self.selected_browser_screen_id = self
                    .visible_screen_refs
                    .get(self.browser_index)
                    .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
                    .map(|row| row.screen_id.clone());
            }
            _ => {
                move_index(
                    &mut self.browser_index,
                    self.visible_workspace_indices.len(),
                    delta,
                );
                self.browser_table_state.select(
                    (!self.visible_workspace_indices.is_empty()).then_some(self.browser_index),
                );
                self.selected_workspace_key = self
                    .visible_workspace_indices
                    .get(self.browser_index)
                    .map(|idx| self.data.workspaces[*idx].key.clone());
            }
        }

        self.inspector_scroll = 0;
        self.recompute_context_selection();
    }

    fn move_context_selection(&mut self, delta: isize) {
        let refs = self.context_screen_refs();
        move_index(&mut self.context_index, refs.len(), delta);
        self.context_table_state
            .select((!refs.is_empty()).then_some(self.context_index));
        self.selected_context_screen_id = refs
            .get(self.context_index)
            .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
            .map(|row| row.screen_id.clone());
        self.inspector_scroll = 0;
    }

    pub(super) fn scroll_inspector(&mut self, delta: i16) {
        self.inspector_scroll = self.inspector_scroll.saturating_add_signed(delta);
    }

    pub(super) fn set_browser_mode(&mut self, mode: BrowserMode) {
        self.browser_mode = mode;
        self.browser_index = 0;
        self.focus = FocusPane::Browser;
        self.inspector_scroll = 0;
        self.recompute_visible();
    }

    pub(super) fn spawn_selected_workspace(&self) -> AppAction {
        self.subject_workspace()
            .map(|workspace| AppAction::SpawnWorkspace(workspace.path.clone()))
            .unwrap_or(AppAction::None)
    }
}
