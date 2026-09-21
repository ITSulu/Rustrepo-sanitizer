//! Axum routes: a token-protected JSON API and the human-facing SSR UI.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

use crate::dto::{self, InputSpec, OptionsDto};
use crate::integrations::{validate_component, IntegrationsError};
use crate::jobs::{self, JobView};
use crate::security::safe_output_name;
use crate::state::AppState;
use crate::ui::App;

#[derive(Deserialize)]
pub struct CreateJobRequest {
    pub input: InputSpec,
    #[serde(default)]
    pub options: OptionsDto,
}

#[derive(Serialize)]
struct ValidationResponse {
    valid: bool,
    error: Option<String>,
}

/// Builds the complete application router.
pub fn build_router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/capabilities", get(capabilities))
        .route("/api/validate", post(validate))
        .route("/api/uploads", post(upload))
        .route("/api/integrations", get(integrations_status))
        .route("/api/integrations/forgejo/repos", get(forgejo_repos))
        .route("/api/integrations/github/repos", get(github_repos))
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/{id}", get(get_job))
        .route("/api/jobs/{id}/cancel", post(cancel_job))
        .route("/api/jobs/{id}/download", get(download_job))
        .route("/api/jobs/{id}/reports/{name}", get(download_report))
        .layer(middleware::from_fn_with_state(state.clone(), require_token));

    let ui = Router::new()
        .route("/", get(ui_index))
        .route("/ui/jobs", post(ui_create_job))
        .route("/ui/jobs/{id}", get(ui_job))
        .route("/ui/jobs/{id}/cancel", post(ui_cancel_job))
        .route("/ui/jobs/{id}/download", get(ui_download_job))
        .route("/ui/jobs/{id}/reports/{name}", get(ui_download_report))
        .route("/ui/integrations/forgejo", get(ui_forgejo_repos))
        .route("/ui/integrations/github", get(ui_github_repos))
        .route("/ui/health", get(ui_health));

    Router::new().merge(api).merge(ui).with_state(state)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn require_token(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if let Some(token) = state.token.as_deref() {
        let provided = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        let ok = provided.is_some_and(|provided| constant_time_eq(provided, token));
        if !ok {
            return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
    }
    next.run(req).await
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}))
}

async fn capabilities() -> Json<dto::CapabilitiesView> {
    Json(dto::capabilities_view())
}

async fn validate(Json(request): Json<CreateJobRequest>) -> Response {
    match validate_request(&request) {
        Ok(()) => Json(ValidationResponse {
            valid: true,
            error: None,
        })
        .into_response(),
        Err(message) => (
            StatusCode::BAD_REQUEST,
            Json(ValidationResponse {
                valid: false,
                error: Some(message),
            }),
        )
            .into_response(),
    }
}

fn validate_request(request: &CreateJobRequest) -> Result<(), String> {
    match &request.input {
        InputSpec::LocalPath { path } if path.trim().is_empty() => {
            return Err("repository path must not be empty".into())
        }
        InputSpec::GitUrl { url } => {
            crate::security::validate_git_url(url).map_err(|err| err.to_string())?;
        }
        InputSpec::Upload { upload_id } if upload_id.trim().is_empty() => {
            return Err("upload id must not be empty".into())
        }
        InputSpec::Forgejo { owner, repo, .. } | InputSpec::GitHub { owner, repo, .. } => {
            validate_component(owner).map_err(|err| err.to_string())?;
            validate_component(repo).map_err(|err| err.to_string())?;
        }
        _ => {}
    }
    request
        .options
        .to_config(
            std::path::PathBuf::from("/validation/repository"),
            std::path::PathBuf::from("/validation/output"),
        )
        .map(|_| ())
}

async fn upload(State(state): State<Arc<AppState>>, multipart: Multipart) -> Response {
    match read_upload(&state, multipart).await {
        Ok(Some(entry)) => Json(serde_json::json!({
            "upload_id": entry.id,
            "name": entry.name,
            "bytes": entry.bytes,
        }))
        .into_response(),
        Ok(None) => (StatusCode::BAD_REQUEST, "no file field named `file`").into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    }
}

async fn read_upload(
    state: &AppState,
    mut multipart: Multipart,
) -> anyhow::Result<Option<crate::uploads::UploadEntry>> {
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("file") {
            let name = field.file_name().unwrap_or("upload.zip").to_owned();
            let data = field.bytes().await?;
            let entry = state.uploads.put(&name, &data[..])?;
            return Ok(Some(entry));
        }
    }
    Ok(None)
}

async fn integrations_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "forgejo_configured": state.integrations.forgejo_configured(),
        "github_configured": state.integrations.github_configured(),
    }))
}

