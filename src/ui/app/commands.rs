use super::parse_prefixed_index;
use crate::ui::{App, AppAction, BrowserMode, FocusPane, InputMode};
use crossterm::event::{KeyCode, KeyModifiers};

impl App {
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
                self.focus = self.next_focus();
                AppAction::None
            }
            KeyCode::BackTab => {
                self.focus = self.prev_focus();
                AppAction::None
            }
            KeyCode::Char('1') => {
                self.set_browser_mode(BrowserMode::Screens);
                AppAction::None
            }
            KeyCode::Char('2') => {
                self.set_browser_mode(BrowserMode::Workspaces);
                AppAction::None
            }
            KeyCode::Char('3') => {
                self.set_browser_mode(BrowserMode::Attention);
                AppAction::None
            }
            KeyCode::Char('4') => {
                self.set_browser_mode(BrowserMode::Running);
                AppAction::None
            }
            KeyCode::Char('5') => {
                self.set_browser_mode(BrowserMode::Recent);
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

        self.run_named_command(command)
    }

    fn run_named_command(&mut self, command: &str) -> AppAction {
        let mut parts = command.split_whitespace();
        match parts.next().unwrap_or_default() {
            "q" | "quit" | "exit" => AppAction::Quit,
            "workspaces" => self.switch_browser_command(BrowserMode::Workspaces),
            "attention" => self.switch_browser_command(BrowserMode::Attention),
            "running" => self.switch_browser_command(BrowserMode::Running),
            "recent" => self.switch_browser_command(BrowserMode::Recent),
            "screens" => self.switch_browser_command(BrowserMode::Screens),
            "spawn" | "new" => self.spawn_selected_workspace(),
            "attach" => self
                .subject_screen()
                .map(|screen| AppAction::Attach(screen.screen_id.clone()))
                .unwrap_or(AppAction::None),
            "wt" | "worktree" => self.run_worktree_command(parts.collect::<Vec<_>>().join(" ")),
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
            "rename" => self.run_rename_command(parts.collect::<Vec<_>>().join(" ")),
            "add" => self.run_add_workspace_command(parts.collect::<Vec<_>>().join(" ")),
            _ => {
                self.set_error(format!("unknown command: {command}"));
                AppAction::None
            }
        }
    }

    fn switch_browser_command(&mut self, mode: BrowserMode) -> AppAction {
        self.set_browser_mode(mode);
        AppAction::None
    }

    fn run_worktree_command(&mut self, branch: String) -> AppAction {
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

    fn run_rename_command(&mut self, value: String) -> AppAction {
        self.subject_screen()
            .filter(|_| !value.is_empty())
            .map(|screen| AppAction::RenameScreen {
                screen_id: screen.screen_id.clone(),
                new_name: value,
            })
            .unwrap_or(AppAction::None)
    }

    fn run_add_workspace_command(&mut self, value: String) -> AppAction {
        if value.is_empty() {
            self.open_add_workspace_overlay();
            AppAction::None
        } else {
            AppAction::AddWorkspace(value)
        }
    }

    fn next_focus(&self) -> FocusPane {
        if self.browser_mode == BrowserMode::Screens {
            match self.focus {
                FocusPane::Browser => FocusPane::Inspector,
                FocusPane::Context => FocusPane::Inspector,
                FocusPane::Inspector => FocusPane::Browser,
            }
        } else {
            match self.focus {
                FocusPane::Browser => FocusPane::Context,
                FocusPane::Context => FocusPane::Inspector,
                FocusPane::Inspector => FocusPane::Browser,
            }
        }
    }

    fn prev_focus(&self) -> FocusPane {
        if self.browser_mode == BrowserMode::Screens {
            match self.focus {
                FocusPane::Browser => FocusPane::Inspector,
                FocusPane::Context => FocusPane::Browser,
                FocusPane::Inspector => FocusPane::Browser,
            }
        } else {
            match self.focus {
                FocusPane::Browser => FocusPane::Inspector,
                FocusPane::Context => FocusPane::Browser,
                FocusPane::Inspector => FocusPane::Context,
            }
        }
    }
}
