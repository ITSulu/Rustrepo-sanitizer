# Contributing

Use TDD: add a failing test before implementing behavior. Before opening a
pull request, run `cargo fmt --check`, `cargo clippy --all-targets
--all-features -- -D warnings`, and `cargo test --all-features`.

Never add real credentials to fixtures or tests. Security-sensitive changes
require tests and a lightweight independent agent or code review before merge.
Add Criterion benchmarks for performance-sensitive changes.

Contributors retain copyright in their contributions. Contributions are
licensed under Apache-2.0 unless explicitly stated otherwise; no CLA or
copyright assignment is required.
