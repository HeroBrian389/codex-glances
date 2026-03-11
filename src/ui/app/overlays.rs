use super::workspace_activity_summary;
use crate::ui::{
    ActionPaletteItem, ActionPaletteOverlay, App, AppAction, BrowserMode, ConfirmOverlay,
    FocusPane, InputOverlay, InputOverlayKind, InspectorTab, OverlayState, PaletteCommand,
    SearchOverlay, SearchResult, SearchTarget, WorktreeOverlay,
};
use crossterm::event::{KeyCode, KeyModifiers};

impl App {
    pub(super) fn handle_overlay_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> AppAction {
        match self.overlay.clone() {
            Some(OverlayState::Help) => match code {
                KeyCode::Esc | KeyCode::Char('?') => {
                    self.overlay = None;
                    AppAction::None
                }
                _ => AppAction::None,
            },
            Some(OverlayState::Search(mut overlay)) => {
                self.handle_search_overlay_key(&mut overlay, code, modifiers)
            }
            Some(OverlayState::ActionPalette(mut overlay)) => {
                self.handle_palette_overlay_key(&mut overlay, code)
            }
            Some(OverlayState::Confirm(confirm)) => self.handle_confirm_overlay_key(confirm, code),
            Some(OverlayState::Worktree(mut overlay)) => {
                self.handle_worktree_overlay_key(&mut overlay, code, modifiers)
            }
            Some(OverlayState::Input(mut overlay)) => {
                self.handle_input_overlay_key(&mut overlay, code, modifiers)
            }
            None => AppAction::None,
        }
    }

