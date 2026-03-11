use super::style::{pane_title, screen_row_style};
use crate::ui::{App, BrowserMode, FocusPane};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::{Alignment, Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

impl App {
    pub(super) fn draw_browser_pane(&mut self, frame: &mut Frame, area: Rect) {
        if self.browser_mode == BrowserMode::Screens {
            self.draw_screens_browser(frame, area);
        } else {
            self.draw_workspaces_browser(frame, area);
        }
    }

    fn draw_workspaces_browser(&mut self, frame: &mut Frame, area: Rect) {
        let title = pane_title(
            "Browser",
            self.focus == FocusPane::Browser,
            format!("{} view", self.browser_mode.as_str()),
        );

        if self.visible_workspace_indices.is_empty() {
            frame.render_widget(
                Paragraph::new("No workspaces match this view")
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title(title)),
                area,
            );
            return;
        }

        let header = Row::new(["", "Workspace", "Activity", "Branch", "Age"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let rows = self
            .visible_workspace_indices
            .iter()
            .map(|workspace_idx| {
                let workspace = &self.data.workspaces[*workspace_idx];
                Row::new(vec![
                    Cell::from(if workspace.pinned { "P" } else { " " }),
                    Cell::from(super::super::util::truncate_chars(
                        &workspace.display_name,
                        20,
                    )),
                    Cell::from(super::super::util::workspace_activity_badge(workspace)),
                    Cell::from(super::super::util::truncate_chars(
                        &workspace.branch_label,
                        16,
                    )),
                    Cell::from(super::super::util::age_string(workspace.last_update)),
                ])
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Percentage(32),
                Constraint::Percentage(22),
                Constraint::Percentage(27),
                Constraint::Length(6),
            ],
        )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(super::super::util::highlight_style(
            self.focus == FocusPane::Browser,
        ));

        frame.render_stateful_widget(table, area, &mut self.browser_table_state);
    }

    fn draw_screens_browser(&mut self, frame: &mut Frame, area: Rect) {
        let title = pane_title(
            "Browser",
            self.focus == FocusPane::Browser,
            "screens".to_string(),
        );

        if self.visible_screen_refs.is_empty() {
            frame.render_widget(
                Paragraph::new("No live screen sessions detected")
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title(title)),
                area,
            );
            return;
        }

        let header = Row::new(["Workspace", "Screen", "State", "Age", "Preview"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let rows = self
            .visible_screen_refs
            .iter()
            .filter_map(|screen_ref| {
                let workspace = self.data.workspaces.get(screen_ref.workspace_idx)?;
                let screen = self.screen_by_ref(*screen_ref)?;
                Some(
                    Row::new(vec![
                        Cell::from(super::super::util::truncate_chars(
                            &workspace.display_name,
                            16,
                        )),
                        Cell::from(super::super::util::truncate_chars(&screen.screen_name, 18)),
                        Cell::from(screen.status.as_str()),
                        Cell::from(super::super::util::age_string(screen.last_update)),
                        Cell::from(super::super::util::truncate_chars(
                            &screen.status_reason,
                            24,
                        )),
                    ])
                    .style(screen_row_style(screen)),
                )
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(22),
                Constraint::Percentage(24),
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Percentage(48),
            ],
        )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(super::super::util::highlight_style(
            self.focus == FocusPane::Browser,
        ));

        frame.render_stateful_widget(table, area, &mut self.browser_table_state);
    }

    pub(super) fn draw_context_pane(&mut self, frame: &mut Frame, area: Rect) {
        let title = pane_title(
            "Context",
            self.focus == FocusPane::Context,
            self.subject_workspace()
                .map(|workspace| workspace.display_name.clone())
                .unwrap_or_else(|| "no workspace".to_string()),
        );

        let refs = self.context_screen_refs();
        if refs.is_empty() {
            let message = self
                .subject_workspace()
                .map(|workspace| {
                    if workspace.session_count == 0 {
                        "No live screens in this workspace".to_string()
                    } else {
                        "No screens available".to_string()
                    }
                })
                .unwrap_or_else(|| "Select a workspace or screen".to_string());

            frame.render_widget(
                Paragraph::new(message)
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title(title)),
                area,
            );
            return;
        }

        let header = Row::new(["Screen", "Branch", "State", "Age", "Reason"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let rows = refs
            .iter()
            .filter_map(|screen_ref| {
                let screen = self.screen_by_ref(*screen_ref)?;
                Some(
                    Row::new(vec![
                        Cell::from(super::super::util::truncate_chars(&screen.screen_name, 16)),
                        Cell::from(super::super::util::truncate_chars(&screen.branch, 16)),
                        Cell::from(screen.status.as_str()),
                        Cell::from(super::super::util::age_string(screen.last_update)),
                        Cell::from(super::super::util::truncate_chars(
                            &screen.status_reason,
                            24,
                        )),
                    ])
                    .style(screen_row_style(screen)),
                )
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(24),
                Constraint::Percentage(24),
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Percentage(46),
            ],
        )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(super::super::util::highlight_style(
            self.focus == FocusPane::Context,
        ));

        frame.render_stateful_widget(table, area, &mut self.context_table_state);
    }
}
