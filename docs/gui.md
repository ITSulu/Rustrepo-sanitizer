# GUI foundation

The CLI and Slint GUI consume the shared `sanitizer` core. User-facing capabilities are registered in `src/lib.rs`; additions must be classified there and exposed in the GUI or documented as an exception.

Run the native development preview with `./scripts/gui-preview`. It enables Slint's development-only live reload and must not be used for release builds. For standalone UI iteration, install `slint-viewer` and run `slint-viewer --auto-reload ui/main.slint`.

GUI controls use accessible labels as stable semantic automation identifiers. `xa11y` is the intended AT-SPI black-box test client; `enigo` is reserved for interactions unavailable through semantic access. `insta` is reserved for normalized view-model, capability, and result snapshots. Visual snapshots cover stable states only.

## Acceptance matrix

| Surface | Semantic coverage |
|---|---|
| Repository/output fields | discover and focus semantically; text-entry compatibility is tracked below |
| Format/compression | discover and inspect values |
| Advanced options | expand and inspect redaction, fail-on-secret, limits, filters, password |
| Sanitize/Cancel | activate and observe status/result |
| Safety | password is never persisted; invalid archive combinations are rejected by the shared core |

The ignored native test is intentionally black-box and must run in a session with D-Bus and AT-SPI enabled; it is not replaced by coordinate automation.

Run the reproducible spike with `./scripts/gui-test`; it bootstraps the dedicated AT-SPI bus and restores the original accessibility state.

### xa11y compatibility spike

The harness distinguishes the normal session bus from the dedicated accessibility bus by calling `org.a11y.Bus.GetAddress`, then verifies `org.a11y.atspi.Registry` on the returned address. The native Slint process is discoverable and semantic controls can be located and activated. The current Slint/AccessKit stack does not expose AT-SPI `EditableText` actions for `LineEdit`; xa11y therefore reports unsupported `InsertText`/`SetTextContents` for text entry. This is documented compatibility evidence, not hidden by coordinate automation.