    fn handle_search_overlay_key(
        &mut self,
        overlay: &mut SearchOverlay,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> AppAction {
        match code {
            KeyCode::Esc => {
                self.overlay = None;
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !overlay.results.is_empty() {
                    overlay.selected = overlay.selected.saturating_sub(1);
                }
                self.overlay = Some(OverlayState::Search(overlay.clone()));
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !overlay.results.is_empty() {
                    overlay.selected = (overlay.selected + 1).min(overlay.results.len() - 1);
                }
                self.overlay = Some(OverlayState::Search(overlay.clone()));
                AppAction::None
            }
            KeyCode::Backspace => {
                overlay.query.pop();
                self.overlay = Some(OverlayState::Search(overlay.clone()));
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
                self.overlay = Some(OverlayState::Search(overlay.clone()));
                self.rebuild_search_results();
                AppAction::None
            }
            _ => {
                self.overlay = Some(OverlayState::Search(overlay.clone()));
                AppAction::None
            }
        }
    }

    fn handle_palette_overlay_key(
        &mut self,
        overlay: &mut ActionPaletteOverlay,
        code: KeyCode,
    ) -> AppAction {
        match code {
            KeyCode::Esc => {
                self.overlay = None;
                AppAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !overlay.items.is_empty() {
                    overlay.selected = overlay.selected.saturating_sub(1);
                }
                self.overlay = Some(OverlayState::ActionPalette(overlay.clone()));
                AppAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !overlay.items.is_empty() {
                    overlay.selected = (overlay.selected + 1).min(overlay.items.len() - 1);
                }
                self.overlay = Some(OverlayState::ActionPalette(overlay.clone()));
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
                self.overlay = Some(OverlayState::ActionPalette(overlay.clone()));
                AppAction::None
            }
        }
    }

    fn handle_confirm_overlay_key(&mut self, confirm: ConfirmOverlay, code: KeyCode) -> AppAction {
        match code {
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
        }
    }

    fn handle_worktree_overlay_key(
        &mut self,
        overlay: &mut WorktreeOverlay,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> AppAction {
        match code {
            KeyCode::Esc => {
                self.overlay = None;
                AppAction::None
            }
            KeyCode::Backspace => {
                overlay.branch_input.pop();
                overlay.target_preview = super::super::util::worktree_preview_path(
                    &overlay.source_cwd,
                    &overlay.branch_input,
                );
                self.overlay = Some(OverlayState::Worktree(overlay.clone()));
                AppAction::None
            }
            KeyCode::Enter => {
                let branch = overlay.branch_input.trim();
                if branch.is_empty() {
                    self.overlay = Some(OverlayState::Worktree(overlay.clone()));
                    AppAction::None
                } else {
                    self.overlay = None;
                    AppAction::SpawnWorktree {
                        source_cwd: overlay.source_cwd.clone(),
                        branch: branch.to_string(),
                    }
                }
            }
            KeyCode::Char(ch)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                overlay.branch_input.push(ch);
                overlay.target_preview = super::super::util::worktree_preview_path(
                    &overlay.source_cwd,
                    &overlay.branch_input,
                );
                self.overlay = Some(OverlayState::Worktree(overlay.clone()));
                AppAction::None
            }
            _ => {
                self.overlay = Some(OverlayState::Worktree(overlay.clone()));
                AppAction::None
            }
        }
    }

    fn handle_input_overlay_key(
        &mut self,
        overlay: &mut InputOverlay,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> AppAction {
        match code {
            KeyCode::Esc => {
                self.overlay = None;
                AppAction::None
            }
            KeyCode::Backspace => {
                overlay.value.pop();
                self.overlay = Some(OverlayState::Input(overlay.clone()));
                AppAction::None
            }
            KeyCode::Enter => {
                let value = overlay.value.trim().to_string();
                self.overlay = None;
                if value.is_empty() {
                    AppAction::None
                } else {
                    match &overlay.submit {
                        InputOverlayKind::RenameScreen { screen_id } => AppAction::RenameScreen {
                            screen_id: screen_id.clone(),
                            new_name: value,
                        },
                        InputOverlayKind::AddWorkspace => AppAction::AddWorkspace(value),
                    }
                }
            }
            KeyCode::Char(ch)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                overlay.value.push(ch);
                self.overlay = Some(OverlayState::Input(overlay.clone()));
                AppAction::None
            }
            _ => {
                self.overlay = Some(OverlayState::Input(overlay.clone()));
                AppAction::None
            }
        }
    }

    pub(super) fn open_search_overlay(&mut self) {
        self.overlay = Some(OverlayState::Search(SearchOverlay {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
        }));
        self.rebuild_search_results();
    }

    pub(super) fn open_help_overlay(&mut self) {
        self.overlay = Some(OverlayState::Help);
    }

    pub(super) fn rebuild_search_results(&mut self) {
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
                let haystack = format!(
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

                if query.is_empty() || haystack.contains(&query) {
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
                self.focus = FocusPane::Browser;
            }
        }

        self.overlay = None;
        self.recompute_visible();
        AppAction::None
    }

    pub(super) fn open_action_palette(&mut self) {
        let mut items = Vec::new();
        if self.subject_screen().is_some() {
            items.extend([
                ActionPaletteItem {
                    label: "Attach screen".to_string(),
                    detail: "Open the selected screen session".to_string(),
                    command: PaletteCommand::AttachSelected,
                },
                ActionPaletteItem {
                    label: "Open worktree".to_string(),
                    detail: "Create or reuse a sibling worktree and spawn a screen there"
                        .to_string(),
                    command: PaletteCommand::OpenWorktree,
                },
                ActionPaletteItem {
                    label: "Interrupt screen".to_string(),
                    detail: "Send Ctrl-C to the selected screen".to_string(),
                    command: PaletteCommand::InterruptSelected,
                },
                ActionPaletteItem {
                    label: "Rename screen".to_string(),
                    detail: "Rename the selected screen session".to_string(),
                    command: PaletteCommand::RenameScreen,
                },
                ActionPaletteItem {
                    label: "Close screen".to_string(),
                    detail: "Quit the selected screen session".to_string(),
                    command: PaletteCommand::ConfirmKill,
                },
            ]);
        }

        if self.subject_workspace().is_some() {
            items.extend([
                ActionPaletteItem {
                    label: "Spawn in workspace".to_string(),
                    detail: "Launch a new Codex screen in this workspace".to_string(),
                    command: PaletteCommand::SpawnWorkspace,
                },
                ActionPaletteItem {
                    label: "Toggle pin".to_string(),
                    detail: "Pin or unpin the selected workspace".to_string(),
                    command: PaletteCommand::TogglePinWorkspace,
                },
            ]);
        }

        items.push(ActionPaletteItem {
            label: "Add workspace".to_string(),
            detail: "Register a repo path in the global workspace index".to_string(),
            command: PaletteCommand::AddWorkspace,
        });

        for mode in [
            BrowserMode::Screens,
            BrowserMode::Workspaces,
            BrowserMode::Attention,
            BrowserMode::Running,
            BrowserMode::Recent,
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

    pub(super) fn open_kill_confirm(&mut self) {
        let Some(screen) = self.subject_screen() else {
            return;
        };
        self.overlay = Some(OverlayState::Confirm(ConfirmOverlay {
            title: "Close screen".to_string(),
            body: format!("Quit {} ({})?", screen.screen_name, screen.screen_id),
            action: AppAction::KillScreen(screen.screen_id.clone()),
        }));
    }

    pub(super) fn open_worktree_overlay(&mut self) {
        let Some(screen) = self.subject_screen() else {
            return;
        };
        self.overlay = Some(OverlayState::Worktree(WorktreeOverlay {
            source_cwd: screen.cwd.clone(),
            branch_input: screen.branch.clone(),
            target_preview: super::super::util::worktree_preview_path(&screen.cwd, &screen.branch),
        }));
    }

    pub(super) fn open_rename_overlay(&mut self) {
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

    pub(super) fn open_add_workspace_overlay(&mut self) {
        self.overlay = Some(OverlayState::Input(InputOverlay {
            title: "Add workspace".to_string(),
            hint: "Register an absolute repo path in the global workspace list".to_string(),
            value: String::new(),
            submit: InputOverlayKind::AddWorkspace,
        }));
    }
}
