use crate::types::{SessionRow, SessionStatus, TimelineEventKind};
use ratatui::prelude::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub(super) fn pane_title(title: &str, focused: bool, subtitle: String) -> Line<'static> {
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

pub(super) fn line_kv(label: &str, value: String) -> Line<'static> {
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

pub(super) fn screen_row_style(screen: &SessionRow) -> Style {
    match screen.status {
        SessionStatus::WaitingInput => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        SessionStatus::Running => Style::default().fg(Color::Green),
        SessionStatus::Idle => Style::default().fg(Color::Blue),
        SessionStatus::Unknown => Style::default().fg(Color::DarkGray),
    }
}

pub(super) fn timeline_kind_style(kind: TimelineEventKind) -> Style {
    match kind {
        TimelineEventKind::User => Style::default().fg(Color::Cyan),
        TimelineEventKind::Agent => Style::default().fg(Color::Green),
        TimelineEventKind::Status => Style::default().fg(Color::Yellow),
        TimelineEventKind::Tool => Style::default().fg(Color::Magenta),
        TimelineEventKind::System => Style::default().fg(Color::Gray),
    }
}
