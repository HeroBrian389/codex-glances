use super::util::{
    age_string, browser_mode_style, centered_rect, highlight_style, shorten_home, tab_style,
    truncate_chars, workspace_activity_badge, worktree_preview_path,
};
use super::{App, BrowserMode, FocusPane, InputMode, InspectorTab, OverlayState};
use crate::types::{SessionRow, SessionStatus, TimelineEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Alignment, Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(2),
            ])
            .split(frame.area());

        self.draw_header(frame, root[0]);

        if root[1].width >= 150 {
            self.draw_wide_body(frame, root[1]);
        } else if root[1].width >= 105 {
            self.draw_medium_body(frame, root[1]);
        } else {
            self.draw_compact_body(frame, root[1]);
        }

        self.draw_footer(frame, root[2]);
        self.draw_overlay(frame);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let workspace_total = self.data.workspaces.len();
        let screen_total = self
            .data
            .workspaces
            .iter()
            .map(|workspace| workspace.session_count)
            .sum::<usize>();
        let waiting_total = self
            .data
            .workspaces
            .iter()
            .map(|workspace| workspace.waiting_sessions)
            .sum::<usize>();
        let running_total = self
            .data
            .workspaces
            .iter()
            .map(|workspace| workspace.running_sessions)
            .sum::<usize>();

        let tabs = [
            BrowserMode::Workspaces,
            BrowserMode::Attention,
            BrowserMode::Running,
            BrowserMode::Recent,
            BrowserMode::Screens,
        ]
        .into_iter()
        .flat_map(|mode| {
            [
                Span::styled(
                    format!(" {} ", mode.label()),
                    browser_mode_style(self.browser_mode == mode),
                ),
                Span::raw(" "),
            ]
        })
        .collect::<Vec<_>>();

        let line = Line::from(
            vec![
                Span::styled(
                    " Codex Glances ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!(
                        "{} workspaces | {} screens | {} waiting | {} running",
                        workspace_total, screen_total, waiting_total, running_total
                    ),
                    Style::default().fg(Color::White),
                ),
                Span::raw("  "),
            ]
            .into_iter()
            .chain(tabs)
            .collect::<Vec<_>>(),
        );

        frame.render_widget(
            Paragraph::new(line)
                .block(Block::default().borders(Borders::ALL).title("Overview"))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_wide_body(&mut self, frame: &mut Frame, area: Rect) {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(31),
                Constraint::Percentage(35),
            ])
            .split(area);
        self.draw_browser_pane(frame, panes[0]);
        self.draw_context_pane(frame, panes[1]);
        self.draw_inspector_pane(frame, panes[2]);
    }

    fn draw_medium_body(&mut self, frame: &mut Frame, area: Rect) {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(41), Constraint::Percentage(59)])
            .split(area);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(43), Constraint::Percentage(57)])
            .split(panes[1]);

        self.draw_browser_pane(frame, panes[0]);
        self.draw_context_pane(frame, right[0]);
        self.draw_inspector_pane(frame, right[1]);
    }

    fn draw_compact_body(&mut self, frame: &mut Frame, area: Rect) {
        let title = match self.focus {
            FocusPane::Browser => "Browser",
            FocusPane::Context => "Context",
            FocusPane::Inspector => "Inspector",
        };
        let panes = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(5)])
            .split(area);

        let breadcrumb = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("Focused: {title}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Tab cycles panes"),
        ]));
        frame.render_widget(breadcrumb, panes[0]);

        match self.focus {
            FocusPane::Browser => self.draw_browser_pane(frame, panes[1]),
            FocusPane::Context => self.draw_context_pane(frame, panes[1]),
            FocusPane::Inspector => self.draw_inspector_pane(frame, panes[1]),
        }
    }

    fn draw_browser_pane(&mut self, frame: &mut Frame, area: Rect) {
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
                    Cell::from(truncate_chars(&workspace.display_name, 20)),
                    Cell::from(workspace_activity_badge(workspace)),
                    Cell::from(truncate_chars(&workspace.branch_label, 16)),
                    Cell::from(age_string(workspace.last_update)),
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
        .row_highlight_style(highlight_style(self.focus == FocusPane::Browser));

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
                        Cell::from(truncate_chars(&workspace.display_name, 16)),
                        Cell::from(truncate_chars(&screen.screen_name, 18)),
                        Cell::from(screen.status.as_str()),
                        Cell::from(age_string(screen.last_update)),
                        Cell::from(truncate_chars(&screen.status_reason, 24)),
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
        .row_highlight_style(highlight_style(self.focus == FocusPane::Browser));

        frame.render_stateful_widget(table, area, &mut self.browser_table_state);
    }

    fn draw_context_pane(&mut self, frame: &mut Frame, area: Rect) {
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
                        Cell::from(truncate_chars(&screen.screen_name, 16)),
                        Cell::from(truncate_chars(&screen.branch, 16)),
                        Cell::from(screen.status.as_str()),
                        Cell::from(age_string(screen.last_update)),
                        Cell::from(truncate_chars(&screen.status_reason, 24)),
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
        .row_highlight_style(highlight_style(self.focus == FocusPane::Context));

        frame.render_stateful_widget(table, area, &mut self.context_table_state);
    }

    fn draw_inspector_pane(&self, frame: &mut Frame, area: Rect) {
        let panes = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(3)])
            .split(area);

        let tab_line = Line::from(
            [
                InspectorTab::Summary,
                InspectorTab::Timeline,
                InspectorTab::Actions,
                InspectorTab::Worktree,
                InspectorTab::Logs,
            ]
            .into_iter()
            .flat_map(|tab| {
                [
                    Span::styled(
                        format!(" {} ", tab.label()),
                        tab_style(self.inspector_tab == tab),
                    ),
                    Span::raw(" "),
                ]
            })
            .collect::<Vec<_>>(),
        );

        frame.render_widget(
            Paragraph::new(tab_line).block(
                Block::default().borders(Borders::ALL).title(pane_title(
                    "Inspector",
                    self.focus == FocusPane::Inspector,
                    self.subject_screen()
                        .map(|screen| screen.screen_name.clone())
                        .unwrap_or_else(|| "selection".to_string()),
                )),
            ),
            panes[0],
        );

        let content = match self.inspector_tab {
            InspectorTab::Summary => self.summary_text(),
            InspectorTab::Timeline => self.timeline_text(),
            InspectorTab::Actions => self.actions_text(),
            InspectorTab::Worktree => self.worktree_text(),
            InspectorTab::Logs => self.logs_text(),
        };

        frame.render_widget(
            Paragraph::new(content)
                .scroll((self.inspector_scroll, 0))
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL)),
            panes[1],
        );
    }

    fn summary_text(&self) -> Text<'static> {
        let mut lines = Vec::new();

        if let Some(workspace) = self.subject_workspace() {
            lines.push(line_kv("Workspace", workspace.display_name.clone()));
            lines.push(line_kv("Path", shorten_home(&workspace.path)));
            lines.push(line_kv("Activity", workspace_activity_badge(workspace)));
            lines.push(line_kv("Branches", workspace.branch_label.clone()));
            lines.push(line_kv("Tags", workspace.tags.join(", ")));
            lines.push(line_kv("Follow-ups", workspace.follow_ups.to_string()));
            lines.push(line_kv("Workspace user", workspace.last_user.clone()));
            lines.push(line_kv("Workspace agent", workspace.last_agent.clone()));
            lines.push(line_kv("Last update", age_string(workspace.last_update)));
            lines.push(Line::raw(""));
        }

        if let Some(screen) = self.subject_screen() {
            lines.push(line_kv("Screen", screen.screen_name.clone()));
            lines.push(line_kv("Screen id", screen.screen_id.clone()));
            lines.push(line_kv("Thread id", screen.thread_id.clone()));
            lines.push(line_kv("Status", screen.status.as_str().to_string()));
            lines.push(line_kv("Reason", screen.status_reason.clone()));
            lines.push(line_kv("Branch", screen.branch.clone()));
            lines.push(line_kv("Cwd", shorten_home(&screen.cwd)));
            lines.push(line_kv(
                "Follow-ups",
                screen.scheduled_follow_ups.to_string(),
            ));
            lines.push(line_kv("Last event", screen.last_event.clone()));
            lines.push(line_kv("Last user", screen.last_user.clone()));
            lines.push(line_kv("Last agent", screen.last_agent.clone()));
            lines.push(line_kv("Updated", age_string(screen.last_update)));
        } else {
            lines.push(Line::raw("No screen selected"));
        }

        Text::from(lines)
    }

    fn timeline_text(&self) -> Text<'static> {
        let Some(screen) = self.subject_screen() else {
            return Text::from("No screen selected");
        };

        if screen.timeline.is_empty() {
            return Text::from("No parsed timeline events yet");
        }

        let mut lines = Vec::new();
        for event in screen.timeline.iter().rev() {
            let kind_style = timeline_kind_style(event.kind);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:>6}", age_string(event.timestamp)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:>5}", event.kind.as_str()),
                    kind_style.add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    event.title.clone(),
                    if event.emphasis {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ]));

            if !event.detail.is_empty() && event.detail != "-" {
                lines.push(Line::from(vec![
                    Span::raw("       "),
                    Span::styled(event.detail.clone(), Style::default().fg(Color::Gray)),
                ]));
            }
            lines.push(Line::raw(""));
        }

        Text::from(lines)
    }

    fn actions_text(&self) -> Text<'static> {
        let mut lines = vec![
            Line::raw("Core"),
            Line::raw("  Enter attaches the selected screen"),
            Line::raw("  Shift+N spawns a new screen in the selected workspace"),
            Line::raw("  Shift+W opens the worktree spawn dialog"),
            Line::raw("  a opens the action palette"),
            Line::raw("  / opens global search"),
            Line::raw(""),
            Line::raw("Navigation"),
            Line::raw("  Tab or Shift+Tab cycles Browser, Context, Inspector"),
            Line::raw("  1..5 switch browser views, with 5 showing all screens"),
            Line::raw("  [ and ] switch inspector tabs"),
            Line::raw("  j/k or arrows move the focused pane"),
            Line::raw(""),
            Line::raw("Screen actions"),
            Line::raw("  i sends Ctrl-C to the selected screen"),
            Line::raw("  Shift+K opens close confirmation"),
            Line::raw("  r renames the selected screen"),
            Line::raw("  p pins or unpins the selected workspace"),
            Line::raw("  Shift+A registers a new workspace path"),
            Line::raw(""),
            Line::raw("Command line"),
            Line::raw("  :screens, :workspaces, :attention, :running, :recent"),
            Line::raw("  :spawn, :attach, :wt [branch], :pin, :interrupt, :kill"),
            Line::raw("  :rename <name>, :add <path>, :w2, :s1, :n3"),
        ];

        if let Some(screen) = self.subject_screen() {
            lines.push(Line::raw(""));
            lines.push(Line::raw("Selected"));
            lines.push(Line::raw(format!(
                "  {} on {}",
                screen.screen_name, screen.branch
            )));
            lines.push(Line::raw(format!("  {}", screen.status_reason)));
        }

        Text::from(lines)
    }

    fn worktree_text(&self) -> Text<'static> {
        let Some(screen) = self.subject_screen() else {
            return Text::from("Select a screen to prepare a worktree spawn");
        };

        Text::from(vec![
            line_kv("Source screen", screen.screen_name.clone()),
            line_kv("Branch", screen.branch.clone()),
            line_kv("Source cwd", shorten_home(&screen.cwd)),
            line_kv(
                "Sibling preview",
                shorten_home(&worktree_preview_path(&screen.cwd, &screen.branch)),
            ),
            Line::raw(""),
            Line::raw("Shift+W opens an editable worktree dialog."),
            Line::raw("The new screen attaches immediately after creation."),
        ])
    }

    fn logs_text(&self) -> Text<'static> {
        let Some(screen) = self.subject_screen() else {
            return Text::from("No screen selected");
        };

        if screen.raw_log.is_empty() {
            return Text::from("No raw log lines captured");
        }

        Text::from(
            screen
                .raw_log
                .iter()
                .rev()
                .cloned()
                .map(Line::raw)
                .collect::<Vec<_>>(),
        )
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
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

    fn draw_overlay(&self, frame: &mut Frame) {
        if self.mode == InputMode::Command {
            let area = centered_rect(72, 18, frame.area());
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

    fn draw_search_overlay(&self, frame: &mut Frame, search: &super::SearchOverlay) {
        let area = centered_rect(78, 72, frame.area());
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

    fn draw_palette_overlay(&self, frame: &mut Frame, palette: &super::ActionPaletteOverlay) {
        let area = centered_rect(74, 64, frame.area());
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

    fn draw_confirm_overlay(&self, frame: &mut Frame, confirm: &super::ConfirmOverlay) {
        let area = centered_rect(50, 24, frame.area());
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

    fn draw_worktree_overlay(&self, frame: &mut Frame, worktree: &super::WorktreeOverlay) {
        let area = centered_rect(72, 32, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                line_kv("Source", shorten_home(&worktree.source_cwd)),
                line_kv("Branch", worktree.branch_input.clone()),
                line_kv("Preview", shorten_home(&worktree.target_preview)),
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

    fn draw_input_overlay(&self, frame: &mut Frame, input: &super::InputOverlay) {
        let area = centered_rect(68, 26, frame.area());
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

fn pane_title(title: &str, focused: bool, subtitle: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {} ", title),
            if focused {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ),
        Span::raw(" "),
        Span::styled(subtitle, Style::default().fg(Color::White)),
    ])
}

fn line_kv(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value),
    ])
}

fn screen_row_style(screen: &SessionRow) -> Style {
    match screen.status {
        SessionStatus::WaitingInput => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        SessionStatus::Running => Style::default().fg(Color::Green),
        SessionStatus::Idle => Style::default().fg(Color::Blue),
        SessionStatus::Unknown => Style::default().fg(Color::DarkGray),
    }
}

fn timeline_kind_style(kind: TimelineEventKind) -> Style {
    match kind {
        TimelineEventKind::User => Style::default().fg(Color::Cyan),
        TimelineEventKind::Agent => Style::default().fg(Color::Green),
        TimelineEventKind::Status => Style::default().fg(Color::Yellow),
        TimelineEventKind::Tool => Style::default().fg(Color::Magenta),
        TimelineEventKind::System => Style::default().fg(Color::Gray),
    }
}
