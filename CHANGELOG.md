# Changelog

This project follows the `MAJOR.MINOR.PATCH` policy documented in
`CONTRIBUTING.md`. Every release is published from a merged pull request.

## 0.5.0 (2026-09-21)

- Added a plain-language tooltip to every GUI control, shown after a short
  hover dwell and hidden when the pointer leaves.
- Added one shared description source so CLI and GUI help text cannot drift.
- Regrouped the CLI help by purpose and described every argument; `--help`,
  `-h`, and `-help` all succeed.

## 0.3.2 (2026-09-13)

- Added reproducible release artifact packaging and idempotent Forgejo/GitHub
  release synchronization tooling.
- Added concise installation guidance for release archives and packages.

## 0.3.1 (2026-09-13)

- Git chronology now includes every unique commit reachable from all local
  refs, including unmerged and tag-only histories, in deterministic
  topological order with sanitized subjects.

## 0.3.0 (2026-09-13)

- Added sanitized chronological Git history to generated archives.
- Added timestamped and CI-stable default archive names.
- Added ZIP, 7z, and multiple TAR compression selections with explicit
  backend capability reporting.
- Added ZIP AES-256 password input via file or stdin.

## 0.2.0 (2026-08-15)

- Improved redaction precision for paths, URLs, Markdown links, schema keys,
  environment references, and Kubernetes/secret-store symbolic references.
- Added regression coverage for preserving useful repository context while
  continuing to redact high-confidence credentials.
- Added deterministic repository/revision-based default archive names.

## 0.1.0 (2026-08-15)

- Initial deterministic Git repository sanitization release.
- Added filtering, redaction, archive generation, reports, checksums, and CLI
  controls for safe review bundles.
- Added path-safety, secret-container, binary, oversized-file, and recursion
  protections.
