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
Follow `rustfmt` defaults: 4-space indentation, trailing commas where formatter adds them, and grouped `use` imports. Keep modules focused and small; prefer helper functions in `src/data/` or `src/ui/` over expanding `main.rs`. Target file sizes of roughly 200-400 lines. If a file grows beyond 600 lines, treat that as a refactor trigger and split it into smaller modules before adding more behavior. Use `snake_case` for functions, modules, and fields, `CamelCase` for types and enums, and descriptive names such as `spawn_screen_in_folder` or `recompute_visible`.

## Rust Design & Error Handling
Prefer small, single-purpose functions and structs with narrow, explicit inputs and outputs. Compose larger behavior from helpers instead of adding multi-mode functions, boolean switches, or broad option bags.

Make invariants visible in types. Prefer `Option`, enums, and small newtypes over sentinel strings such as `"-"` or empty `String` values in internal logic. Convert missing or unknown state into display placeholders only at the UI boundary.

Do not add silent fallbacks or swallow errors in parsing and IO code. Low-level helpers should return `Result` when failure is meaningful, and callers should decide explicitly whether to surface the error, cache an unknown state, or degrade for display.

Prefer explicit dependency injection for shared components. When code depends on paths, environment-derived locations, or runtime settings, pass them in via constructor arguments or small config structs instead of hard-coding ambient process state deep in the implementation.

## Testing Guidelines
Add unit tests next to the module area they cover. Prefer focused tests that exercise parsing edge cases, session-state inference, sorting/filtering behavior, and command shortcuts. Name tests by behavior, for example `parse_session_summary_task_complete_clears_wait_flags`. Run `cargo test` before opening a PR; if you change rendering or keyboard flow, include a short note describing the scenario you verified manually.

## Commit & Pull Request Guidelines
This repository currently has no established Git history, so use clear imperative commit subjects such as `Add branch sort for session rows`. Keep commits scoped to one change. PRs should include a concise summary, any related issue or task link, test results, and a screenshot or terminal capture when TUI behavior changes.

## Security & Configuration Tips
Prefer `HOME`-relative or environment-driven paths over machine-specific absolute paths. Avoid committing session logs, local snapshots, shell dumps, `.env` files, or other transient data discovered from Codex/screen environments.
