use super::style::{line_kv, pane_title, timeline_kind_style};
use crate::ui::{App, FocusPane, InspectorTab};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

impl App {
    pub(super) fn draw_inspector_pane(&self, frame: &mut Frame, area: Rect) {
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
                        super::super::util::tab_style(self.inspector_tab == tab),
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
            lines.push(line_kv(
                "Path",
                super::super::util::shorten_home(&workspace.path),
            ));
            lines.push(line_kv(
                "Activity",
                super::super::util::workspace_activity_badge(workspace),
            ));
            lines.push(line_kv("Branches", workspace.branch_label.clone()));
            lines.push(line_kv("Tags", workspace.tags.join(", ")));
            lines.push(line_kv("Follow-ups", workspace.follow_ups.to_string()));
            lines.push(line_kv("Workspace user", workspace.last_user.clone()));
            lines.push(line_kv("Workspace agent", workspace.last_agent.clone()));
            lines.push(line_kv(
                "Last update",
                super::super::util::age_string(workspace.last_update),
            ));
            lines.push(Line::raw(""));
        }

        if let Some(screen) = self.subject_screen() {
            lines.push(line_kv("Screen", screen.screen_name.clone()));
            lines.push(line_kv("Screen id", screen.screen_id.clone()));
            lines.push(line_kv("Thread id", screen.thread_id.clone()));
            lines.push(line_kv("Status", screen.status.as_str().to_string()));
            lines.push(line_kv("Reason", screen.status_reason.clone()));
            lines.push(line_kv("Branch", screen.branch.clone()));
            lines.push(line_kv(
                "Cwd",
                super::super::util::shorten_home(&screen.cwd),
            ));
            lines.push(line_kv(
                "Follow-ups",
                screen.scheduled_follow_ups.to_string(),
            ));
            lines.push(line_kv("Last event", screen.last_event.clone()));
            lines.push(line_kv("Last user", screen.last_user.clone()));
            lines.push(line_kv("Last agent", screen.last_agent.clone()));
            lines.push(line_kv(
                "Updated",
                super::super::util::age_string(screen.last_update),
            ));
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
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:>6}", super::super::util::age_string(event.timestamp)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:>5}", event.kind.as_str()),
                    timeline_kind_style(event.kind).add_modifier(Modifier::BOLD),
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
            line_kv("Source cwd", super::super::util::shorten_home(&screen.cwd)),
            line_kv(
                "Sibling preview",
                super::super::util::shorten_home(&super::super::util::worktree_preview_path(
                    &screen.cwd,
                    &screen.branch,
                )),
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
}
