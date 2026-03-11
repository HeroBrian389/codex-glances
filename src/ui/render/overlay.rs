use crate::ui::{App, InputMode, OverlayState};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Alignment, Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table, Wrap};

impl App {
    pub(super) fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let status = if let Some(overlay) = &self.overlay {
            match overlay {
                OverlayState::Search(search) => format!(
                    "Search {} results | type to filter, Enter to jump, Esc to close",
                    search.results.len()
                ),
                OverlayState::ActionPalette(_) => {
                    "Action palette | Enter to run, Esc to close".to_string()
                }
                OverlayState::Confirm(_) => {
                    "Confirm | Enter or y to accept, Esc or n to cancel".to_string()
                }
                OverlayState::Worktree(_) => {
                    "Worktree | edit branch, Enter to spawn and attach".to_string()
                }
                OverlayState::Input(input) => format!("{} | Enter to submit", input.title),
            }
        } else if self.mode == InputMode::Command {
            format!(":{}", self.command)
        } else if let Some(error) = &self.last_error {
            error.clone()
        } else if let Some(info) = &self.last_info {
            info.clone()
        } else {
            "Tab cycles panes, / searches globally, a opens actions, 5 shows all screens"
                .to_string()
        };

        frame.render_widget(
            Paragraph::new(status)
                .block(Block::default().borders(Borders::TOP))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    pub(super) fn draw_overlay(&self, frame: &mut Frame) {
        if self.mode == InputMode::Command {
            self.draw_command_overlay(frame);
            return;
        }

        let Some(overlay) = &self.overlay else {
            return;
        };

        match overlay {
            OverlayState::Search(search) => self.draw_search_overlay(frame, search),
            OverlayState::ActionPalette(palette) => self.draw_palette_overlay(frame, palette),
            OverlayState::Confirm(confirm) => self.draw_confirm_overlay(frame, confirm),
            OverlayState::Worktree(worktree) => self.draw_worktree_overlay(frame, worktree),
            OverlayState::Input(input) => self.draw_input_overlay(frame, input),
        }
    }

    fn draw_command_overlay(&self, frame: &mut Frame) {
        let area = super::super::util::centered_rect(72, 18, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(format!(":{}", self.command))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Command")
                        .title_bottom("Enter to run, Esc to cancel"),
                )
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_search_overlay(&self, frame: &mut Frame, search: &crate::ui::SearchOverlay) {
        let area = super::super::util::centered_rect(78, 72, frame.area());
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(6)])
            .split(area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(format!("Query: {}", search.query)).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Search")
                    .title_bottom("Type to filter, Enter to jump"),
            ),
            inner[0],
        );

        let rows = search
            .results
            .iter()
            .map(|result| Row::new(vec![result.label.clone(), result.detail.clone()]))
            .collect::<Vec<_>>();
        let table = Table::new(
            rows,
            [Constraint::Percentage(34), Constraint::Percentage(66)],
        )
        .header(
            Row::new(["Item", "Detail"]).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));

        let mut state = ratatui::widgets::TableState::default();
        if !search.results.is_empty() {
            state.select(Some(search.selected));
        }
        frame.render_stateful_widget(table, inner[1], &mut state);
    }

    fn draw_palette_overlay(&self, frame: &mut Frame, palette: &crate::ui::ActionPaletteOverlay) {
        let area = super::super::util::centered_rect(74, 64, frame.area());
        frame.render_widget(Clear, area);
        let rows = palette
            .items
            .iter()
            .map(|item| Row::new(vec![item.label.clone(), item.detail.clone()]))
            .collect::<Vec<_>>();
        let table = Table::new(
            rows,
            [Constraint::Percentage(30), Constraint::Percentage(70)],
        )
        .header(
            Row::new(["Action", "Detail"]).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(palette.title.clone())
                .title_bottom("Enter to run, Esc to cancel"),
        );

        let mut state = ratatui::widgets::TableState::default();
        if !palette.items.is_empty() {
            state.select(Some(palette.selected));
        }
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn draw_confirm_overlay(&self, frame: &mut Frame, confirm: &crate::ui::ConfirmOverlay) {
        let area = super::super::util::centered_rect(50, 24, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::raw(confirm.body.clone()),
                Line::raw(""),
                Line::raw("Enter or y confirms. Esc or n cancels."),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(confirm.title.clone()),
            )
            .alignment(Alignment::Center),
            area,
        );
    }

    fn draw_worktree_overlay(&self, frame: &mut Frame, worktree: &crate::ui::WorktreeOverlay) {
        let area = super::super::util::centered_rect(72, 32, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                super::style::line_kv(
                    "Source",
                    super::super::util::shorten_home(&worktree.source_cwd),
                ),
                super::style::line_kv("Branch", worktree.branch_input.clone()),
                super::style::line_kv(
                    "Preview",
                    super::super::util::shorten_home(&worktree.target_preview),
                ),
                Line::raw(""),
                Line::raw(
                    "Edit the branch and press Enter to create or reuse the sibling worktree.",
                ),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Spawn Worktree")
                    .title_bottom("Enter to spawn and attach"),
            )
            .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_input_overlay(&self, frame: &mut Frame, input: &crate::ui::InputOverlay) {
        let area = super::super::util::centered_rect(68, 26, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::raw(input.hint.clone()),
                Line::raw(""),
                Line::raw(input.value.clone()),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(input.title.clone())
                    .title_bottom("Enter to submit"),
            )
            .wrap(Wrap { trim: true }),
            area,
        );
    }
}
