# codex-glances

Glances-style Rust TUI for coordinating many Codex + GNU Screen sessions.

## Features

- Auto-discovers detached `screen` sessions from `screen -ls`
- Maps sessions to Codex working folder, branch, thread id, and recent output
- Status inference:
  - `RUNNING`: thread has an active turn (`task_started` / `turn_started` without terminal turn event)
  - `WAITING`: thread emitted explicit wait events (`exec_approval_request`, `request_user_input`) or clearly asks for user action
  - `IDLE`: thread known but not active and not waiting
  - `UNKNOWN`: no Codex thread mapping found
- Attention scoring for rows that likely need user intervention
- Live refresh dashboard (3s)
- Search/filter + sort keybinds
- Shortcut attach commands (`s1`, `s2`, ...)

## Build

```bash
source ~/.cargo/env
cd /home/ubuntu/codex-glances
cargo build --release
```

## Run

```bash
source ~/.cargo/env
cd /home/ubuntu/codex-glances
cargo run --release
```

## Keybinds

- `q`: quit
- `r`: refresh now
- `j` / `k` or `Down` / `Up`: move selection
- `Enter`: attach selected screen (`screen -d -r <id>`)
- `/`: search mode
- `:`: command mode
- `s`: command mode prefilled with `s` (type `sNN` + Enter)
- `n`: command mode prefilled with `n` (type `nNN` + Enter to spawn in that row's folder)
- `N`: spawn a new detached Codex screen in the selected row's folder
- `c`: clear search filter
- `1`: sort by attention/status
- `2`: sort by screen
- `3`: sort by branch
- `4`: sort by last update

## Command Mode

Examples:

- `s3` => attach third visible row
- `7014.s1` => attach explicit screen id
- `s1` => fastest jump flow
- `n3` => spawn a new detached Codex screen in the third visible row's folder
