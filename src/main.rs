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
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use std::time::{Duration, Instant};

use crate::data::{DataCollector, add_workspace_registry_entry, toggle_workspace_pinned};
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
                AppAction::SpawnWorkspace(cwd) => {
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
                AppAction::SpawnWorktree { source_cwd, branch } => {
                    match create_or_reuse_worktree_and_spawn(&source_cwd, &branch) {
                        Ok(result) => {
                            let _ = add_workspace_registry_entry(Path::new(&result.worktree_path));
                            suspend_terminal(terminal)?;
                            let status = Command::new("screen")
                                .args(["-d", "-r", &result.screen_name])
                                .status()
                                .with_context(|| {
                                    format!(
                                        "failed to execute screen attach for {}",
                                        result.screen_name
                                    )
                                });
                            resume_terminal(terminal)?;

                            match status {
                                Ok(exit_status) if exit_status.success() => {
                                    let verb = if result.reused_worktree {
                                        "reused"
                                    } else {
                                        "created"
                                    };
                                    app.set_info(format!(
                                        "{verb} worktree {} and opened {}",
                                        result.worktree_path, result.screen_name
                                    ));
                                }
                                Ok(exit_status) => {
                                    app.set_error(format!(
                                        "screen returned status {} while attaching {}",
                                        exit_status, result.screen_name
                                    ));
                                }
                                Err(err) => {
                                    app.set_error(format!(
                                        "attach failed for {}: {err:#}",
                                        result.screen_name
                                    ));
                                }
                            }
                        }
                        Err(err) => {
                            app.set_error(format!(
                                "worktree spawn failed for {source_cwd} on {branch}: {err:#}"
                            ));
                        }
                    }

                    app.refresh();
                    app.wait_for_refresh(Duration::from_millis(750));
                    last_refresh = Instant::now();
                }
                AppAction::AddWorkspace(path) => {
                    match add_workspace_registry_entry(Path::new(&path)) {
                        Ok(normalized) => {
                            app.set_info(format!("registered workspace {}", normalized.display()));
                        }
                        Err(err) => {
                            app.set_error(format!("workspace add failed for {path}: {err:#}"));
                        }
                    }

                    app.refresh();
                    app.wait_for_refresh(Duration::from_millis(750));
                    last_refresh = Instant::now();
                }
                AppAction::TogglePinWorkspace(path) => {
                    match toggle_workspace_pinned(&path) {
                        Ok(true) => app.set_info(format!("pinned {path}")),
                        Ok(false) => app.set_info(format!("unpinned {path}")),
                        Err(err) => app.set_error(format!("pin toggle failed for {path}: {err:#}")),
                    }

                    app.refresh();
                    app.wait_for_refresh(Duration::from_millis(750));
                    last_refresh = Instant::now();
                }
                AppAction::KillScreen(screen_id) => {
                    match screen_quit(&screen_id) {
                        Ok(()) => app.set_info(format!("closed {screen_id}")),
                        Err(err) => app.set_error(format!("close failed for {screen_id}: {err:#}")),
                    }

                    app.refresh();
                    app.wait_for_refresh(Duration::from_millis(750));
                    last_refresh = Instant::now();
                }
                AppAction::InterruptScreen(screen_id) => {
                    match screen_interrupt(&screen_id) {
                        Ok(()) => app.set_info(format!("sent interrupt to {screen_id}")),
                        Err(err) => {
                            app.set_error(format!("interrupt failed for {screen_id}: {err:#}"));
                        }
                    }

                    app.refresh();
                    app.wait_for_refresh(Duration::from_millis(750));
                    last_refresh = Instant::now();
                }
                AppAction::RenameScreen {
                    screen_id,
                    new_name,
                } => {
                    match rename_screen_session(&screen_id, &new_name) {
                        Ok(()) => app.set_info(format!("renamed {screen_id} to {new_name}")),
                        Err(err) => app.set_error(format!(
                            "rename failed for {screen_id} -> {new_name}: {err:#}"
                        )),
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

fn screen_quit(screen_id: &str) -> Result<()> {
    let status = Command::new("screen")
        .args(["-S", screen_id, "-X", "quit"])
        .status()
        .with_context(|| format!("failed to execute screen quit for {screen_id}"))?;

    if !status.success() {
        anyhow::bail!("screen returned status {status}");
    }

    Ok(())
}

fn screen_interrupt(screen_id: &str) -> Result<()> {
    let status = Command::new("screen")
        .args(["-S", screen_id, "-X", "stuff", "\u{3}"])
        .status()
        .with_context(|| format!("failed to execute screen interrupt for {screen_id}"))?;

    if !status.success() {
        anyhow::bail!("screen returned status {status}");
    }

    Ok(())
}

fn rename_screen_session(screen_id: &str, new_name: &str) -> Result<()> {
    let status = Command::new("screen")
        .args(["-S", screen_id, "-X", "sessionname", new_name])
        .status()
        .with_context(|| format!("failed to execute screen rename for {screen_id}"))?;

    if !status.success() {
        anyhow::bail!("screen returned status {status}");
    }

    Ok(())
}

struct WorktreeSpawnResult {
    worktree_path: String,
    screen_name: String,
    reused_worktree: bool,
}

fn create_or_reuse_worktree_and_spawn(
    source_cwd: &str,
    branch: &str,
) -> Result<WorktreeSpawnResult> {
    if branch.trim().is_empty() || branch == "-" {
        anyhow::bail!("branch is not available");
    }

    let worktree_info = resolve_worktree_target(source_cwd, branch)?;
    if !worktree_info.reused_worktree {
        let status = Command::new("git")
            .args([
                "-C",
                source_cwd,
                "worktree",
                "add",
                worktree_info.target_path.to_string_lossy().as_ref(),
                branch,
            ])
            .status()
            .with_context(|| {
                format!(
                    "failed to execute git worktree add for {} at {}",
                    branch,
                    worktree_info.target_path.display()
                )
            })?;

        if !status.success() {
            anyhow::bail!("git worktree add returned status {status}");
        }
    }

    let worktree_path = worktree_info.target_path.to_string_lossy().into_owned();
    let screen_name = build_screen_name(&worktree_path);
    let status = Command::new("screen")
        .current_dir(&worktree_info.target_path)
        .args(["-dmS", &screen_name, "codex"])
        .status()
        .with_context(|| format!("failed to execute detached screen in {}", worktree_path))?;

    if !status.success() {
        anyhow::bail!("screen returned status {status}");
    }

    Ok(WorktreeSpawnResult {
        worktree_path,
        screen_name,
        reused_worktree: worktree_info.reused_worktree,
    })
}

struct WorktreeTarget {
    target_path: PathBuf,
    reused_worktree: bool,
}

fn resolve_worktree_target(source_cwd: &str, branch: &str) -> Result<WorktreeTarget> {
    let output = Command::new("git")
        .args(["-C", source_cwd, "worktree", "list", "--porcelain"])
        .output()
        .with_context(|| format!("failed to list git worktrees for {source_cwd}"))?;
    if !output.status.success() {
        anyhow::bail!("git worktree list returned status {}", output.status);
    }

    let entries = parse_worktree_list(&String::from_utf8_lossy(&output.stdout));
    let main_path = entries
        .first()
        .map(|entry| entry.path.clone())
        .ok_or_else(|| anyhow::anyhow!("no git worktree entries found"))?;
    let target_path = build_sibling_worktree_path(&main_path, branch);

    if let Some(existing) = entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some(branch))
    {
        if existing.path == target_path {
            return Ok(WorktreeTarget {
                target_path,
                reused_worktree: true,
            });
        }
        anyhow::bail!(
            "branch {branch} already has a worktree at {}",
            existing.path.display()
        );
    }

    if target_path.exists() {
        anyhow::bail!(
            "target worktree path already exists at {}",
            target_path.display()
        );
    }

    Ok(WorktreeTarget {
        target_path,
        reused_worktree: false,
    })
}

#[derive(Debug)]
struct ParsedWorktreeEntry {
    path: PathBuf,
    branch: Option<String>,
}

fn parse_worktree_list(raw: &str) -> Vec<ParsedWorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;

    for line in raw.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(path) = current_path.take() {
                entries.push(ParsedWorktreeEntry {
                    path,
                    branch: current_branch.take(),
                });
            }
            current_path = Some(PathBuf::from(path));
            current_branch = None;
        } else if let Some(branch_ref) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(branch_ref.to_string());
        }
    }

    if let Some(path) = current_path {
        entries.push(ParsedWorktreeEntry {
            path,
            branch: current_branch,
        });
    }

    entries
}

fn build_sibling_worktree_path(main_path: &Path, branch: &str) -> PathBuf {
    let parent = main_path.parent().unwrap_or_else(|| Path::new("."));
    let base_name = main_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("workspace");
    let branch_suffix = branch
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    parent.join(format!("{base_name}--{branch_suffix}"))
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

#[cfg(test)]
mod tests {
    use super::{build_sibling_worktree_path, parse_worktree_list};
    use std::path::Path;

    #[test]
    fn parse_worktree_list_reads_main_and_branch_entries() {
        let raw = "\
worktree /tmp/repo
HEAD deadbeef
branch refs/heads/main

worktree /tmp/repo--feature-a
HEAD cafefood
branch refs/heads/feature/a
";
        let entries = parse_worktree_list(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, Path::new("/tmp/repo"));
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert_eq!(entries[1].path, Path::new("/tmp/repo--feature-a"));
        assert_eq!(entries[1].branch.as_deref(), Some("feature/a"));
    }

    #[test]
    fn build_sibling_worktree_path_sanitizes_branch_name() {
        let path = build_sibling_worktree_path(Path::new("/tmp/repo"), "feature/api-v2");
        assert_eq!(path, Path::new("/tmp/repo--feature-api-v2"));
    }
}