async fn forgejo_repos(State(state): State<Arc<AppState>>) -> Response {
    match state.integrations.list_forgejo_repos().await {
        Ok(repos) => Json(repos).into_response(),
        Err(IntegrationsError::NotConfigured(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "integration not configured",
        )
            .into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}

async fn github_repos(State(state): State<Arc<AppState>>) -> Response {
    match state.integrations.list_github_repos().await {
        Ok(repos) => Json(repos).into_response(),
        Err(IntegrationsError::NotConfigured(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "integration not configured",
        )
            .into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}

async fn create_job(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateJobRequest>,
) -> Response {
    if let Err(message) = validate_request(&request) {
        return (StatusCode::BAD_REQUEST, message).into_response();
    }
    match jobs::start_job(state, request.input, request.options).await {
        Ok(id) => (StatusCode::ACCEPTED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    }
}

async fn get_job(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.jobs.get(&id) {
        Some(job) => Json(job).into_response(),
        None => (StatusCode::NOT_FOUND, "job not found").into_response(),
    }
}

async fn cancel_job(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    if state.jobs.cancel(&id) {
        StatusCode::ACCEPTED.into_response()
    } else {
        (StatusCode::NOT_FOUND, "job not found").into_response()
    }
}

fn artifact_response(path: &std::path::Path, name: &str) -> Response {
    let file_name = safe_output_name(name).unwrap_or_else(|_| "sanitized-output".to_owned());
    match std::fs::File::open(path) {
        Ok(file) => {
            let stream = ReaderStream::new(tokio::fs::File::from_std(file));
            let body = Body::from_stream(stream);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{file_name}\""),
                )
                .body(body)
                .unwrap()
        }
        Err(_) => (StatusCode::NOT_FOUND, "artifact not found").into_response(),
    }
}

async fn download_job(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.jobs.artifact(&id, None) {
        Some((path, name)) => artifact_response(&path, &name),
        None => (StatusCode::NOT_FOUND, "artifact not available").into_response(),
    }
}

async fn download_report(
    State(state): State<Arc<AppState>>,
    Path((id, name)): Path<(String, String)>,
) -> Response {
    match state.jobs.artifact(&id, Some(&name)) {
        Some((path, name)) => artifact_response(&path, &name),
        None => (StatusCode::NOT_FOUND, "report not available").into_response(),
    }
}

// ---- SSR UI ----

async fn render_app(
    caps: dto::CapabilitiesView,
    job: Option<JobView>,
    flash: Option<String>,
    error: Option<String>,
    req: Request<Body>,
) -> Response {
    let handler = leptos_axum::render_app_async(move || {
        let caps = caps.clone();
        let job = job.clone();
        let flash = flash.clone();
        let error = error.clone();
        view! { <App caps=caps job=job flash=flash error=error/> }
    });
    handler(req).await
}

async fn ui_index(State(_state): State<Arc<AppState>>, req: Request<Body>) -> Response {
    render_app(dto::capabilities_view(), None, None, None, req).await
}

async fn ui_health() -> &'static str {
    "ok"
}

fn parse_options(fields: &HashMap<String, String>) -> OptionsDto {
    let mut options = OptionsDto::default();
    if let Some(format) = fields.get("format") {
        if let Ok(value) = serde_json::from_value(serde_json::Value::String(format.clone())) {
            options.format = value;
        }
    }
    if let Some(compression) = fields.get("compression") {
        if let Ok(value) = serde_json::from_value(serde_json::Value::String(compression.clone())) {
            options.compression = value;
        }
    }
    if let Some(report) = fields.get("report") {
        if let Ok(value) = serde_json::from_value(serde_json::Value::String(report.clone())) {
            options.report = value;
        }
    }
    options.include_untracked = fields.contains_key("include_untracked");
    options.redact = fields.contains_key("redact");
    options.fail_on_secret = fields.contains_key("fail_on_secret");
    options.dry_run = fields.contains_key("dry_run");
    options.timestamp_name = fields.contains_key("timestamp_name");
    if let Some(size) = fields.get("max_file_size").and_then(|v| v.parse().ok()) {
        options.max_file_size = size;
    }
    options.includes = split_lines(fields.get("includes"));
    options.excludes = split_lines(fields.get("excludes"));
    options.password = fields
        .get("password")
        .filter(|value| !value.is_empty())
        .cloned();
    if let Some(length) = fields
        .get("password_min_length")
        .and_then(|v| v.parse().ok())
    {
        options.password_policy.minimum_length = length;
    }
    options.password_policy.require_uppercase = fields.contains_key("password_require_uppercase");
    options.password_policy.require_lowercase = fields.contains_key("password_require_lowercase");
    options.password_policy.require_number = fields.contains_key("password_require_number");
    options.password_policy.require_special = fields.contains_key("password_require_special");
    options.output_name = fields
        .get("output_name")
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    options
}

fn split_lines(value: Option<&String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_owner_name(value: Option<&String>) -> Option<(String, String)> {
    let value = value?.trim();
    let (owner, repo) = value.split_once('/')?;
    if validate_component(owner).is_err() || validate_component(repo).is_err() {
        return None;
    }
    Some((owner.to_owned(), repo.to_owned()))
}

async fn ui_create_job(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut upload_id: Option<String> = None;
    let mut upload_error: Option<String> = None;
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                let name = field.name().unwrap_or_default().to_owned();
                if name == "upload" {
                    if let Some(file_name) = field.file_name().map(str::to_owned) {
                        if !file_name.is_empty() {
                            match field.bytes().await {
                                Ok(data) => match state.uploads.put(&file_name, &data[..]) {
                                    Ok(entry) => upload_id = Some(entry.id),
                                    Err(err) => upload_error = Some(err.to_string()),
                                },
                                Err(err) => upload_error = Some(err.to_string()),
                            }
                        }
                    }
                } else if let Ok(text) = field.text().await {
                    fields.insert(name, text);
                }
            }
            Ok(None) => break,
            Err(err) => {
                return render_app(
                    dto::capabilities_view(),
                    None,
                    None,
                    Some(format!("could not read the form: {err}")),
                    Request::new(Body::empty()),
                )
                .await
            }
        }
    }

    let mode = fields.get("mode").cloned().unwrap_or_default();
    let git_ref = fields
        .get("git_ref")
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let input = match mode.as_str() {
        "local_path" => Some(InputSpec::LocalPath {
            path: fields.get("path").cloned().unwrap_or_default(),
        }),
        "git_url" => Some(InputSpec::GitUrl {
            url: fields.get("url").cloned().unwrap_or_default(),
        }),
        "upload" => upload_id
            .clone()
            .map(|upload_id| InputSpec::Upload { upload_id }),
        "forgejo" => {
            parse_owner_name(fields.get("forgejo_repo")).map(|(owner, repo)| InputSpec::Forgejo {
                owner,
                repo,
                git_ref: git_ref.clone(),
            })
        }
        "github" => {
            parse_owner_name(fields.get("github_repo")).map(|(owner, repo)| InputSpec::GitHub {
                owner,
                repo,
                git_ref: git_ref.clone(),
            })
        }
        _ => None,
    };

    let Some(input) = input else {
        let message = upload_error
            .unwrap_or_else(|| "the selected input method is missing required fields".to_owned());
        return render_app(
            dto::capabilities_view(),
            None,
            None,
            Some(message),
            Request::new(Body::empty()),
        )
        .await;
    };
    let options = parse_options(&fields);
    match jobs::start_job(state, input, options).await {
        Ok(id) => Redirect::to(&format!("/ui/jobs/{id}")).into_response(),
        Err(err) => {
            render_app(
                dto::capabilities_view(),
                None,
                None,
                Some(err.to_string()),
                Request::new(Body::empty()),
            )
            .await
        }
    }
}

