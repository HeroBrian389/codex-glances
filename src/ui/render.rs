use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::{Alignment, Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};

use crate::types::SessionStatus;

use super::util::{age_string, centered_rect, shorten_home, truncate_chars};
use super::{App, FocusPane, InputMode};

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(8),
                Constraint::Length(3),
            ])
            .split(frame.area());
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
            .split(areas[1]);

        self.draw_summary(frame, areas[0]);
        self.draw_workspace_table(frame, body[0]);
        self.draw_session_table(frame, body[1]);
        self.draw_details(frame, areas[2]);
        self.draw_footer(frame, areas[3]);

        if matches!(self.mode, InputMode::Search | InputMode::Command) {
            self.draw_overlay_prompt(frame);
        }
    }

    fn draw_summary(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let workspace_total = self.data.workspaces.len();
        let active_workspaces = self
            .data
            .workspaces
            .iter()
            .filter(|workspace| workspace.session_count > 0)
            .count();
        let active_sessions = self
            .data
            .workspaces
            .iter()
            .map(|workspace| workspace.session_count)
            .sum::<usize>();
        let waiting = self
            .data
            .workspaces
            .iter()
            .map(|workspace| workspace.waiting_sessions)
            .sum::<usize>();
        let running = self
            .data
            .workspaces
            .iter()
            .map(|workspace| workspace.running_sessions)
            .sum::<usize>();
        let follow_ups = self
            .data
            .workspaces
            .iter()
            .map(|workspace| workspace.follow_ups)
            .sum::<usize>();

        let line = Line::from(vec![
            Span::styled(
                " CODEX GLANCES ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("View: {}", self.view_mode.as_str()),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Workspaces: {workspace_total}"),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Active repos: {active_workspaces}"),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Screens: {active_sessions}"),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Waiting: {waiting}"),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Running: {running}"),
                Style::default().fg(Color::LightBlue),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Follow-ups: {follow_ups}"),
                Style::default().fg(Color::Magenta),
            ),
        ]);

        frame.render_widget(
            Paragraph::new(vec![line])
                .block(
                    Block::default()
                        .title("Global Overview")
                        .borders(Borders::ALL),
                )
                .alignment(Alignment::Left),
            area,
        );
    }

    fn draw_workspace_table(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        if self.data.workspaces.is_empty() && self.is_refresh_in_flight() {
            frame.render_widget(
                Paragraph::new("Loading workspaces...")
                    .block(Block::default().title("Workspaces").borders(Borders::ALL))
                    .alignment(Alignment::Center),
                area,
            );
            return;
        }

        let header = Row::new(vec![
            Cell::from(""),
            Cell::from("Shortcut"),
            Cell::from("Workspace"),
            Cell::from("Sessions"),
            Cell::from("Follow"),
            Cell::from("Branch"),
            Cell::from("Age"),
        ])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let rows = self
            .visible_workspace_indices
            .iter()
            .enumerate()
            .map(|(display_idx, workspace_idx)| {
                let workspace = &self.data.workspaces[*workspace_idx];
                let badge = if workspace.waiting_sessions > 0 {
                    format!(
                        "W{} R{}",
                        workspace.waiting_sessions, workspace.running_sessions
                    )
                } else if workspace.session_count > 0 {
                    format!("{} live", workspace.session_count)
                } else {
                    "saved".to_string()
                };

                Row::new(vec![
                    Cell::from(if workspace.pinned { "P" } else { " " }),
                    Cell::from(format!("w{}", display_idx + 1)),
                    Cell::from(truncate_chars(&workspace.display_name, 20)),
                    Cell::from(badge),
                    Cell::from(if workspace.follow_ups == 0 {
                        "-".to_string()
                    } else {
                        workspace.follow_ups.to_string()
                    }),
                    Cell::from(truncate_chars(&workspace.branch_label, 18)),
                    Cell::from(age_string(workspace.last_update)),
                ])
            })
            .collect::<Vec<_>>();

        let block = Block::default()
            .title(format!(
                "Workspaces | {} | filter='{}'{}",
                self.view_mode.as_str(),
                self.search_query,
                if self.focus == FocusPane::Workspaces {
                    " | focus"
                } else {
                    ""
                }
            ))
            .borders(Borders::ALL);
        let table = Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Length(9),
                Constraint::Length(20),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(18),
                Constraint::Length(8),
            ],
        )
        .header(header)
        .block(block)
        .row_highlight_style(highlight_style(self.focus == FocusPane::Workspaces));

        frame.render_stateful_widget(table, area, &mut self.workspace_table_state);
    }

    fn draw_session_table(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let Some(workspace) = self.selected_workspace() else {
            frame.render_widget(
                Paragraph::new("No matching workspaces")
                    .block(Block::default().title("Sessions").borders(Borders::ALL))
                    .alignment(Alignment::Center),
                area,
            );
            return;
        };

        if workspace.sessions.is_empty() {
            frame.render_widget(
                Paragraph::new("No active screen sessions for this workspace")
                    .block(
                        Block::default()
                            .title(format!("Sessions | {}", workspace.display_name))
                            .borders(Borders::ALL),
                    )
                    .alignment(Alignment::Center),
                area,
            );
            return;
        }

        let header = Row::new(vec![
            Cell::from("Shortcut"),
            Cell::from("Screen"),
            Cell::from("Status"),
            Cell::from("Branch"),
            Cell::from("Age"),
            Cell::from("Last Event"),
        ])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let rows = workspace
            .sessions
            .iter()
            .enumerate()
            .map(|(idx, session)| {
                Row::new(vec![
                    Cell::from(format!("s{}", idx + 1)),
                    Cell::from(truncate_chars(&session.screen_name, 22)),
                    Cell::from(session.status.as_str()),
                    Cell::from(truncate_chars(&session.branch, 18)),
                    Cell::from(age_string(session.last_update)),
                    Cell::from(truncate_chars(&session.last_event, 18)),
                ])
                .style(session_style(session.status))
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Length(9),
                Constraint::Length(22),
                Constraint::Length(10),
                Constraint::Length(18),
                Constraint::Length(8),
                Constraint::Length(18),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(format!(
                    "Sessions | {}{}",
                    workspace.display_name,
                    if self.focus == FocusPane::Sessions {
                        " | focus"
                    } else {
                        ""
                    }
                ))
                .borders(Borders::ALL),
        )
        .row_highlight_style(highlight_style(self.focus == FocusPane::Sessions));

        frame.render_stateful_widget(table, area, &mut self.session_table_state);
    }

    fn draw_details(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let Some(workspace) = self.selected_workspace() else {
            frame.render_widget(
                Paragraph::new("No active rows")
                    .block(Block::default().title("Details").borders(Borders::ALL)),
                area,
            );
            return;
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Workspace: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&workspace.display_name),
                Span::raw("   "),
                Span::styled("Path: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(shorten_home(&workspace.path)),
            ]),
            Line::from(vec![
                Span::styled("Branch: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&workspace.branch_label),
                Span::raw("   "),
                Span::styled(
                    "Follow-ups: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(workspace.follow_ups.to_string()),
                Span::raw("   "),
                Span::styled("Tags: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(if workspace.tags.is_empty() {
                    "-".to_string()
                } else {
                    workspace.tags.join(", ")
                }),
            ]),
            Line::from(vec![
                Span::styled(
                    "Workspace last agent: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(&workspace.last_agent),
            ]),
        ];

        if let Some(session) = self
            .selected_session()
            .or_else(|| workspace.sessions.first())
        {
            lines.push(Line::from(vec![
                Span::styled("Screen: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&session.screen_id),
                Span::raw("   "),
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(session.status.as_str()),
                Span::raw("   "),
                Span::styled(
                    "Follow-ups: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(if session.scheduled_follow_ups == 0 {
                    "none".to_string()
                } else {
                    session.scheduled_follow_ups.to_string()
                }),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Last user: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&session.last_user),
            ]));
            lines.push(Line::from(vec![
                Span::styled(
                    "Last agent: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(&session.last_agent),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("Last user: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&workspace.last_user),
            ]));
        }

        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default().title("Details").borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn draw_footer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mut line = vec![
            hotkey("1..4"),
            Span::raw(" views  "),
            hotkey("Tab"),
            Span::raw(" switch pane  "),
            hotkey("Enter"),
            Span::raw(" attach / spawn  "),
            hotkey("N"),
            Span::raw(" spawn workspace  "),
            hotkey("W"),
            Span::raw(" worktree+attach  "),
            hotkey("p"),
            Span::raw(" pin  "),
            hotkey("x"),
            Span::raw(" kill  "),
            hotkey("i"),
            Span::raw(" interrupt  "),
            hotkey(":"),
            Span::raw(" commands"),
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
                .block(Block::default().title("Controls").borders(Borders::ALL)),
            area,
        );
    }

    fn draw_overlay_prompt(&self, frame: &mut Frame) {
        let popup = centered_rect(72, 20, frame.area());
        let (title, value, hint) = if self.mode == InputMode::Search {
            (
                "Search",
                self.search_query.as_str(),
                "Filter workspaces and sessions globally. Esc/Enter to close.",
            )
        } else {
            (
                "Command",
                self.command.as_str(),
                "Examples: w3 select, s2 attach, n3 spawn, wt, wt feature/x, k1 kill, i1 interrupt, add /path, rename new-name",
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

fn highlight_style(is_focused: bool) -> Style {
    if is_focused {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::Black)
    }
}

fn session_style(status: SessionStatus) -> Style {
    match status {
        SessionStatus::Running => Style::default().fg(Color::Green),
        SessionStatus::WaitingInput => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        SessionStatus::Idle => Style::default().fg(Color::Blue),
        SessionStatus::Unknown => Style::default().fg(Color::DarkGray),
    }
}
