# Contributing

Use TDD: add a failing test before implementing behavior. Before opening a
pull request, run `cargo fmt --check`, `cargo clippy --all-targets
--all-features -- -D warnings`, and `cargo test --all-features`.

Never add real credentials to fixtures or tests. Security-sensitive changes
require tests and a lightweight independent agent or code review before merge.
Add Criterion benchmarks for performance-sensitive changes.

## Versioning and releases

Rustrepo-sanitizer follows `MAJOR.MINOR.PATCH` versioning, with each numeric
component in the range 0–999. Major versions indicate incompatible changes or
a new stable generation; minor versions add meaningful backward-compatible
features; patches contain bug and security fixes. Every release is created
from a merged pull request, using an annotated `vMAJOR.MINOR.PATCH` tag.

Contributors retain copyright in their contributions. Contributions are
licensed under Apache-2.0 unless explicitly stated otherwise; no CLA or
copyright assignment is required.