async fn ui_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    req: Request<Body>,
) -> Response {
    match state.jobs.get(&id) {
        Some(job) => render_app(dto::capabilities_view(), Some(job), None, None, req).await,
        None => {
            render_app(
                dto::capabilities_view(),
                None,
                None,
                Some("job not found".to_owned()),
                req,
            )
            .await
        }
    }
}

async fn ui_cancel_job(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    state.jobs.cancel(&id);
    Redirect::to(&format!("/ui/jobs/{id}")).into_response()
}

async fn ui_download_job(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.jobs.artifact(&id, None) {
        Some((path, name)) => artifact_response(&path, &name),
        None => (StatusCode::NOT_FOUND, "artifact not available").into_response(),
    }
}

async fn ui_download_report(
    State(state): State<Arc<AppState>>,
    Path((id, name)): Path<(String, String)>,
) -> Response {
    match state.jobs.artifact(&id, Some(&name)) {
        Some((path, name)) => artifact_response(&path, &name),
        None => (StatusCode::NOT_FOUND, "report not available").into_response(),
    }
}

fn repo_list_page(
    title: &str,
    configured: bool,
    repos: &[crate::integrations::RepoSummary],
) -> Response {
    let mut html = String::from(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>",
    );
    html.push_str(title);
    html.push_str("</title></head><body><main><h1>");
    html.push_str(title);
    html.push_str("</h1>");
    if !configured {
        html.push_str("<p role=\"alert\">This integration is not configured on the server.</p>");
    } else if repos.is_empty() {
        html.push_str(
            "<p role=\"status\">No repositories are accessible with the server token.</p>",
        );
    } else {
        html.push_str("<ul>");
        for repo in repos {
            html.push_str("<li>");
            html.push_str(&escape_html(&repo.full_name));
            html.push_str(if repo.private { " (private)" } else { "" });
            html.push_str("</li>");
        }
        html.push_str("</ul>");
    }
    html.push_str("<p><a href=\"/\">Back to the sanitizer</a></p></main></body></html>");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn ui_forgejo_repos(State(state): State<Arc<AppState>>) -> Response {
    let configured = state.integrations.forgejo_configured();
    let repos = if configured {
        state
            .integrations
            .list_forgejo_repos()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    repo_list_page("Forgejo repositories", configured, &repos)
}

async fn ui_github_repos(State(state): State<Arc<AppState>>) -> Response {
    let configured = state.integrations.github_configured();
    let repos = if configured {
        state
            .integrations
            .list_github_repos()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    repo_list_page("GitHub repositories", configured, &repos)
}

/// Extracts a bearer token for tests and callers.
pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}
