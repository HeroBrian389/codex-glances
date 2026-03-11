use super::{compare_optional_desc, compare_screen_rows};
use crate::types::DashboardData;
use crate::ui::{App, BrowserMode, OverlayState, ScreenRef};
use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
use std::time::Duration;

impl App {
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
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
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
            Err(err) => self.set_error(err),
        }
    }

    pub fn recompute_visible(&mut self) {
        self.visible_workspace_indices = self.compute_visible_workspace_indices();
        self.visible_screen_refs = self.compute_visible_screen_refs();

        self.refresh_workspace_selection();
        self.refresh_screen_selection();
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

        refs.sort_by(
            |left, right| match (self.screen_by_ref(*left), self.screen_by_ref(*right)) {
                (Some(left_row), Some(right_row)) => compare_screen_rows(left_row, right_row),
                _ => std::cmp::Ordering::Equal,
            },
        );

        refs
    }

    fn refresh_workspace_selection(&mut self) {
        if self.visible_workspace_indices.is_empty() {
            self.browser_index = 0;
            self.selected_workspace_key = None;
            self.browser_table_state.select(None);
            return;
        }

        let browser_index = self
            .selected_workspace_key
            .as_deref()
            .and_then(|selected_key| {
                self.visible_workspace_indices
                    .iter()
                    .position(|idx| self.data.workspaces[*idx].key == selected_key)
            })
            .unwrap_or(
                self.browser_index
                    .min(self.visible_workspace_indices.len().saturating_sub(1)),
            );

        self.browser_index = browser_index;
        self.selected_workspace_key = self
            .visible_workspace_indices
            .get(self.browser_index)
            .map(|idx| self.data.workspaces[*idx].key.clone());

        if self.browser_mode != BrowserMode::Screens {
            self.browser_table_state.select(Some(self.browser_index));
        }
    }

    fn refresh_screen_selection(&mut self) {
        if self.visible_screen_refs.is_empty() {
            self.selected_browser_screen_id = None;
            if self.browser_mode == BrowserMode::Screens {
                self.browser_index = 0;
                self.browser_table_state.select(None);
            }
            return;
        }

        let browser_index = self
            .selected_browser_screen_id
            .as_deref()
            .and_then(|selected_screen_id| {
                self.visible_screen_refs.iter().position(|screen_ref| {
                    self.screen_by_ref(*screen_ref)
                        .is_some_and(|row| row.screen_id == selected_screen_id)
                })
            })
            .unwrap_or(
                self.browser_index
                    .min(self.visible_screen_refs.len().saturating_sub(1)),
            );

        self.selected_browser_screen_id = self
            .visible_screen_refs
            .get(browser_index)
            .and_then(|screen_ref| self.screen_by_ref(*screen_ref))
            .map(|row| row.screen_id.clone());

        if self.browser_mode == BrowserMode::Screens {
            self.browser_index = browser_index;
            self.browser_table_state.select(Some(self.browser_index));
        }
    }
}
