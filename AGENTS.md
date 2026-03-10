# Repository Guidelines

## Project Structure & Module Organization
`src/main.rs` boots the TUI and owns terminal lifecycle, refresh timing, and `screen` attach/spawn actions. Data collection lives under `src/data/` (`collector.rs`, `helpers.rs`, `parsing.rs`), UI state and rendering live under `src/ui/` (`app.rs`, `render.rs`, `worker.rs`, `util.rs`), and shared types are in `src/types.rs`. Unit tests are colocated in `src/data/tests.rs` and `src/ui/tests.rs`. Build artifacts land in `target/`; do not commit them.

## Build, Test, and Development Commands
Use Cargo from the repo root:

- `cargo run --release` runs the dashboard locally.
- `cargo build --release` builds an optimized binary.
- `cargo test` runs all unit tests.
- `cargo fmt -- --check` verifies formatting.
- `cargo clippy --all-targets --all-features -- -D warnings` enforces a warning-free lint pass.

The app expects `screen` and `codex` to be available in `PATH` when you exercise attach/spawn flows.

## Coding Style & Naming Conventions
Follow `rustfmt` defaults: 4-space indentation, trailing commas where formatter adds them, and grouped `use` imports. Keep modules focused and small; prefer helper functions in `src/data/` or `src/ui/` over expanding `main.rs`. Use `snake_case` for functions, modules, and fields, `CamelCase` for types and enums, and descriptive names such as `spawn_screen_in_folder` or `recompute_visible`.

## Testing Guidelines
Add unit tests next to the module area they cover. Prefer focused tests that exercise parsing edge cases, session-state inference, sorting/filtering behavior, and command shortcuts. Name tests by behavior, for example `parse_session_summary_task_complete_clears_wait_flags`. Run `cargo test` before opening a PR; if you change rendering or keyboard flow, include a short note describing the scenario you verified manually.

## Commit & Pull Request Guidelines
This repository currently has no established Git history, so use clear imperative commit subjects such as `Add branch sort for session rows`. Keep commits scoped to one change. PRs should include a concise summary, any related issue or task link, test results, and a screenshot or terminal capture when TUI behavior changes.

## Security & Configuration Tips
Do not hardcode machine-specific paths beyond the `/home/ubuntu/...` conventions already assumed by the tool. Avoid committing session logs, local snapshots, or other transient data discovered from Codex/screen environments.
