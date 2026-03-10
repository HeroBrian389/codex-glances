mod data;
mod types;
mod ui;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;
use std::time::{Duration, Instant};

use crate::data::DataCollector;
use crate::ui::{App, AppAction, InputMode};

fn main() -> Result<()> {
    let collector = DataCollector::new()?;
    let mut app = App::new(collector);
    app.refresh();
    app.wait_for_refresh(Duration::from_millis(750));

    let mut terminal = setup_terminal()?;
    let run_result = run_app(&mut terminal, &mut app);
    let restore_result = restore_terminal(&mut terminal);

    if let Err(err) = restore_result {
        eprintln!("terminal restore failed: {err:#}");
    }

    run_result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let mut last_refresh = Instant::now();

    loop {
        app.poll_refresh();
        terminal.draw(|frame| app.draw(frame))?;

        let refresh_interval = app.refresh_interval();
        let remaining = refresh_interval
            .checked_sub(last_refresh.elapsed())
            .unwrap_or(Duration::from_millis(0));
        let poll_timeout = if app.is_refresh_in_flight() {
            remaining.min(Duration::from_millis(100))
        } else {
            remaining
        };

        if event::poll(poll_timeout)?
            && let Event::Key(key) = event::read()?
        {
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }

            let action = match app.mode() {
                InputMode::Normal => app.handle_normal_key(key.code, key.modifiers),
                InputMode::Search => {
                    app.handle_search_key(key.code);
                    AppAction::None
                }
                InputMode::Command => app.handle_command_key(key.code),
            };

            match action {
                AppAction::None => {}
                AppAction::Quit => return Ok(()),
                AppAction::Attach(screen_id) => {
                    suspend_terminal(terminal)?;
                    let status = Command::new("screen")
                        .args(["-d", "-r", &screen_id])
                        .status()
                        .with_context(|| {
                            format!("failed to execute screen attach for {screen_id}")
                        });
                    resume_terminal(terminal)?;

                    match status {
                        Ok(exit_status) if exit_status.success() => {
                            app.set_info(format!("detached from {screen_id}"));
                        }
                        Ok(exit_status) => {
                            app.set_error(format!(
                                "screen returned status {} while attaching {}",
                                exit_status, screen_id
                            ));
                        }
                        Err(err) => {
                            app.set_error(format!("attach failed for {}: {err:#}", screen_id));
                        }
                    }

                    app.refresh();
                    app.wait_for_refresh(Duration::from_millis(750));
                    last_refresh = Instant::now();
                }
                AppAction::Spawn(cwd) => {
                    match spawn_screen_in_folder(&cwd) {
                        Ok(screen_name) => {
                            app.set_info(format!("spawned {screen_name} in {cwd}"));
                        }
                        Err(err) => {
                            app.set_error(format!("spawn failed for {cwd}: {err:#}"));
                        }
                    }

                    app.refresh();
                    app.wait_for_refresh(Duration::from_millis(750));
                    last_refresh = Instant::now();
                }
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            app.refresh();
            last_refresh = Instant::now();
        }
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).context("failed to build terminal")?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to show cursor")?;
    Ok(())
}

fn suspend_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode for attach")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen for attach")?;
    terminal
        .show_cursor()
        .context("failed to show cursor for attach")?;
    Ok(())
}

fn resume_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    enable_raw_mode().context("failed to re-enable raw mode")?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)
        .context("failed to re-enter alternate screen")?;
    terminal.clear().context("failed to clear terminal")?;
    Ok(())
}

fn spawn_screen_in_folder(cwd: &str) -> Result<String> {
    let path = Path::new(cwd);
    if !path.is_dir() {
        anyhow::bail!("folder is not available");
    }

    let screen_name = build_screen_name(cwd);
    let status = Command::new("screen")
        .current_dir(path)
        .args(["-dmS", &screen_name, "codex"])
        .status()
        .with_context(|| format!("failed to execute detached screen in {cwd}"))?;

    if !status.success() {
        anyhow::bail!("screen returned status {status}");
    }

    Ok(screen_name)
}

fn build_screen_name(cwd: &str) -> String {
    let base = Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("session");
    let sanitized = base
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    format!("codex-{sanitized}-{stamp}")
}
