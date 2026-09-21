//! Leptos server-rendered web UI.
//!
//! Rendered to HTML by the Axum server. Every control is a native form element
//! with an explicit label and `aria-describedby` help text, so the workflow is
//! fully keyboard-operable and does not require JavaScript.

use leptos::prelude::*;

use crate::dto::CapabilitiesView;
use crate::jobs::JobStatus;
use crate::jobs::JobView;

const STYLE: &str = r#"
:root { color-scheme: light dark; --fg:#111827; --bg:#f8fafc; --card:#ffffff; --accent:#1d4ed8; --border:#cbd5e1; --muted:#475569; }
@media (prefers-color-scheme: dark) { :root { --fg:#e5e7eb; --bg:#0b1220; --card:#111827; --accent:#93c5fd; --border:#334155; --muted:#94a3b8; } }
* { box-sizing: border-box; }
body { margin:0; font:16px/1.5 system-ui, sans-serif; color:var(--fg); background:var(--bg); }
a, button, input, select, textarea { font: inherit; }
a:focus-visible, button:focus-visible, input:focus-visible, select:focus-visible, textarea:focus-visible, summary:focus-visible { outline:3px solid var(--accent); outline-offset:2px; }
.skip { position:absolute; left:-9999px; }
.skip:focus { left:1rem; top:1rem; background:var(--card); padding:.5rem 1rem; z-index:10; }
header, main, footer { max-width: 72rem; margin: 0 auto; padding: 1rem; }
h1 { font-size: clamp(1.5rem, 4vw, 2.25rem); }
fieldset { border:1px solid var(--border); border-radius:.5rem; padding:1rem; margin:0 0 1rem; background:var(--card); }
legend { font-weight:600; padding:0 .35rem; }
.grid { display:grid; gap:.75rem 1rem; grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr)); }
.field { display:flex; flex-direction:column; gap:.25rem; }
.field.inline { flex-direction:row; align-items:center; gap:.5rem; }
.help { color:var(--muted); font-size:.85rem; }
input[type=text], input[type=url], input[type=password], input[type=number], select, textarea { padding:.5rem; border:1px solid var(--border); border-radius:.35rem; background:transparent; color:inherit; width:100%; }
button { background:var(--accent); color:#fff; border:0; border-radius:.35rem; padding:.6rem 1.1rem; cursor:pointer; }
button.secondary { background:transparent; color:var(--accent); border:1px solid var(--accent); }
.status { border-left:4px solid var(--accent); padding:.75rem 1rem; background:var(--card); margin:1rem 0; }
.status[data-kind="error"] { border-color:#dc2626; }
.status[data-kind="ok"] { border-color:#16a34a; }
table { border-collapse:collapse; width:100%; }
th, td { text-align:left; padding:.4rem .5rem; border-bottom:1px solid var(--border); }
@media (max-width: 40rem) { header, main, footer { padding:.75rem; } .grid { grid-template-columns: 1fr; } }
"#;

fn option(selected: &str, item: &str) -> impl IntoView {
    let item = item.to_owned();
    let selected = selected.to_owned();
    let is_selected = item == selected;
    let value = item.clone();
    view! { <option value=value selected=is_selected>{item}</option> }
}

#[component]
pub fn App(
    caps: CapabilitiesView,
    job: Option<JobView>,
    flash: Option<String>,
    error: Option<String>,
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

    let help_groups = caps.help.groups.clone();
    let refresh = job.as_ref().is_some_and(|job| !job.status.is_terminal());

    view! {
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>"Rustrepo Sanitizer Web"</title>
                {refresh.then(|| view! { <meta http-equiv="refresh" content="2"/> })}
                <style>{STYLE}</style>
            </head>
            <body>
                <a class="skip" href="#main">"Skip to main content"</a>
                <header>
                    <h1>"Rustrepo Sanitizer Web"</h1>
                    <p class="help">"Create a deterministic, sanitized AI review bundle. Version " {version}</p>
                </header>
                <main id="main">
                    {error.map(|message| view! {
                        <p class="status" data-kind="error" role="alert">{message}</p>
                    })}
                    {flash.map(|message| view! {
                        <p class="status" data-kind="ok" role="status">{message}</p>
                    })}
                    {job.map(|job| render_job(&job))}
                    <section aria-labelledby="capabilities-heading">
                        <h2 id="capabilities-heading">"Supported formats"</h2>
                        <table>
                            <thead><tr><th scope="col">"Format"</th><th scope="col">"Compressions"</th><th scope="col">"Password"</th></tr></thead>
                            <tbody>
                                {caps.formats.iter().map(|format| {
                                    let name = format.name.clone();
                                    let compressions = format.compressions.join(", ");
                                    let password = if format.password_encryption { "yes" } else { "no" };
                                    view! { <tr><td>{name}</td><td>{compressions}</td><td>{password}</td></tr> }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </section>
                    <section aria-labelledby="form-heading">
                        <h2 id="form-heading">"Sanitize a repository"</h2>
                        <form method="post" action="/ui/jobs" enctype="multipart/form-data">
                            <fieldset>
                                <legend>"Repository source"</legend>
                                <div class="field">
                                    <label for="mode">"Input method"</label>
                                    <select id="mode" name="mode" aria-describedby="mode-help">
                                        <option value="local_path">"Server-local path"</option>
                                        <option value="git_url">"Git repository URL"</option>
                                        <option value="upload">"Uploaded repository archive"</option>
                                        <option value="forgejo">"Forgejo repository"</option>
                                        <option value="github">"GitHub repository"</option>
                                    </select>
                                    <small class="help" id="mode-help">"Choose how the server obtains the repository. Fields that do not apply may be left blank."</small>
                                </div>
                                <div class="grid">
                                    <div class="field">
                                        <label for="path">"Server-local path"</label>
                                        <input type="text" id="path" name="path" autocomplete="off" aria-describedby="path-help"/>
                                        <small class="help" id="path-help">"Absolute path to a repository inside an allowed server root."</small>
                                    </div>
                                    <div class="field">
                                        <label for="url">"Git URL (https)"</label>
                                        <input type="url" id="url" name="url" inputmode="url" autocomplete="off" aria-describedby="url-help"/>
                                        <small class="help" id="url-help">"Public https Git repository. Private and loopback hosts are rejected."</small>
                                    </div>
                                    <div class="field">
                                        <label for="upload">"Upload archive"</label>
                                        <input type="file" id="upload" name="upload" accept=".zip,.tar,.tar.gz,.tgz" aria-describedby="upload-help"/>
                                        <small class="help" id="upload-help">"Zip or tar(.gz) archive of a Git repository, extracted safely."</small>
                                    </div>
                                    <div class="field">
                                        <label for="forgejo-repo">"Forgejo repository (owner/name)"</label>
                                        <input type="text" id="forgejo-repo" name="forgejo_repo" placeholder="owner/name" aria-describedby="forgejo-help"/>
                                        <small class="help" id="forgejo-help">"Cloned server-side with the configured Forgejo token. A repository list is available at /ui/integrations/forgejo."</small>
                                    </div>
                                    <div class="field">
                                        <label for="github-repo">"GitHub repository (owner/name)"</label>
                                        <input type="text" id="github-repo" name="github_repo" placeholder="owner/name" aria-describedby="github-help"/>
                                        <small class="help" id="github-help">"Cloned server-side. A repository list is available at /ui/integrations/github."</small>
                                    </div>
                                    <div class="field">
                                        <label for="git-ref">"Branch or tag (optional)"</label>
                                        <input type="text" id="git-ref" name="git_ref" autocomplete="off" aria-describedby="git-ref-help"/>
                                        <small class="help" id="git-ref-help">"Clone a specific branch or tag; leave blank for the default branch."</small>
                                    </div>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Output"</legend>
                                <div class="grid">
                                    <div class="field">
                                        <label for="format">"Archive format"</label>
                                        <select id="format" name="format">{formats.iter().map(|f| option(&default_format, f)).collect_view()}</select>
                                    </div>
                                    <div class="field">
                                        <label for="compression">"Compression"</label>
                                        <select id="compression" name="compression">{compressions.iter().map(|c| option(&default_compression, c)).collect_view()}</select>
                                    </div>
                                    <div class="field">
                                        <label for="report">"Report format"</label>
                                        <select id="report" name="report">{reports.iter().map(|r| option(&default_report, r)).collect_view()}</select>
                                    </div>
                                    <div class="field">
                                        <label for="output_name">"Output filename (optional)"</label>
                                        <input type="text" id="output_name" name="output_name" autocomplete="off"/>
                                    </div>
                                    <div class="field">
                                        <label for="max_file_size">"Maximum file size (bytes)"</label>
                                        <input type="number" id="max_file_size" name="max_file_size" min="0" value=defaults.max_file_size.to_string()/>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="include_untracked" name="include_untracked" value="1"/>
                                        <label for="include_untracked">"Include untracked files"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="timestamp_name" name="timestamp_name" value="1" checked=defaults.timestamp_name/>
                                        <label for="timestamp_name">"Timestamp output filename"</label>
                                    </div>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Filters"</legend>
                                <div class="grid">
                                    <div class="field">
                                        <label for="includes">"Include globs (one per line)"</label>
                                        <textarea id="includes" name="includes" rows="3"></textarea>
                                    </div>
                                    <div class="field">
                                        <label for="excludes">"Exclude globs (one per line)"</label>
                                        <textarea id="excludes" name="excludes" rows="3"></textarea>
                                    </div>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Redaction and safety"</legend>
                                <div class="grid">
                                    <div class="field inline">
                                        <input type="checkbox" id="redact" name="redact" value="1" checked=defaults.redact/>
                                        <label for="redact">"Redact detected secrets"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="fail_on_secret" name="fail_on_secret" value="1"/>
                                        <label for="fail_on_secret">"Fail when secrets are detected"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="dry_run" name="dry_run" value="1"/>
                                        <label for="dry_run">"Dry run (no archive)"</label>
                                    </div>
                                </div>
                            </fieldset>
                            <fieldset>
                                <legend>"Password (ZIP AES only)"</legend>
                                <div class="grid">
                                    <div class="field">
                                        <label for="password">"Password"</label>
                                        <input type="password" id="password" name="password" autocomplete="new-password" aria-describedby="password-help"/>
                                        <small class="help" id="password-help">"Never stored or logged. Only valid with the zip format."</small>
                                    </div>
                                    <div class="field">
                                        <label for="password_min_length">"Minimum length"</label>
                                        <input type="number" id="password_min_length" name="password_min_length" min="1" max="256" value="8"/>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_uppercase" name="password_require_uppercase" value="1" checked/>
                                        <label for="password_require_uppercase">"Require uppercase"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_lowercase" name="password_require_lowercase" value="1" checked/>
                                        <label for="password_require_lowercase">"Require lowercase"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_number" name="password_require_number" value="1" checked/>
                                        <label for="password_require_number">"Require number"</label>
                                    </div>
                                    <div class="field inline">
                                        <input type="checkbox" id="password_require_special" name="password_require_special" value="1" checked/>
                                        <label for="password_require_special">"Require special character"</label>
                                    </div>
                                </div>
                            </fieldset>
                            <button type="submit">"Start sanitizing"</button>
                        </form>
                    </section>
                    <section aria-labelledby="help-heading">
                        <h2 id="help-heading">"Option reference"</h2>
                        {help_groups.into_iter().map(|group| view! {
                            <h3>{group.heading}</h3>
                            <dl>
                                {group.entries.into_iter().map(|entry| view! {
                                    <dt>{entry.field}</dt><dd class="help">{entry.text}</dd>
                                }).collect_view()}
                            </dl>
                        }).collect_view()}
                    </section>
                </main>
                <footer>
                    <p class="help">"Rustrepo-sanitizer reuses one sanitizer core across the CLI, desktop GUI, and this web UI."</p>
                </footer>
            </body>
        </html>
    }
}

fn render_job(job: &JobView) -> AnyView {
    let id = job.id.clone();
    let download = format!("/ui/jobs/{id}/download");
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
            <p class="status" data-kind=kind role="status" aria-live="polite">{text}</p>
            {(!terminal).then(|| view! {
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
