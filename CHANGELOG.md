# Changelog

This project follows the `MAJOR.MINOR.PATCH` policy documented in
`CONTRIBUTING.md`. Every release is published from a merged pull request.

## 0.2.0

- Improved redaction precision for paths, URLs, Markdown links, schema keys,
  environment references, and Kubernetes/secret-store symbolic references.
- Added regression coverage for preserving useful repository context while
  continuing to redact high-confidence credentials.
- Added deterministic repository/revision-based default archive names.

## 0.1.0

- Initial deterministic Git repository sanitization release.
- Added filtering, redaction, archive generation, reports, checksums, and CLI
  controls for safe review bundles.
- Added path-safety, secret-container, binary, oversized-file, and recursion
  protections.
