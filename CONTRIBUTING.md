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

## Release engineering

Build artifacts with `release/build-artifacts.sh VERSION OUTPUT_DIR` using a
Forgejo runner or Garuda workstation and the `RELEASE_PLATFORM`,
`RELEASE_ARCH`, and optional `RELEASE_FORMATS` environment variables. The
script checks the binary version, uses consistent names, and writes
`SHA256SUMS`. `release/publish-release.sh` publishes the same directory
idempotently to Forgejo and GitHub; provide tokens only through protected
credential mechanisms. Forgejo tags, CI, review gates, builds, and releases
remain authoritative. GitHub is a downstream source and release mirror only.

The 0.3.2 policy covers glibc and musl Linux archives, Debian/Ubuntu,
Fedora/RHEL, Arch, Alpine, Nix, Flatpak, and a Garuda-built Windows x86_64
archive validated with Wine. macOS source/Cargo installation is supported;
prebuilt macOS binaries are deferred until a native ITSulu macOS environment
exists.
