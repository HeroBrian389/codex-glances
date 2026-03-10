use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::{Alignment, Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};

use crate::types::SessionStatus;

use super::util::{age_string, centered_rect, shorten_home, truncate_chars};
use super::{App, InputMode};

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(8),
                Constraint::Length(3),
            ])
            .split(frame.area());

        self.draw_summary(frame, areas[0]);
        self.draw_table(frame, areas[1]);
        self.draw_details(frame, areas[2]);
        self.draw_footer(frame, areas[3]);

        if matches!(self.mode, InputMode::Search | InputMode::Command) {
            self.draw_overlay_prompt(frame);
        }
    }

    fn draw_summary(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let total = self.rows.len();
        let running = self
            .rows
            .iter()
            .filter(|row| row.status == SessionStatus::Running)
            .count();
        let waiting = self
            .rows
            .iter()
            .filter(|row| row.status == SessionStatus::WaitingInput)
            .count();
        let attention = self.rows.iter().filter(|row| row.needs_attention).count();
        let follow_ups = self
            .rows
            .iter()
            .map(|row| row.scheduled_follow_ups)
            .sum::<usize>();

        let line = Line::from(vec![
            Span::styled(
                " CODEx GLANCES ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(format!("Total: {total}"), Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled(
                format!("Running: {running}"),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Waiting: {waiting}"),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Needs attention: {attention}"),
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Follow-ups: {follow_ups}"),
                Style::default().fg(Color::Magenta),
            ),
        ]);

        frame.render_widget(
            Paragraph::new(vec![line])
                .block(Block::default().title("Overview").borders(Borders::ALL))
                .alignment(Alignment::Left),
            area,
        );
    }

    fn draw_table(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        if self.rows.is_empty() && self.is_refresh_in_flight() {
            frame.render_widget(
                Paragraph::new("Loading sessions...")
                    .block(Block::default().title("Sessions").borders(Borders::ALL))
                    .alignment(Alignment::Center),
                area,
            );
            return;
        }

        let header = Row::new(vec![
            Cell::from("!"),
            Cell::from("Shortcut"),
            Cell::from("Screen"),
            Cell::from("Status"),
            Cell::from("Follow"),
            Cell::from("Branch"),
            Cell::from("Folder"),
            Cell::from("Last Agent"),
            Cell::from("Last Event"),
            Cell::from("Age"),
            Cell::from("Thread"),
        ])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let rows = self
            .visible_indices
            .iter()
            .enumerate()
            .map(|(display_idx, row_idx)| {
                let row = &self.rows[*row_idx];
                let style = match row.status {
                    SessionStatus::Running => Style::default().fg(Color::Green),
                    SessionStatus::WaitingInput => Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    SessionStatus::Idle => Style::default().fg(Color::Blue),
                    SessionStatus::Unknown => Style::default().fg(Color::DarkGray),
                };
                let thread_short = if row.thread_id.is_empty() {
                    "-".to_string()
                } else {
                    truncate_chars(&row.thread_id, 12)
                };
                let follow_ups = if row.scheduled_follow_ups == 0 {
                    "-".to_string()
                } else {
                    row.scheduled_follow_ups.to_string()
                };

                Row::new(vec![
                    Cell::from(if row.needs_attention { "*" } else { " " }),
                    Cell::from(format!("s{}", display_idx + 1)),
                    Cell::from(truncate_chars(&row.screen_name, 24)),
                    Cell::from(row.status.as_str().to_string()),
                    Cell::from(follow_ups),
                    Cell::from(truncate_chars(&row.branch, 20)),
                    Cell::from(truncate_chars(&shorten_home(&row.cwd), 28)),
                    Cell::from(truncate_chars(&row.last_agent, 24)),
                    Cell::from(row.last_event.clone()),
                    Cell::from(age_string(row.last_update)),
                    Cell::from(thread_short),
                ])
                .style(style)
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Length(9),
                Constraint::Length(24),
                Constraint::Length(10),
                Constraint::Length(6),
                Constraint::Length(20),
                Constraint::Length(28),
                Constraint::Length(24),
                Constraint::Length(12),
                Constraint::Length(8),
                Constraint::Length(14),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(format!(
                    "Sessions | sort={} | filter='{}'",
                    self.sort_mode.as_str(),
                    self.search_query
                ))
                .borders(Borders::ALL),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn draw_details(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        if self.rows.is_empty() && self.is_refresh_in_flight() {
            frame.render_widget(
                Paragraph::new("Collecting screen and session state...")
                    .block(Block::default().title("Details").borders(Borders::ALL)),
                area,
            );
            return;
        }

        let Some(row) = self.selected_row() else {
            frame.render_widget(
                Paragraph::new("No active rows")
                    .block(Block::default().title("Details").borders(Borders::ALL)),
                area,
            );
            return;
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Screen: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&row.screen_id),
                Span::raw("   "),
                Span::styled("Branch: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&row.branch),
            ]),
            Line::from(vec![
                Span::styled("Folder: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(shorten_home(&row.cwd)),
            ]),
            Line::from(vec![
                Span::styled("Last user: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&row.last_user),
            ]),
            Line::from(vec![
                Span::styled(
                    "Follow-ups: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(if row.scheduled_follow_ups == 0 {
                    "none".to_string()
                } else {
                    format!("{} scheduled", row.scheduled_follow_ups)
                }),
            ]),
            Line::from(vec![
                Span::styled(
                    "Last agent: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(&row.last_agent),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default().title("Details").borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn draw_footer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mut line = vec![
            hotkey("q"),
            Span::raw(" quit  "),
            hotkey("r"),
            Span::raw(" refresh  "),
            hotkey("/"),
            Span::raw(" search  "),
            hotkey("sNN"),
            Span::raw(" attach  "),
            hotkey("nNN"),
            Span::raw(" spawn  "),
            hotkey("Enter"),
            Span::raw(" attach selected  "),
            hotkey("N"),
            Span::raw(" spawn selected  "),
            hotkey("1..4"),
            Span::raw(" sort"),
        ];

        if let Some(err) = &self.last_error {
            line.push(Span::raw("  |  "));
            line.push(Span::styled(
                truncate_chars(err, 120),
                Style::default().fg(Color::LightRed),
            ));
        } else if self.refresh_in_flight {
            line.push(Span::raw("  |  "));
            line.push(Span::styled(
                "refreshing",
                Style::default().fg(Color::Yellow),
            ));
        } else if let Some(info) = &self.last_info {
            line.push(Span::raw("  |  "));
            line.push(Span::styled(
                truncate_chars(info, 120),
                Style::default().fg(Color::Green),
            ));
        }

        frame.render_widget(
            Paragraph::new(Line::from(line))
                .block(Block::default().title("Keybinds").borders(Borders::ALL)),
            area,
        );
    }

    fn draw_overlay_prompt(&self, frame: &mut Frame) {
        let popup = centered_rect(70, 20, frame.area());
        let (title, value, hint) = if self.mode == InputMode::Search {
            (
                "Search",
                self.search_query.as_str(),
                "Type to filter sessions. Esc/Enter to close.",
            )
        } else {
            (
                "Command",
                self.command.as_str(),
                "Examples: s3 attach, n3 spawn in row folder, or full screen id (7014.s1)",
            )
        };

        let content = vec![
            Line::from(Span::raw(value.to_string())),
            Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))),
        ];
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(content)
                .block(Block::default().title(title).borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            popup,
        );
    }
}

fn hotkey(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}
