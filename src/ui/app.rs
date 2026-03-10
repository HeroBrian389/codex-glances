use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
use std::time::Duration;

use crate::data::DataCollector;
use crate::types::SessionRow;

use super::util::row_matches_query;
use super::worker::spawn_refresh_worker;
use super::{App, AppAction, InputMode, SortMode};

impl App {
    pub fn new(collector: DataCollector) -> Self {
        let (refresh_tx, refresh_rx) = spawn_refresh_worker(collector);
        Self {
            rows: Vec::new(),
            visible_indices: Vec::new(),
            selected: 0,
            selected_screen_id: None,
            table_state: ratatui::widgets::TableState::default(),
            mode: InputMode::Normal,
            search_query: String::new(),
            command: String::new(),
            sort_mode: SortMode::Attention,
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

    fn apply_refresh_result(&mut self, result: Result<Vec<SessionRow>, String>) {
        self.refresh_in_flight = false;
        match result {
            Ok(rows) => {
                self.rows = rows;
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
            KeyCode::Char('s') => {
                self.mode = InputMode::Command;
                self.command = "s".to_string();
                AppAction::None
            }
            KeyCode::Char('n') => {
                self.mode = InputMode::Command;
                self.command = "n".to_string();
                AppAction::None
            }
            KeyCode::Char('N') if modifiers.contains(KeyModifiers::SHIFT) => self.spawn_selected(),
            KeyCode::Char('c') => {
                self.search_query.clear();
                self.recompute_visible();
                AppAction::None
            }
            KeyCode::Char('1') => self.set_sort_mode(SortMode::Attention),
            KeyCode::Char('2') => self.set_sort_mode(SortMode::Screen),
            KeyCode::Char('3') => self.set_sort_mode(SortMode::Branch),
            KeyCode::Char('4') => self.set_sort_mode(SortMode::Updated),
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
            KeyCode::Enter => self.attach_selected(),
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

    fn set_sort_mode(&mut self, sort_mode: SortMode) -> AppAction {
        self.sort_mode = sort_mode;
        self.recompute_visible();
        AppAction::None
    }

    pub(super) fn recompute_visible(&mut self) {
        self.selected_screen_id = self.selected_row().map(|row| row.screen_id.clone());
        let query = self.search_query.to_lowercase();
        self.visible_indices = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(idx, row)| {
                (query.is_empty() || row_matches_query(row, &query)).then_some(idx)
            })
            .collect();

        self.visible_indices.sort_by(|a, b| {
            let left = &self.rows[*a];
            let right = &self.rows[*b];
            match self.sort_mode {
                SortMode::Attention => {
                    let key_left = (
                        !left.needs_attention,
                        left.status.rank(),
                        left.screen_name.clone(),
                    );
                    let key_right = (
                        !right.needs_attention,
                        right.status.rank(),
                        right.screen_name.clone(),
                    );
                    key_left.cmp(&key_right)
                }
                SortMode::Screen => left.screen_name.cmp(&right.screen_name),
                SortMode::Branch => {
                    let key_left = (left.branch.clone(), left.screen_name.clone());
                    let key_right = (right.branch.clone(), right.screen_name.clone());
                    key_left.cmp(&key_right)
                }
                SortMode::Updated => {
                    let key_left = (left.last_update, left.screen_name.clone());
                    let key_right = (right.last_update, right.screen_name.clone());
                    key_right.cmp(&key_left)
                }
            }
        });

        if self.visible_indices.is_empty() {
            self.selected = 0;
            self.selected_screen_id = None;
            self.table_state.select(None);
            return;
        }

        self.selected = self
            .selected_screen_id
            .as_ref()
            .and_then(|screen_id| {
                self.visible_indices.iter().position(|idx| {
                    self.rows
                        .get(*idx)
                        .is_some_and(|row| &row.screen_id == screen_id)
                })
            })
            .unwrap_or_else(|| self.selected.min(self.visible_indices.len() - 1));
        self.selected_screen_id = self.selected_row().map(|row| row.screen_id.clone());
        self.table_state.select(Some(self.selected));
    }

    fn move_selection(&mut self, delta: isize) {
        if self.visible_indices.is_empty() {
            self.selected = 0;
            self.table_state.select(None);
            return;
        }
        let len = self.visible_indices.len() as isize;
        let next = (self.selected as isize + delta).clamp(0, len - 1);
        self.selected = next as usize;
        self.table_state.select(Some(self.selected));
        self.selected_screen_id = self.selected_row().map(|row| row.screen_id.clone());
    }

    fn select_first(&mut self) {
        if self.visible_indices.is_empty() {
            self.table_state.select(None);
            return;
        }
        self.selected = 0;
        self.table_state.select(Some(0));
        self.selected_screen_id = self.selected_row().map(|row| row.screen_id.clone());
    }

    fn select_last(&mut self) {
        if self.visible_indices.is_empty() {
            self.table_state.select(None);
            return;
        }
        self.selected = self.visible_indices.len() - 1;
        self.table_state.select(Some(self.selected));
        self.selected_screen_id = self.selected_row().map(|row| row.screen_id.clone());
    }

    pub(super) fn selected_row(&self) -> Option<&SessionRow> {
        let idx = self.visible_indices.get(self.selected)?;
        self.rows.get(*idx)
    }

    fn attach_selected(&mut self) -> AppAction {
        if let Some(row) = self.selected_row() {
            AppAction::Attach(row.screen_id.clone())
        } else {
            self.last_error = Some("nothing selected".to_string());
            AppAction::None
        }
    }

    fn spawn_selected(&mut self) -> AppAction {
        let Some(row) = self.selected_row() else {
            self.last_error = Some("nothing selected".to_string());
            return AppAction::None;
        };

        if row.cwd == "-" {
            self.last_error = Some("selected row has no folder".to_string());
            return AppAction::None;
        }

        AppAction::Spawn(row.cwd.clone())
    }

    fn execute_command(&mut self) -> AppAction {
        let normalized = self.command.trim().trim_start_matches(':');
        if normalized.is_empty() {
            return AppAction::None;
        }

        if let Some(num_str) = normalized.strip_prefix('s')
            && let Ok(n) = num_str.parse::<usize>()
        {
            if (1..=self.visible_indices.len()).contains(&n) {
                let row_idx = self.visible_indices[n - 1];
                return AppAction::Attach(self.rows[row_idx].screen_id.clone());
            }
            self.last_error = Some(format!("shortcut out of range: s{n}"));
            return AppAction::None;
        }

        if let Some(num_str) = normalized.strip_prefix('n')
            && let Ok(n) = num_str.parse::<usize>()
        {
            if (1..=self.visible_indices.len()).contains(&n) {
                let row_idx = self.visible_indices[n - 1];
                let cwd = self.rows[row_idx].cwd.clone();
                if cwd == "-" {
                    self.last_error = Some(format!("row n{n} has no folder"));
                    return AppAction::None;
                }
                return AppAction::Spawn(cwd);
            }
            self.last_error = Some(format!("shortcut out of range: n{n}"));
            return AppAction::None;
        }

        if let Some(row) = self
            .rows
            .iter()
            .find(|row| row.screen_id == normalized || row.screen_name == normalized)
        {
            return AppAction::Attach(row.screen_id.clone());
        }

        self.last_error = Some(format!("unknown command: {normalized}"));
        AppAction::None
    }
}
