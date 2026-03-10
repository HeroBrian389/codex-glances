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
- Workspace-first two-pane dashboard: global workspace list plus per-workspace session list
- Cross-repo views for all workspaces, waiting work, running work, and recent activity
- Screen controls from the TUI: attach, spawn, kill, interrupt, rename, and pin workspaces
- Live refresh dashboard (3s)
- Global search across workspace paths, branches, session ids, and last messages

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
- `r`: refresh now
- `j` / `k` or `Down` / `Up`: move selection in the focused pane
- `Tab`, `Left`, `Right`: switch between workspace and session panes
- `Enter`: attach the selected session, attach the best session in the selected workspace, or spawn in an inactive workspace
- `/`: search mode
- `:`: command mode
- `N`: spawn a new detached Codex screen in the selected workspace
- `W`: create or reuse a sibling git worktree for the selected session branch, spawn a detached Codex screen there, and immediately attach
- `p`: pin or unpin the selected workspace
- `x`: kill the selected or best session
- `i`: send `Ctrl-C` to the selected or best session
- `c`: clear search filter
- `1`: show all known workspaces
- `2`: show workspaces waiting on input
- `3`: show workspaces with running sessions
- `4`: show recent workspace activity

## Command Mode

Examples:

- `w3` => select third visible workspace
- `s2` => attach second session in the selected workspace
- `n3` => spawn a new detached Codex screen in the third visible workspace
- `wt` => create or reuse a sibling worktree for the selected session branch, spawn there, and attach
- `wt feature/x` => same flow, but force a specific branch from the selected session's repo
- `k1` => kill first session in the selected workspace
- `i1` => interrupt first session in the selected workspace
- `rename api-hotfix` => rename the selected or best session
- `add /path/to/repo` => register a workspace even if it has no active screens
- `7014.s1` => attach explicit screen id
