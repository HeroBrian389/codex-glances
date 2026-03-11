mod browser;
mod inspector;
mod overlay;
mod style;

use crate::ui::{App, BrowserMode, FocusPane};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

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
                    super::util::browser_mode_style(self.browser_mode == mode),
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

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("Focused: {title}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  Tab cycles panes"),
            ])),
            panes[0],
        );

        match self.focus {
            FocusPane::Browser => self.draw_browser_pane(frame, panes[1]),
            FocusPane::Context => self.draw_context_pane(frame, panes[1]),
            FocusPane::Inspector => self.draw_inspector_pane(frame, panes[1]),
        }
    }
}
