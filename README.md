# Rustrepo-sanitizer

[**Rustrepo-sanitizer**](https://git.itsulu.com/itsulu/Rustrepo-sanitizer) creates
a sanitized and compressed view of any Git repository for safe AI review. The intent
is to to automatically remove private information from a private repository for use
with any cloud or closed weight AI model. 

This uses Git's tracked file view by default, streams files rather than
loading a repository into memory, and operates on standard Git repositories
without assuming a project specific directory structure.

Forgejo is authoritative repositor and is mirrored by push to GitHub:
https://github.com/ITSulu/Rustrepo-sanitizer

## Install and run

```bash
cargo install --path .
itsulu-repo-sanitizer sanitize .
itsulu-repo-sanitizer sanitize /path/to/repository --output /tmp/review.tar.zst
```

The archive contains sanitized source files plus `SANITIZATION-REPORT.md`,
`REPOSITORY-INVENTORY.md`, `SECRET-AUDIT.md`, `SHA256SUMS`, and `manifest.json`.
Reports contain counts, paths, reasons, and checksums—not secret values.

## Usage

```text
itsulu-repo-sanitizer sanitize [REPOSITORY] [OPTIONS]

  --output PATH                 Archive destination
  --format tar.gz|tar.zst       Archive compression format
  --report markdown|json|none   Report format
  --include-untracked           Consider untracked regular files too
  --max-file-size SIZE          Per-file size ceiling (for example, 2MiB)
  --exclude PATTERN             Exclude a glob (repeatable)
  --include PATTERN             Include a glob (repeatable)
  --redact / --no-redact        Enable or disable value redaction
  --fail-on-secret              Stop if a likely credential is found
  --dry-run                     Inspect and report; write no archive
  --verbose / --quiet           Control diagnostic output
```

Examples:

```bash
itsulu-repo-sanitizer sanitize . --dry-run --verbose
itsulu-repo-sanitizer sanitize . --include-untracked --output /tmp/review.tar.gz
itsulu-repo-sanitizer sanitize . --fail-on-secret --quiet
```

When `--output` is omitted, the archive is named
`<repository>-<short-git-head>-sanitized.tar.zst` (or `.tar.gz` with
`--format tar.gz`) beside the repository. Use `--output` to choose a complete
custom path and filename.

`--dry-run` never creates an archive. There are no interactive prompts, making
the command suitable for agents and CI. Output is ordered and timestamp free
where archive formats permit, so unchanged input yields reproducible output.

## Safety model

The sanitizer excludes `.git`, build products and caches, private keys,
kubeconfigs, credential and environment secret files, databases and dumps,
snapshots, binary blobs, and oversized files. It treats repository content and
filenames as untrusted: tracked paths are validated, symlinks cannot escape the
repository root, and an output archive is not reingested.

Likely credentials in eligible text are detected and their **values** are
replaced while retaining configuration structure. Detection is value oriented:
repository paths, Markdown links, URLs, schema keys, environment variable
references, OpenBao/Vault paths, and Kubernetes `secretKeyRef` or `remoteRef`
references remain intact. Known secret containers can be excluded entirely
when safe partial redaction is not possible. Secret material is never emitted
to console output, errors, reports, or test fixtures.

## Exit codes

| Code | Meaning |
| ---: | --- |
| 0 | Sanitization completed successfully (including dry run). |
| 2 | Command-line usage or configuration error. |
| 3 | Repository discovery, traversal, or input/output I/O error. |
| 4 | A likely secret was found with `--fail-on-secret`. |
| 5 | Archive or report generation failed. |

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo bench
```

See [benchmark guidance](docs/benchmarks.md) for how to run and record the
Criterion performance suite.

## License

Licensed under the Apache License 2.0; see [LICENSE](LICENSE) and
[NOTICE](NOTICE). Contributors retain copyright to their own contributions.
Unless explicitly stated otherwise, contributions are submitted under the
Apache-2.0 license.

Designed and directed by Nicholas Riegel. Developed by Codex