# Web UI

The unified `Rustrepo-sanitizer` executable serves an Axum HTTP/API server and a
Leptos server-rendered (SSR) web UI when started with `--web`. All interfaces
(CLI, Slint desktop GUI, web) call the same sanitizer core, so sanitization
logic is never duplicated.

## Run

```bash
# web UI only
cargo run -- --web
# GUI and web together, in one process:
cargo run -- --gui --web
# from a release build:
Rustrepo-sanitizer --web
Rustrepo-sanitizer --gui --web
```

The server binds to `127.0.0.1:8787` by default. Open
<http://127.0.0.1:8787/> for the UI.

### Launch options

| Option | Purpose |
|---|---|
| `--web` | Start the web UI and HTTP API. |
| `--gui` | Start the desktop GUI. |
| `--gui --web` | Run both in one process. |
| `--web-bind <ADDR>` | Bind address (equivalent to `RUSTREPO_WEB_BIND`). |
| `--web-token <TOKEN>` | Require a bearer token for the API and UI login. |
| `--web-root <DIR>` | Workspace/upload root. |
| `--web-local-roots <PATHS>` | Allowed local-path roots. |
| `--web-forgejo-base <URL>` / `--web-forgejo-token <TOKEN>` | Forgejo selector. |
| `--web-github-api <URL>` / `--web-github-token <TOKEN>` | GitHub selector. |

Web options are only accepted together with `--web`.

### Environment

| Variable | Purpose |
|---|---|
| `RUSTREPO_WEB_BIND` | Bind address, e.g. `0.0.0.0:8787`. Prefer loopback. |
| `RUSTREPO_WEB_ROOT` | Root for job workspaces and uploads (default: temp dir). |
| `RUSTREPO_WEB_TOKEN` | When set, `/api/*` requires `Authorization: Bearer <token>` and the UI requires signing in at `/ui/login` (an HttpOnly session cookie). |
| `RUSTREPO_WEB_LOCAL_ROOTS` | Colon-separated roots allowed for the server-local path input mode. |
| `RUSTREPO_WEB_FORGEJO_BASE` | Forgejo base URL, e.g. `https://git.itsulu.com`. |
| `RUSTREPO_WEB_FORGEJO_TOKEN` | Forgejo token (server-side only). |
| `RUSTREPO_WEB_GITHUB_API` | GitHub API base (default `https://api.github.com`). |
| `RUSTREPO_WEB_GITHUB_TOKEN` | GitHub token (server-side only). |

Integration tokens are only ever sent to the upstream API. They are never
serialized into a response and never reach the browser.

## Interface

The page is server rendered and works without JavaScript. Two scripts are
progressive enhancements only: one keeps the maximum file size in step with its
unit selector, and one drives the include/exclude glob lists. Everything is
still submitted and resolved server side when scripting is off.

- **Top navigation** moves between the Sanitize form and the Option Reference.
- **Option Reference** lists every option with a one-line tooltip that appears
  after a two second hover, matching the desktop GUI's hover help.
- **Repository Source** offers Server Local Path, Git URL, Uploaded Archive,
  Forgejo Repository, and GitHub Repository. A Branch Or Tag field sits beside
  the repository fields and applies to Forgejo, GitHub, and plain Git URLs.
- **Maximum File Size** in Output is a numeric field with a KiB/MiB/GiB
  selector. Changing the unit preserves the exact byte count; the form carries
  the byte-equivalent value, and the server clamps any submitted value to a
  usable range.
- **Filters** offer a common-pattern dropdown, an Add button, and a custom
  pattern field for both include and exclude globs. Each added pattern is
  listed with a Remove control and submitted as a newline separated value.
- **Supported Formats** appears inside Output, below Maximum File Size.
- Repository, path, and tag fields use non-email input semantics with autofill
  disabled, so browsers do not offer email aliases for them.

### Browser tests

```bash
npm install
npx playwright install chromium
npx playwright test
```

The suite starts the real binary in web mode with a temporary fixture
repository. `RRS_TEST_LOCAL=1` runs the local sanitize, download, and filter
cases; `RRS_TEST_NETWORK=1` additionally clones a public repository to exercise
real acquisition, branch selection, and unknown-branch rejection. Forgejo runs
the local subset on the primary runner.

## Architecture

All modules live under `src/web/`.

- `security` — URL/SSRF validation, path traversal checks, and safe archive
  extraction.
- `acquire` — resolves each of the five input modes into an isolated checkout.
- `workspace` — bounded, expiring job workspaces with deterministic cleanup.
- `uploads` — opaque-id upload store with streaming size enforcement.
- `server` — web server lifecycle (web-only, or a background thread for GUI+Web).
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
  routes when `RUSTREPO_WEB_TOKEN` is set, and the UI requires signing in
  (`/ui/login`) with an `HttpOnly` session cookie that is marked `Secure` when
  the bind address is not loopback. Bind to loopback unless you provide
  authentication in front of the UI.
- **Residual SSRF**: the resolved address is validated before cloning, redirects
  are disabled, and private/reserved addresses are rejected, but a
  DNS-rebinding host could still resolve to an internal address at clone time
  (the validated answer is not pinned). Run the server where it cannot reach
  unintended internal services.
- **Workspaces** are isolated per job and removed deterministically.

## Measurements

`tests/performance.rs` records request latency, idle memory, browser
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

### Unified-binary idle memory

Release-build idle RSS (`VmRSS`), measured after startup with the server running:

| Configuration | Idle RSS |
|---|---|
| 0.6.0 `rustrepo-sanitizer-web` (web only) | ~5 MiB |
| 0.6.1 `Rustrepo-sanitizer --web` | ~10 MiB |
| 0.6.1 `Rustrepo-sanitizer --gui --web` | ~185 MiB (Slint/winit + GL) |

Web-only memory rises slightly over 0.6.0 because the same binary also links the
GUI toolkit; the GUI stack is only initialized when `--gui` is used. GUI + Web
adds the Slint windowing/renderer footprint and runs both interfaces in one
process with no helper executable.
