# codex-glances

Glances-style Rust TUI for coordinating many Codex + GNU Screen sessions across repos.

## Requirements

- Rust toolchain with Cargo
- GNU Screen available in `PATH`
- Codex CLI available in `PATH`

## Features

- Auto-discovers detached `screen` sessions from `screen -ls`
- Groups active screens and historical Codex threads into a cross-repo workspace index
- Persists known workspaces in `~/.config/codex-glances/workspaces.json`
- Maps sessions to Codex working folder, branch, thread id, and recent output
- Status inference:
  - `RUNNING`: thread has an active turn (`task_started` / `turn_started` without terminal turn event)
  - `WAITING`: thread emitted explicit wait events (`exec_approval_request`, `request_user_input`) or clearly asks for user action
  - `IDLE`: thread known but not active and not waiting
  - `UNKNOWN`: no Codex thread mapping found
- Browser/inspector-first TUI with adaptive layouts, including a dedicated two-pane Screens view
- Cross-repo views for all live screens, all workspaces, waiting work, running work, and recent activity
- Screen-first controls from the TUI: attach, spawn, kill, interrupt, rename, and pin workspaces
- Live refresh dashboard (3s)
- Global search overlay across workspace paths, branches, session ids, and recent timeline content
- Inspector tabs for summary, parsed timeline, action hints, worktree preview, and raw logs
- Action palette and confirmation/input overlays for the main operational flows

The app reads Codex state from `~/.codex/`.

## Getting Started

```bash
git clone https://github.com/HeroBrian389/codex-glances.git
cd codex-glances
```

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run --release
```

## Verify

```bash
cargo test
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Keybinds

- `q`: quit
- `Tab` / `Shift+Tab`: cycle panes. In `Screens` view this switches only between `Browser` and `Inspector`
- `j` / `k` or `Down` / `Up`: move inside the focused pane
- `PageUp` / `PageDown`: scroll the inspector
- `Enter`: attach the selected screen, move from workspace browser into context, or open the active inspector action
- `/`: open global search
- `a`: open the action palette
- `:`: open command mode
- `N`: spawn a new detached Codex screen in the selected workspace
- `W`: open the worktree spawn overlay for the selected screen branch
- `p`: pin or unpin the selected workspace
- `i`: send `Ctrl-C` to the selected screen
- `K`: open close confirmation for the selected screen
- `r`: rename the selected screen
- `A`: register a workspace path globally
- `[` / `]`: switch inspector tabs
- `1`: show all live screens globally
- `2`: show all workspaces
- `3`: show workspaces needing attention
- `4`: show running workspaces
- `5`: show recent workspace activity

## Command Mode

Examples:

- `w3` => select third visible workspace
- `s2` => select second visible screen in the context pane
- `n3` => spawn a new detached Codex screen in the third visible workspace
- `screens` => switch the browser to the global screen list
- `workspaces`, `attention`, `running`, `recent` => switch browser views
- `spawn` or `new` => spawn in the selected workspace
- `attach` => attach the selected screen
- `wt` => create or reuse a sibling worktree for the selected session branch, spawn there, and attach
- `wt feature/x` => same flow, but force a specific branch from the selected session's repo
- `interrupt` => send `Ctrl-C` to the selected screen
- `kill` => quit the selected screen
- `rename api-hotfix` => rename the selected or best session
- `add /path/to/repo` => register a workspace even if it has no active screens
- `pin` => pin or unpin the selected workspace
