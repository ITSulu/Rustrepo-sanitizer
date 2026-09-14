# GUI foundation

The CLI and Slint GUI consume the shared `sanitizer` core. User-facing capabilities are registered in `src/lib.rs`; additions must be classified there and exposed in the GUI or documented as an exception.

Run the native development preview with `./scripts/gui-preview`. It enables Slint's development-only live reload and must not be used for release builds. For standalone UI iteration, install `slint-viewer` and run `slint-viewer --auto-reload ui/main.slint`.

GUI controls use accessible labels as stable semantic automation identifiers. `xa11y` is the intended AT-SPI black-box test client; `enigo` is reserved for interactions unavailable through semantic access. `insta` is reserved for normalized view-model, capability, and result snapshots. Visual snapshots cover stable states only.

## Acceptance matrix

| Surface | Semantic coverage |
|---|---|
| Repository/output fields | discover, focus, type text |
| Format/compression | discover and inspect values |
| Advanced options | expand and inspect redaction, fail-on-secret, limits, filters, password |
| Sanitize/Cancel | activate and observe status/result |
| Safety | password is never persisted; invalid archive combinations are rejected by the shared core |

The ignored native test is intentionally black-box and must run in a session with D-Bus and AT-SPI enabled; it is not replaced by coordinate automation.

Run the reproducible spike with `./scripts/gui-xa11y`; its current non-zero result is the documented AT-SPI compatibility failure.

### xa11y compatibility spike

On the development workstation, `at-spi2-registryd` and `org.a11y.Bus` are present, and the native Slint process remains alive on `DISPLAY=:0`/Wayland. The focused command `cargo test --test gui_xa11y -- --ignored --nocapture` currently reports `SelectorNotMatched` for `application[name="rustrepo-sanitizer-gui"]`; xa11y lists desktop applications but not the Slint process. This is retained as a failing compatibility gate pending diagnosis of the local Slint/AT-SPI exposure, with no coordinate-based fallback.
