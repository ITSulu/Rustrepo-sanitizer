# Web UI

The `rustrepo-sanitizer-web` crate serves an Axum HTTP/API server and a Leptos
server-rendered (SSR) web UI. Both frontends call the same sanitizer core as
the CLI and the Slint desktop GUI, so sanitization logic is never duplicated.

## Run

```bash
cargo run -p itsulu-repo-sanitizer-web
# or, from a release build:
rustrepo-sanitizer-web
```

The server binds to `127.0.0.1:8787` by default. Open
<http://127.0.0.1:8787/> for the UI.

### Environment

| Variable | Purpose |
|---|---|
| `RUSTREPO_WEB_BIND` | Bind address, e.g. `0.0.0.0:8787`. Prefer loopback. |
| `RUSTREPO_WEB_ROOT` | Root for job workspaces and uploads (default: temp dir). |
| `RUSTREPO_WEB_TOKEN` | When set, `/api/*` requires `Authorization: Bearer <token>`. |
| `RUSTREPO_WEB_LOCAL_ROOTS` | Colon-separated roots allowed for the server-local path input mode. |
| `RUSTREPO_WEB_FORGEJO_BASE` | Forgejo base URL, e.g. `https://git.itsulu.com`. |
| `RUSTREPO_WEB_FORGEJO_TOKEN` | Forgejo token (server-side only). |
| `RUSTREPO_WEB_GITHUB_API` | GitHub API base (default `https://api.github.com`). |
| `RUSTREPO_WEB_GITHUB_TOKEN` | GitHub token (server-side only). |

Integration tokens are only ever sent to the upstream API. They are never
serialized into a response and never reach the browser.

## Architecture

- `security` — URL/SSRF validation, path traversal checks, and safe archive
  extraction.
- `acquire` — resolves each of the five input modes into an isolated checkout.
- `workspace` — bounded, expiring job workspaces with deterministic cleanup.
- `uploads` — opaque-id upload store with streaming size enforcement.
- `integrations` — Forgejo/GitHub clients; tokens stay in `IntegrationsConfig`.
- `dto` — request/response types and the mapping onto the shared core `Config`.
- `jobs` — in-memory job registry and the sanitization runner.
- `reports` — extracts the archive's report members for browser download.
- `routes` — the token-protected JSON API and the SSR UI.
- `ui` — Leptos components.

## API

| Method | Path | Description |
|---|---|---|
| GET | `/api/health` | Liveness and version. |
| GET | `/api/capabilities` | Capability/help metadata shared with CLI and GUI. |
| POST | `/api/validate` | Validate an input spec and options. |
| POST | `/api/uploads` | Upload an archive (multipart, field `file`). |
| GET | `/api/integrations` | Which integrations are configured. |
| GET | `/api/integrations/{forgejo,github}/repos` | List accessible repositories. |
| POST | `/api/jobs` | Create a sanitization job. |
| GET | `/api/jobs/{id}` | Job status and progress. |
| POST | `/api/jobs/{id}/cancel` | Request cancellation. |
| GET | `/api/jobs/{id}/download` | Download the archive. |
| GET | `/api/jobs/{id}/reports/{name}` | Download an embedded report. |

`POST /api/jobs` body:

```json
{
  "input": { "mode": "git_url", "url": "https://git.example.com/org/repo.git" },
  "options": { "format": "tar", "compression": "zstd", "report": "markdown" }
}
```

Input modes: `local_path`, `git_url`, `upload`, `forgejo`, `github`.

## Security model

- **URLs** must be `https`, must not embed credentials, and must not resolve to
  private/loopback/reserved addresses. The `ext` and `file` Git transports are
  disabled.
- **Uploads** are streamed under a size bound and extracted with traversal,
  symlink, entry-count, and byte-budget checks.
- **Paths** for output names, workspace members, and local repository roots are
  normalized and validated; traversal is rejected.
- **Git refs** are validated to prevent option injection.
- **API auth** is a constant-time bearer-token check applied to all `/api/*`
  routes when `RUSTREPO_WEB_TOKEN` is set. Bind to loopback unless you provide
  authentication in front of the UI.
- **Workspaces** are isolated per job and removed deterministically.

## Measurements

`crates/web/tests/performance.rs` records request latency, idle memory, browser
payload size, concurrent-job throughput, and cleanup on every run. Representative
values from a debug test build (release builds are faster and smaller):

| Metric | Value |
|---|---|
| Idle RSS | ~14 MiB |
| `/api/capabilities` latency | ~1.3 ms |
| `/` SSR render latency | ~1.2 ms |
| Capabilities JSON size | ~5 KB |
| Browser payload for the UI shell | ~13 KB (server-rendered; no JS bundle) |
| Concurrent jobs (bounded, `max_concurrent_jobs = 4`) | all terminal, < 0.2 s for a small repo |
| Workspace cleanup after expiry | 0 leftover directories |

Server-rendered HTML keeps the browser payload minimal: the UI is fully
keyboard-operable without JavaScript, so there is no hydration bundle to ship.
