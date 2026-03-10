mod app;
mod render;
mod util;
mod worker;

#[cfg(test)]
mod tests;

use crate::types::SessionRow;
use ratatui::widgets::TableState;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SortMode {
    Attention,
    Screen,
    Branch,
    Updated,
}

impl SortMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Attention => "attention",
            Self::Screen => "screen",
            Self::Branch => "branch",
            Self::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AppAction {
    None,
    Quit,
    Attach(String),
    Spawn(String),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
    Command,
}

pub struct App {
    pub(super) rows: Vec<SessionRow>,
    pub(super) visible_indices: Vec<usize>,
    pub(super) selected: usize,
    pub(super) selected_screen_id: Option<String>,
    pub(super) table_state: TableState,
    pub(super) mode: InputMode,
    pub(super) search_query: String,
    pub(super) command: String,
    pub(super) sort_mode: SortMode,
    pub(super) last_error: Option<String>,
    pub(super) last_info: Option<String>,
    pub(super) refresh_tx: Sender<()>,
    pub(super) refresh_rx: Receiver<Result<Vec<SessionRow>, String>>,
    pub(super) refresh_in_flight: bool,
    pub(super) refresh_queued: bool,
}

impl App {
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_secs(3)
    }
}
