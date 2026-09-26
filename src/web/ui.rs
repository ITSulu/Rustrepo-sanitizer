//! Leptos server-rendered web UI.
//!
//! Rendered to HTML by the Axum server. Every control is a native form element
//! with an explicit label and `aria-describedby` help text, so the workflow is
//! fully keyboard-operable and does not require JavaScript. Submitted values are
//! echoed back on validation errors, and job status is announced via a live
//! region (no auto-refresh that would interrupt assistive technology).

use std::collections::HashMap;

use leptos::prelude::*;

use crate::size::{self, SizeUnit};
use crate::web::dto::CapabilitiesView;
use crate::web::jobs::JobStatus;
use crate::web::jobs::JobView;
use crate::{COMMON_EXCLUDE_GLOBS, COMMON_INCLUDE_GLOBS};

/// How long an option-reference tooltip waits before appearing.
const TOOLTIP_DELAY_MS: u32 = 2000;

const STYLE: &str = r#"
:root { color-scheme: light dark; --fg:#111827; --bg:#f8fafc; --card:#ffffff; --accent:#1d4ed8; --border:#cbd5e1; --muted:#475569; --opt-fg:#111827; --opt-bg:#ffffff; --accent-fill:#1d4ed8; }
@media (prefers-color-scheme: dark) { :root { --fg:#e5e7eb; --bg:#0b1220; --card:#111827; --accent:#93c5fd; --border:#64748b; --muted:#94a3b8; --opt-fg:#f1f5f9; --opt-bg:#1e293b; --accent-fill:#1d4ed8; } }
* { box-sizing: border-box; }
body { margin:0; font:16px/1.5 system-ui, sans-serif; color:var(--fg); background:var(--bg); }
a, button, input, select, textarea { font: inherit; }
a:focus-visible, button:focus-visible, input:focus-visible, select:focus-visible, textarea:focus-visible, summary:focus-visible { outline:3px solid var(--accent); outline-offset:2px; }
.skip { position:absolute; left:-9999px; }
.skip:focus { left:1rem; top:1rem; background:var(--card); padding:.5rem 1rem; z-index:10; }
header, main, footer { max-width: 72rem; margin: 0 auto; padding: 1rem; }
h1 { font-size: clamp(1.5rem, 4vw, 2.25rem); }
nav#nav { display:flex; gap:1rem; border-bottom:1px solid var(--border); padding-bottom:.5rem; margin-bottom:1rem; }
nav#nav a { text-decoration:none; padding:.35rem .6rem; border-radius:.35rem; }
nav#nav a[aria-current="page"] { background:var(--accent-fill); color:#fff; }
fieldset { border:1px solid var(--border); border-radius:.5rem; padding:1rem; margin:0 0 1rem; background:var(--card); }
legend { font-weight:600; padding:0 .35rem; }
.grid { display:grid; gap:.75rem 1rem; grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr)); }
.field { display:flex; flex-direction:column; gap:.25rem; }
.field.inline { flex-direction:row; align-items:center; gap:.5rem; }
.row { display:grid; gap:.75rem 1rem; grid-template-columns: minmax(0,1fr) minmax(0,1fr); }
.help { color:var(--muted); font-size:.85rem; }
.visually-hidden { position:absolute; width:1px; height:1px; margin:-1px; padding:0; overflow:hidden; clip-path:inset(50%); white-space:nowrap; border:0; }
.table-wrap { overflow-x:auto; margin-top:1rem; }
input[type=text], input[type=url], input[type=password], input[type=number], select, textarea { padding:.5rem; border:1px solid var(--border); border-radius:.35rem; background:var(--card); color:var(--fg); width:100%; }
select option { background:var(--opt-bg); color:var(--opt-fg); }
.size-row { display:flex; gap:.5rem; }
.size-row input[type=number] { flex:1 1 auto; }
.size-row select { flex:0 0 7rem; }
button { background:var(--accent-fill); color:#fff; border:0; border-radius:.35rem; padding:.6rem 1.1rem; cursor:pointer; }
button.secondary { background:transparent; color:var(--accent); border:1px solid var(--accent); }
.status { border-left:4px solid var(--accent); padding:.75rem 1rem; background:var(--card); margin:1rem 0; }
.status[data-kind="error"] { border-color:#dc2626; }
.status[data-kind="ok"] { border-color:#16a34a; }
table { border-collapse:collapse; width:100%; word-break:break-word; }
caption { text-align:left; padding:.25rem 0 .5rem; color:var(--muted); }
th, td { text-align:left; padding:.4rem .5rem; border-bottom:1px solid var(--border); }
.tag-list { list-style:none; margin:.5rem 0 0; padding:0; }
.tag-list li { display:flex; align-items:center; gap:.5rem; margin-bottom:.25rem; }
.tag-list code { background:var(--card); border:1px solid var(--border); border-radius:.25rem; padding:.1rem .35rem; }
/* Option-reference tooltips appear only after a deliberate hover. */
.tip { position:relative; display:inline-block; }
.tip .tip-text { position:absolute; left:0; top:1.4em; z-index:20; width:max-content; max-width:min(32rem, 90vw); white-space:normal; text-wrap:balance; padding:.4rem .6rem; border-radius:.35rem; background:var(--fg); color:var(--bg); font-size:.85rem; line-height:1.3; opacity:0; visibility:hidden; transition:opacity .1s linear; transition-delay: 2s; }
.tip:hover .tip-text, .tip:focus-within .tip-text { opacity:1; visibility:visible; }
@media (prefers-reduced-motion: reduce) { .tip .tip-text { transition:none; } }
@media (max-width: 40rem) { header, main, footer { padding:.75rem; } .grid { grid-template-columns: 1fr; } .row { grid-template-columns: 1fr; } }
"#;

/// Preserves the submitted form values across an error re-render.
pub type FormValues = HashMap<String, String>;

fn value_of(values: &FormValues, key: &str) -> String {
    values.get(key).cloned().unwrap_or_default()
}

fn text_value(values: &FormValues, key: &str, initial: &str) -> String {
    match values.get(key) {
        Some(value) => value.clone(),
        None if values.is_empty() => initial.to_owned(),
        None => String::new(),
    }
}

fn is_checked(values: &FormValues, key: &str, default_on: bool) -> bool {
    if values.is_empty() {
        default_on
    } else {
        values.contains_key(key)
    }
}

/// Splits a textarea value into individual glob patterns.
fn split_globs(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Resolves the submitted maximum file size into bytes plus the display unit.
///
/// Accepts the byte-equivalent hidden field produced by the unit selector, and
/// falls back to parsing the human value with its unit.
pub fn submitted_max_file_size(values: &FormValues) -> (String, SizeUnit, u64) {
    if let Some(bytes) = values
        .get("max_file_size_bytes")
        .and_then(|raw| raw.trim().parse::<u64>().ok())
    {
        let value = size::Size::from_bytes(bytes);
        let unit = value.display().unit();
        if matches!(unit, SizeUnit::Kib | SizeUnit::Mib | SizeUnit::Gib) {
            return (value.value_in(unit), unit, bytes);
        }
        return (value.value_in(SizeUnit::Kib), SizeUnit::Kib, bytes);
    }
    let raw = values
        .get("max_file_size")
        .cloned()
        .unwrap_or_else(|| "10MiB".to_owned());
    // With no unit field, fall back to the shared parser so a bare byte value
    // from an API client keeps its original meaning.
    if !values.contains_key("max_file_size_unit") {
        return match size::parse_size(&raw) {
            Ok(parsed) => {
                let unit = parsed.display().unit();
                (parsed.value_in(unit), unit, parsed.bytes())
            }
            Err(_) => {
                let fallback = size::Size::new(10.0, SizeUnit::Mib);
                (
                    fallback.value_in(SizeUnit::Mib),
                    SizeUnit::Mib,
                    fallback.bytes(),
                )
            }
        };
    }
    let unit = values
        .get("max_file_size_unit")
        .and_then(|raw| match raw.trim() {
            "KiB" => Some(SizeUnit::Kib),
            "MiB" => Some(SizeUnit::Mib),
            "GiB" => Some(SizeUnit::Gib),
            _ => None,
        })
        .unwrap_or(SizeUnit::Mib);
    // The form value is expressed in the selected unit, so parse it as a
    // number and apply the unit explicitly rather than re-reading it as bytes.
    let parsed = raw
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| size::Size::new(value, unit));
    match parsed {
        Some(parsed) => (parsed.value_in(unit), unit, parsed.bytes()),
        None => {
            let fallback = size::Size::new(10.0, SizeUnit::Mib);
            (fallback.value_in(unit), unit, fallback.bytes())
        }
    }
}

fn unit_options(selected: SizeUnit) -> Vec<AnyView> {
    SizeUnit::ALL
        .iter()
        .map(|unit| {
            let is_selected = *unit == selected;
            let label = unit.suffix().to_owned();
            let value = label.clone();
            view! { <option value=value selected=is_selected>{label}</option> }.into_any()
        })
        .collect()
}

fn glob_options(presets: &[&str]) -> Vec<AnyView> {
    let mut out: Vec<AnyView> = Vec::new();
    out.push(view! { <option value="">"Select a common pattern…"</option> }.into_any());
    for preset in presets {
        let value = (*preset).to_owned();
        let label = value.clone();
        out.push(view! { <option value=value>{label}</option> }.into_any());
    }
    out
}

/// Progressive enhancement for the maximum file size field.
///
/// Keeps the byte-equivalent hidden field in step with the visible value and
/// unit. The form submits every field, so without scripting the server still
/// resolves the value with the shared parser.
const SIZE_SYNC_SCRIPT: &str = r#"
(function () {
  var unitBytes = { KiB: 1024, MiB: 1048576, GiB: 1073741824 };
  var maxBytes = 9007199254740991; // Number.MAX_SAFE_INTEGER
  var value = document.getElementById('max_file_size');
  var unit = document.getElementById('max_file_size_unit');
  var bytes = document.getElementById('max_file_size_bytes');
  if (!value || !unit || !bytes) { return; }
  function factor() { return unitBytes[unit.value] || 0; }
  function sync() {
    var n = parseFloat(value.value);
    var f = factor();
    if (isFinite(n) && n >= 0 && f && n * f <= maxBytes) {
      bytes.value = String(Math.round(n * f));
    } else {
      // Clearing rather than leaving a stale value lets the server fall back to
      // the visible value and its unit instead of applying the wrong size.
      bytes.value = '';
    }
  }
  // Rewrites the visible value when the unit changes, preserving the size. The
  // exact byte count is stored first so a rounded display can never move the
  // size that is actually applied.
  function convertUnit() {
    var n = parseFloat(value.value);
    var previous = unitBytes[bytes.dataset.previousUnit];
    var f = factor();
    if (isFinite(n) && n >= 0 && previous && f) {
      var exact = Math.min(Math.round(n * previous), maxBytes);
      bytes.value = String(exact);
      value.value = String(exact / f);
    }
    bytes.dataset.previousUnit = unit.value;
    sync();
  }
  value.addEventListener('input', sync);
  value.addEventListener('change', sync);
  unit.addEventListener('change', convertUnit);
  bytes.dataset.previousUnit = unit.value;
})();
"#;

/// Adds the selected or typed pattern to a filter list.
///
/// The list is submitted as a newline separated value, which is exactly what
/// the server parses, so this only mirrors what the Add button means.
const GLOB_LIST_SCRIPT: &str = r#"
(function () {
  function addPattern(kind) {
    var choice = document.getElementById(kind + '-choice');
    var entry = document.getElementById(kind + '-entry');
    var list = document.getElementById(kind + '-list');
    var hidden = document.getElementById(kind + '-globs');
    if (!choice || !entry || !list || !hidden) { return; }
    var pattern = entry.value.trim() || choice.value;
    if (!pattern) { return; }
    var current = hidden.value ? hidden.value.split('\n') : [];
    if (current.indexOf(pattern) === -1) { current.push(pattern); }
    hidden.value = current.join('\n');
    render(kind, list, current);
    entry.value = '';
    choice.value = '';
  }
  function render(kind, list, patterns) {
    list.textContent = '';
    patterns.forEach(function (pattern) {
      var item = document.createElement('li');
      var code = document.createElement('code');
      code.textContent = pattern;
      item.appendChild(code);
      var remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'secondary';
      remove.textContent = 'Remove';
      remove.setAttribute('aria-label', 'Remove ' + pattern);
      remove.addEventListener('click', function () {
        var next = hidden.value.split('\n').filter(function (entry) {
          return entry && entry !== pattern;
        });
        hidden.value = next.join('\n');
        render(kind, list, next);
      });
      item.appendChild(remove);
      list.appendChild(item);
    });
  }
  ['include', 'exclude'].forEach(function (kind) {
    var button = document.getElementById(kind + '-add');
    if (button) { button.addEventListener('click', function () { addPattern(kind); }); }
  });
})();
"#;

#[component]
pub fn App(
    caps: CapabilitiesView,
    job: Option<JobView>,
    flash: Option<String>,
    error: Option<String>,
    values: FormValues,
) -> impl IntoView {
    let defaults = caps.defaults.clone();
    let version = caps.version.clone();
    let formats: Vec<String> = caps.formats.iter().map(|f| f.name.clone()).collect();
    let compressions: Vec<String> = caps.compressions.iter().map(|c| c.name.clone()).collect();
    let reports = ["markdown", "json", "none"];
    let default_format = serde_json::to_value(defaults.format)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "tar".into());
    let default_compression = serde_json::to_value(defaults.compression)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "zstd".into());
    let default_report = serde_json::to_value(defaults.report)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "markdown".into());

    let chosen_format = values.get("format").cloned().unwrap_or(default_format);
    let chosen_compression = values
        .get("compression")
        .cloned()
        .unwrap_or(default_compression);
    let chosen_report = values.get("report").cloned().unwrap_or(default_report);
    let chosen_mode = values
        .get("mode")
        .cloned()
        .unwrap_or_else(|| "local_path".to_owned());
    let has_error = error.is_some();
    let help_groups = caps.help.groups.clone();

    let (size_value, size_unit, size_bytes) = submitted_max_file_size(&values);
    let include_globs = split_globs(&text_value(&values, "includes", ""));
    let exclude_globs = split_globs(&text_value(&values, "excludes", ""));
    let include_globs_value = include_globs.join("\n");
    let exclude_globs_value = exclude_globs.join("\n");

    let mode_opt = |value: &str, label: &str| {
        let selected = chosen_mode == value;
        let value = value.to_owned();
        let label = label.to_owned();
        view! { <option value=value selected=selected>{label}</option> }.into_any()
    };

    view! {
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>"Rustrepo Sanitizer Web"</title>
                <style>{STYLE}</style>
            </head>
            <body>
                <a class="skip" href="#main">"Skip to main content"</a>
                <header>
                    <h1>"Rustrepo Sanitizer Web"</h1>
                    <p class="help">"Create a deterministic, sanitized AI review bundle. Version " {version}</p>
                    <nav id="nav" aria-label="Sections">
                        <a href="#sanitize" aria-current="page">"Sanitize"</a>
                        <a href="#option-reference">"Option Reference"</a>
                    </nav>
                </header>
                <main id="main">
                    {error.map(|message| view! {
                        <p id="form-error" class=move || "status" data-kind="error" role="alert">{message}</p>
                    })}
                    {flash.map(|message| view! {
                        <p class=move || "status" data-kind="ok" role="status">{message}</p>
                    })}
                    {job.map(|job| render_job(&job))}
                    <section id="sanitize" aria-labelledby="form-heading">
                        <h2 id="form-heading">"Sanitize A Repository"</h2>
                        <form method="post" action="/ui/jobs" enctype="multipart/form-data" aria-describedby=has_error.then_some("form-error")>
                            <fieldset>
                                <legend>"Repository Source"</legend>
                                <div class="field">
                                    <label for="mode">"Input Method"</label>
                                    <select id="mode" name="mode" aria-describedby="mode-help">
                                        {mode_opt("local_path", "Server Local Path")}
                                        {mode_opt("git_url", "Git URL")}
                                        {mode_opt("upload", "Upload Archive")}
                                        {mode_opt("forgejo", "Forgejo Repository")}
                                        {mode_opt("github", "GitHub Repository")}
                                    </select>
                                    <small class="help" id="mode-help">"Choose how the server obtains the repository. Fields that do not apply may be left blank."</small>
                                </div>
                                <div class="field" id="path-row">
                                    <label for="path">"Server Local Path"</label>
                                    <input type="text" id="path" name="path" autocomplete="off" autocapitalize="off" spellcheck="false" value=value_of(&values, "path") aria-describedby="path-help"/>
                                    <small class="help" id="path-help">"Absolute path to a repository inside an allowed server root."</small>
                                </div>
                                <div class="field" id="forgejo-row">
                                    <label for="forgejo-repo">"Forgejo Repository"</label>
                                    <input type="text" id="forgejo-repo" name="forgejo_repo" placeholder="owner/name" autocomplete="off" autocapitalize="off" spellcheck="false" value=value_of(&values, "forgejo_repo") aria-describedby="forgejo-help"/>
                                    <small class="help" id="forgejo-help">"Cloned server-side with the configured Forgejo token."</small>
                                </div>
                                <div class="field" id="url-row">
                                    <label for="url">"Git URL"</label>
                                    <input type="text" id="url" name="url" inputmode="url" autocomplete="off" autocapitalize="off" spellcheck="false" value=value_of(&values, "url") aria-describedby="url-help"/>
                                    <small class="help" id="url-help">"Public https Git repository. Private and loopback hosts are rejected."</small>
                                </div>
                                <div class="field" id="upload-row">
                                    <label for="upload">"Upload Archive"</label>
                                    <input type="file" id="upload" name="upload" accept=".zip,.tar,.tar.gz,.tgz" aria-describedby="upload-help"/>
                                    <small class="help" id="upload-help">"Zip or tar(.gz) archive of a Git repository, extracted safely."</small>
                                </div>
                                <div class="row" id="github-row">
                                    <div class="field">
                                        <label for="github-repo">"GitHub Repository"</label>
                                        <input type="text" id="github-repo" name="github_repo" placeholder="owner/name" autocomplete="off" autocapitalize="off" spellcheck="false" value=value_of(&values, "github_repo") aria-describedby="github-help"/>
                                        <small class="help" id="github-help">"Cloned server-side with the configured GitHub token."</small>
                                    </div>
                                    <div class="field" id="branch-row">
                                        <label for="git-ref">"Branch Or Tag"</label>
                                        <input type="text" id="git-ref" name="git_ref" autocomplete="off" autocapitalize="off" spellcheck="false" value=value_of(&values, "git_ref") aria-describedby="git-ref-help"/>
                                        <small class="help" id="git-ref-help">"Optional branch or tag for Forgejo and GitHub sources."</small>
                                    </div>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Output"</legend>
                                <div class="grid">
                                    <div class="field">
                                        <label for="format">"Archive Format"</label>
                                        <select id="format" name="format" aria-describedby="format-help">
                                            {formats.iter().map(|f| select_option(&chosen_format, f)).collect_view()}
                                        </select>
                                        <small class="help" id="format-help">"Container format for the sanitized bundle."</small>
                                    </div>
                                    <div class="field">
                                        <label for="compression">"Compression"</label>
                                        <select id="compression" name="compression" aria-describedby="compression-help">
                                            {compressions.iter().map(|c| select_option(&chosen_compression, c)).collect_view()}
                                        </select>
                                        <small class="help" id="compression-help">"Only codecs valid for the chosen format are accepted."</small>
                                    </div>
                                    <div class="field">
                                        <label for="report">"Report Format"</label>
                                        <select id="report" name="report" aria-describedby="report-help">
                                            {reports.iter().map(|r| select_option(&chosen_report, r)).collect_view()}
                                        </select>
                                        <small class="help" id="report-help">"Human-readable Markdown, JSON, or none."</small>
                                    </div>
                                    <div class="field">
                                        <label for="max_file_size">"Maximum File Size"</label>
                                        <div class="size-row">
                                            <input type="number" id="max_file_size" name="max_file_size" min="0" step="any" value=size_value aria-describedby="max-file-size-help"/>
                                            <select id="max_file_size_unit" name="max_file_size_unit" aria-label="Maximum file size unit">
                                                {unit_options(size_unit)}
                                            </select>
                                        </div>
                                        <input type="hidden" id="max_file_size_bytes" name="max_file_size_bytes" value=size_bytes.to_string()/>
                                        <small class="help" id="max-file-size-help">"Files larger than this are excluded. Changing the unit keeps the same size."</small>
                                    </div>
                                    <div class="field">
                                        <label for="output_name">"Output Filename (Optional)"</label>
                                        <input type="text" id="output_name" name="output_name" autocomplete="off" spellcheck="false" value=value_of(&values, "output_name") aria-describedby="output-name-help"/>
                                        <small class="help" id="output-name-help">"Filename without directories; the server stores it in the job workspace."</small>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="include_untracked" name="include_untracked" value="1" checked=is_checked(&values, "include_untracked", false)/>
                                        <label for="include_untracked">"Include Untracked Files"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="timestamp_name" name="timestamp_name" value="1" checked=is_checked(&values, "timestamp_name", true)/>
                                        <label for="timestamp_name">"Timestamp Output Filename"</label>
                                    </div>
                                </div>
                                <div class="table-wrap">
                                    <h3>Supported Formats</h3>
                                    <table>
                                        <caption>"Archive formats and password support"</caption>
                                        <thead><tr><th scope="col">"Format"</th><th scope="col">"Compressions"</th><th scope="col">"Password"</th></tr></thead>
                                        <tbody>
                                            {caps.formats.iter().map(|format| {
                                                let name = format.name.clone();
                                                let compressions = format.compressions.join(", ");
                                                let password = if format.password_encryption { "Yes" } else { "No" };
                                                view! { <tr><td>{name}</td><td>{compressions}</td><td>{password}</td></tr> }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Filters"</legend>
                                <div class="grid">
                                    <div class="field">
                                        <label for="include-choice">"Include Globs"</label>
                                        <div class="size-row">
                                            <select id="include-choice" name="include_choice" aria-describedby="include-glob-help">
                                                {glob_options(&COMMON_INCLUDE_GLOBS)}
                                            </select>
                                            <button type="button" id="include-add" class="secondary" aria-label="Add include glob">"Add"</button>
                                        </div>
                                        <input type="hidden" id="include-globs" name="includes" value=include_globs_value/>
                                        <label class="visually-hidden" for="include-entry">"Custom Include Pattern"</label>
                                        <input type="text" id="include-entry" name="include_entry" placeholder="Custom pattern" autocomplete="off" spellcheck="false" aria-describedby="include-glob-help"/>
                                        <small class="help" id="include-glob-help">"Only files matching these patterns are packed."</small>
                                        <ul class="tag-list" id="include-list" aria-label="Selected include globs">
                                            {include_globs.iter().map(|glob| {
                                                let code = glob.clone();
                                                view! { <li><code>{code}</code></li> }
                                            }).collect_view()}
                                        </ul>
                                    </div>
                                    <div class="field">
                                        <label for="exclude-choice">"Exclude Globs"</label>
                                        <div class="size-row">
                                            <select id="exclude-choice" name="exclude_choice" aria-describedby="exclude-glob-help">
                                                {glob_options(&COMMON_EXCLUDE_GLOBS)}
                                            </select>
                                            <button type="button" id="exclude-add" class="secondary" aria-label="Add exclude glob">"Add"</button>
                                        </div>
                                        <input type="hidden" id="exclude-globs" name="excludes" value=exclude_globs_value/>
                                        <label class="visually-hidden" for="exclude-entry">"Custom Exclude Pattern"</label>
                                        <input type="text" id="exclude-entry" name="exclude_entry" placeholder="Custom pattern" autocomplete="off" spellcheck="false" aria-describedby="exclude-glob-help"/>
                                        <small class="help" id="exclude-glob-help">"Files matching these patterns are left out."</small>
                                        <ul class="tag-list" id="exclude-list" aria-label="Selected exclude globs">
                                            {exclude_globs.iter().map(|glob| {
                                                let code = glob.clone();
                                                view! { <li><code>{code}</code></li> }
                                            }).collect_view()}
                                        </ul>
                                    </div>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Redaction And Safety"</legend>
                                <div class="grid">
                                    <div class="field inline">
                                        <input type="checkbox" id="redact" name="redact" value="1" checked=is_checked(&values, "redact", true)/>
                                        <label for="redact">"Redact Detected Secrets"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="fail_on_secret" name="fail_on_secret" value="1" checked=is_checked(&values, "fail_on_secret", false)/>
                                        <label for="fail_on_secret">"Fail When Secrets Are Detected"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="dry_run" name="dry_run" value="1" checked=is_checked(&values, "dry_run", false)/>
                                        <label for="dry_run">"Dry Run (No Archive)"</label>
                                    </div>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Password (ZIP AES Only)"</legend>
                                <div class="grid">
                                    <div class="field">
                                        <label for="password">"Password"</label>
                                        <input type="password" id="password" name="password" autocomplete="new-password" aria-describedby="password-help"/>
                                        <small class="help" id="password-help">"Never stored or logged. Only valid with the zip format."</small>
                                    </div>
                                    <div class="field">
                                        <label for="password_min_length">"Minimum Length"</label>
                                        <input type="number" id="password_min_length" name="password_min_length" min="1" max="256" value=text_value(&values, "password_min_length", "8") aria-describedby="password-min-help"/>
                                        <small class="help" id="password-min-help">"Minimum number of characters required."</small>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_uppercase" name="password_require_uppercase" value="1" checked=is_checked(&values, "password_require_uppercase", true)/>
                                        <label for="password_require_uppercase">"Require Uppercase"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_lowercase" name="password_require_lowercase" value="1" checked=is_checked(&values, "password_require_lowercase", true)/>
                                        <label for="password_require_lowercase">"Require Lowercase"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_number" name="password_require_number" value="1" checked=is_checked(&values, "password_require_number", true)/>
                                        <label for="password_require_number">"Require Number"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_special" name="password_require_special" value="1" checked=is_checked(&values, "password_require_special", true)/>
                                        <label for="password_require_special">"Require Special Character"</label>
                                    </div>
                                </div>
                            </fieldset>
                            <button type="submit">"Start Sanitizing"</button>
                        </form>
                    </section>
                    <section id="option-reference" aria-labelledby="help-heading">
                        <h2 id="help-heading">"Option Reference"</h2>
                        <p class="help">
                            {format!("Hover a field name for one second to see its help. Tooltips appear after {TOOLTIP_DELAY_MS} ms.")}
                        </p>
                        {help_groups.into_iter().map(|group| view! {
                            <h3>{group.heading}</h3>
                            <dl>
                                {group.entries.into_iter().map(|entry| {
                                    let field = entry.field;
                                    let tip = format!("tip-{field}");
                                    let text = entry.text;
                                    view! {
                                        <dt>
                                            <span class="tip" tabindex="0">
                                                {field}
                                                <span class="tip-text" id=tip role="tooltip">{text}</span>
                                            </span>
                                        </dt>
                                    }
                                }).collect_view()}
                            </dl>
                        }).collect_view()}
                    </section>
                </main>
                <footer>
                    <p class="help">"Rustrepo-sanitizer reuses one sanitizer core across the CLI, desktop GUI, and this web UI."</p>
                </footer>
                <script>{SIZE_SYNC_SCRIPT}</script>
                <script>{GLOB_LIST_SCRIPT}</script>
            </body>
        </html>
    }
}

fn select_option(selected: &str, item: &str) -> AnyView {
    let item = item.to_owned();
    let is_selected = item == selected;
    let value = item.clone();
    view! { <option value=value selected=is_selected>{item}</option> }.into_any()
}

fn render_job(job: &JobView) -> AnyView {
    let id = job.id.clone();
    let download = format!("/ui/jobs/{id}/download");
    let refresh = format!("/ui/jobs/{id}");
    let (kind, text) = match &job.status {
        JobStatus::Queued => ("status", "Queued".to_owned()),
        JobStatus::Running {
            phase,
            examined,
            included,
        } => (
            "status",
            format!("Running: {phase} ({examined} examined, {included} written)"),
        ),
        JobStatus::Completed {
            included,
            excluded,
            redactions,
            dry_run,
            ..
        } => (
            "ok",
            format!(
                "Completed: {included} files, {excluded} excluded, {redactions} redactions{}",
                if *dry_run { " (dry run)" } else { "" }
            ),
        ),
        JobStatus::Failed { message } => ("error", format!("Failed: {message}")),
        JobStatus::Cancelled => ("error", "Cancelled".to_owned()),
    };
    let reports = match &job.status {
        JobStatus::Completed { reports, .. } => reports.clone(),
        _ => Vec::new(),
    };
    let terminal = job.status.is_terminal();
    let completed = matches!(job.status, JobStatus::Completed { .. });
    view! {
        <section aria-labelledby="job-heading">
            <h2 id="job-heading">"Job " {id.clone()}</h2>
            <p class=move || "status" data-kind=kind role="status" aria-live="polite">{text}</p>
            {(!terminal).then(|| view! {
                <p><a href=refresh>"Refresh status"</a></p>
                <form method="post" action=format!("/ui/jobs/{id}/cancel")>
                    <button class="secondary" type="submit">"Cancel"</button>
                </form>
            })}
            {completed.then(|| view! {
                <p><a href=download>"Download sanitized archive"</a></p>
                <ul>
                    {reports.into_iter().map(|name| {
                        let href = format!("/ui/jobs/{id}/reports/{name}");
                        view! { <li><a href=href>{name}</a></li> }
                    }).collect_view()}
                </ul>
            })}
        </section>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> FormValues {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn submitted_size_prefers_the_byte_equivalent_field() {
        let (value, unit, bytes) =
            submitted_max_file_size(&values(&[("max_file_size_bytes", "20971520")]));
        assert_eq!(bytes, 20 * 1024 * 1024);
        assert_eq!(unit, SizeUnit::Mib);
        assert_eq!(value, "20");
    }

    #[test]
    fn submitted_size_parses_value_with_unit_when_no_bytes() {
        let (value, unit, bytes) = submitted_max_file_size(&values(&[
            ("max_file_size", "2"),
            ("max_file_size_unit", "GiB"),
        ]));
        assert_eq!(bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(unit, SizeUnit::Gib);
        assert_eq!(value, "2");
    }

    #[test]
    fn submitted_size_defaults_to_ten_mib() {
        let (_, unit, bytes) = submitted_max_file_size(&FormValues::new());
        assert_eq!(bytes, 10 * 1024 * 1024);
        assert_eq!(unit, SizeUnit::Mib);
    }

    #[test]
    fn submitted_size_falls_back_on_unparsable_input() {
        let (_, _, bytes) = submitted_max_file_size(&values(&[("max_file_size", "not-a-size")]));
        assert_eq!(bytes, 10 * 1024 * 1024);
    }

    #[test]
    fn glob_lists_are_trimmed_and_filtered() {
        assert_eq!(
            split_globs(" src/** \n\n\ttarget/**\n"),
            vec!["src/**".to_owned(), "target/**".to_owned()]
        );
        assert!(split_globs("  \n ").is_empty());
    }
}
