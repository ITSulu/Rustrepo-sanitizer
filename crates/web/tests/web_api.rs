//! End-to-end tests for the web server: auth boundaries, the five input modes,
//! validation, downloads, mocked integrations, and the SSR UI.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::future::BoxFuture;
use http_body_util::BodyExt;
use itsulu_repo_sanitizer_web::acquire::{CloneRunner, HostResolver};
use itsulu_repo_sanitizer_web::integrations::{Integrations, IntegrationsConfig};
use itsulu_repo_sanitizer_web::routes::build_router;
use itsulu_repo_sanitizer_web::state::AppState;
use itsulu_repo_sanitizer_web::uploads::UploadStore;
use itsulu_repo_sanitizer_web::workspace::{Limits, WorkspaceManager};
use tower::ServiceExt;

fn init_executor() {
    itsulu_repo_sanitizer_web::init_executor();
}

struct FakeRunner;
impl CloneRunner for FakeRunner {
    fn clone(
        &self,
        _argv: Vec<String>,
        _env: Vec<(String, String)>,
        _dest: PathBuf,
    ) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

struct PublicResolver;
impl HostResolver for PublicResolver {
    fn resolve(&self, _host: String) -> BoxFuture<'static, Result<Vec<IpAddr>, String>> {
        Box::pin(async { Ok(vec!["1.1.1.1".parse().unwrap()]) })
    }
}

fn test_state(
    root: &std::path::Path,
    token: Option<&str>,
    integrations: IntegrationsConfig,
) -> Arc<AppState> {
    let limits = Limits::default();
    let workspaces = WorkspaceManager::new(root.join("workspaces"), limits.clone()).unwrap();
    let uploads = Arc::new(UploadStore::new(root.join("uploads")).unwrap());
    Arc::new(AppState::new(
        workspaces,
        uploads,
        Arc::new(Integrations::new(integrations)),
        token.map(str::to_owned),
        vec![root.to_path_buf()],
        limits,
        Arc::new(FakeRunner),
        Arc::new(PublicResolver),
    ))
}

fn git_repo(root: &std::path::Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "# Demo\n").unwrap();
    std::fs::write(repo.join("config.txt"), "password: super-secret-value\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "initial"]);
    repo
}

fn json_request(
    method: &str,
    uri: &str,
    body: serde_json::Value,
    token: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn api_requires_the_bearer_token_when_configured() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), Some("s3cret"), IntegrationsConfig::default());
    let app = build_router(state);

    let unauthorized = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/health",
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let wrong = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/health",
            serde_json::Value::Null,
            Some("nope"),
        ))
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let authorized = app
        .oneshot(json_request(
            "GET",
            "/api/health",
            serde_json::Value::Null,
            Some("s3cret"),
        ))
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}

fn form_request(uri: &str, fields: &[(&str, &str)]) -> Request<Body> {
    let boundary = "FORM";
    let mut body = String::new();
    for (name, value) in fields {
        body.push_str(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        ));
    }
    body.push_str(&format!("--{boundary}--\r\n"));
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

async fn wait_for_job(state: &Arc<AppState>, id: &str) -> serde_json::Value {
    let mut status = serde_json::Value::Null;
    for _ in 0..300 {
        let response = build_router(state.clone())
            .oneshot(json_request(
                "GET",
                &format!("/api/jobs/{id}"),
                serde_json::Value::Null,
                None,
            ))
            .await
            .unwrap();
        status = body_json(response).await;
        if status["status"]["state"]
            .as_str()
            .is_some_and(|s| matches!(s, "completed" | "failed" | "cancelled"))
        {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    status
}

#[tokio::test]
async fn ssr_form_submission_creates_and_completes_a_job() {
    let root = tempfile::tempdir().unwrap();
    let repo = git_repo(root.path());
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    init_executor();
    let path = repo.to_string_lossy().into_owned();
    let response = build_router(state.clone())
        .oneshot(form_request(
            "/ui/jobs",
            &[
                ("mode", "local_path"),
                ("path", &path),
                ("format", "tar"),
                ("compression", "zstd"),
                ("report", "markdown"),
                ("timestamp_name", "1"),
                ("redact", "1"),
            ],
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(location.starts_with("/ui/jobs/"), "{location}");
    let id = location.trim_start_matches("/ui/jobs/").to_owned();
    let status = wait_for_job(&state, &id).await;
    assert_eq!(status["status"]["state"], "completed", "{status}");
}

#[tokio::test]
async fn ssr_validation_error_re_renders_alert_and_echoes_values() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    init_executor();
    let response = build_router(state.clone())
        .oneshot(form_request(
            "/ui/jobs",
            &[("mode", "forgejo"), ("forgejo_repo", "../etc")],
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
        .into_owned();
    assert!(html.contains("id=\"form-error\""), "{html}");
    assert!(html.contains("role=\"alert\""));
    // The submitted value is echoed back.
    assert!(
        html.contains("value=\"../etc\""),
        "value not echoed: {html}"
    );
}

#[tokio::test]
async fn dry_run_completes_without_an_archive() {
    let root = tempfile::tempdir().unwrap();
    let repo = git_repo(root.path());
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    let create = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "local_path", "path": repo.to_string_lossy()},
                "options": {"dry_run": true}
            }),
            None,
        ))
        .await
        .unwrap();
    let id = body_json(create).await["id"].as_str().unwrap().to_owned();
    let status = wait_for_job(&state, &id).await;
    assert_eq!(status["status"]["state"], "completed", "{status}");
    assert_eq!(status["status"]["dry_run"], true);
    assert!(status["status"]["archive"].is_null());

    let download = build_router(state.clone())
        .oneshot(json_request(
            "GET",
            &format!("/api/jobs/{id}/download"),
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn fail_on_secret_fails_the_job() {
    let root = tempfile::tempdir().unwrap();
    let repo = git_repo(root.path());
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    let create = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "local_path", "path": repo.to_string_lossy()},
                "options": {"fail_on_secret": true}
            }),
            None,
        ))
        .await
        .unwrap();
    let id = body_json(create).await["id"].as_str().unwrap().to_owned();
    let status = wait_for_job(&state, &id).await;
    assert_eq!(status["status"]["state"], "failed", "{status}");
}

#[tokio::test]
async fn cancel_endpoint_reports_unknown_and_known_jobs() {
    let root = tempfile::tempdir().unwrap();
    let repo = git_repo(root.path());
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    let create = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "local_path", "path": repo.to_string_lossy()},
                "options": {}
            }),
            None,
        ))
        .await
        .unwrap();
    let id = body_json(create).await["id"].as_str().unwrap().to_owned();
    let known = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            &format!("/api/jobs/{id}/cancel"),
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(known.status(), StatusCode::ACCEPTED);
    let unknown = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            "/api/jobs/does-not-exist/cancel",
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn integration_pages_report_unconfigured_state() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    for path in ["/ui/integrations/forgejo", "/ui/integrations/github"] {
        let response = build_router(state.clone())
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html =
            String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
                .into_owned();
        assert!(html.contains("not configured"), "{path}: {html}");
        assert!(html.contains("role=\"alert\""));
    }
}

#[tokio::test]
async fn all_api_routes_are_protected_by_the_token() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), Some("tok"), IntegrationsConfig::default());
    for (method, path) in [
        ("GET", "/api/capabilities"),
        ("GET", "/api/integrations"),
        ("POST", "/api/jobs"),
        ("GET", "/api/jobs/x"),
    ] {
        let response = build_router(state.clone())
            .oneshot(json_request(method, path, serde_json::json!({}), None))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must require a token"
        );
    }
}

fn multipart_request(uri: &str, field: &str, value: &str) -> Request<Body> {
    let boundary = "XBOUND";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"\r\n\r\n{value}\r\n--{boundary}--\r\n"
    );
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn ui_routes_require_authentication_when_a_token_is_configured() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), Some("s3cret"), IntegrationsConfig::default());
    init_executor();

    // The UI root redirects an unauthenticated browser to the login page.
    let index = build_router(state.clone())
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(index.status(), StatusCode::SEE_OTHER);
    assert_eq!(index.headers().get("location").unwrap(), "/ui/login");

    // Download routes are not reachable without auth.
    let download = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/ui/jobs/x/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::SEE_OTHER);

    // A wrong login token is rejected.
    let bad = build_router(state.clone())
        .oneshot(multipart_request("/ui/login", "token", "wrong"))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

    // A correct login sets an HttpOnly session cookie and redirects home.
    let good = build_router(state.clone())
        .oneshot(multipart_request("/ui/login", "token", "s3cret"))
        .await
        .unwrap();
    assert_eq!(good.status(), StatusCode::SEE_OTHER);
    let cookie = good
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));

    // The session cookie authorizes the UI.
    let authorized = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", cookie.split(';').next().unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}

#[tokio::test]
async fn output_name_is_validated() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    for bad in ["../escape.tar", "a/b.tar", "bad\r\nInjected: 1"] {
        let response = build_router(state.clone())
            .oneshot(json_request(
                "POST",
                "/api/jobs",
                serde_json::json!({
                    "input": {"mode": "local_path", "path": root.path().to_string_lossy()},
                    "options": {"output_name": bad}
                }),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "accepted {bad:?}"
        );
    }
}

#[tokio::test]
async fn capabilities_expose_shared_metadata() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    let response = build_router(state)
        .oneshot(json_request(
            "GET",
            "/api/capabilities",
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value = body_json(response).await;
    assert!(value["capabilities"].as_array().unwrap().len() > 5);
    assert!(value["formats"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "zip"));
    assert!(value["help"]["groups"].as_array().unwrap().len() >= 5);
}

#[tokio::test]
async fn validation_rejects_invalid_options() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    let response = build_router(state)
        .oneshot(json_request(
            "POST",
            "/api/validate",
            serde_json::json!({
                "input": {"mode": "git_url", "url": "https://git.example.com/org/repo.git"},
                "options": {"format": "zip", "compression": "lz4"}
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let value = body_json(response).await;
    assert_eq!(value["valid"], false);
}

#[tokio::test]
async fn end_to_end_local_repository_sanitize_and_download() {
    let root = tempfile::tempdir().unwrap();
    let repo = git_repo(root.path());
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    let app = build_router(state.clone());

    let create = app
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "local_path", "path": repo.to_string_lossy()},
                "options": {"format": "tar", "compression": "zstd", "report": "markdown"}
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::ACCEPTED);
    let id = body_json(create).await["id"].as_str().unwrap().to_owned();

    // Poll to completion.
    let mut status = serde_json::Value::Null;
    for _ in 0..200 {
        let response = build_router(state.clone())
            .oneshot(json_request(
                "GET",
                &format!("/api/jobs/{id}"),
                serde_json::Value::Null,
                None,
            ))
            .await
            .unwrap();
        status = body_json(response).await;
        let state_name = status["status"]["state"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if matches!(state_name.as_str(), "completed" | "failed" | "cancelled") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        status["status"]["state"], "completed",
        "job did not complete: {status}"
    );
    assert!(status["status"]["redactions"].as_u64().unwrap() >= 1);

    // Download the archive and check the zstd magic bytes.
    let download = build_router(state.clone())
        .oneshot(json_request(
            "GET",
            &format!("/api/jobs/{id}/download"),
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    let headers = download.headers().clone();
    assert!(headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("attachment"));
    let bytes = download.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.len() > 32);
    assert_eq!(
        &bytes[..4],
        &[0x28, 0xb5, 0x2f, 0xfd],
        "expected zstd stream"
    );

    // Reports extracted from the archive are downloadable.
    let report = build_router(state.clone())
        .oneshot(json_request(
            "GET",
            &format!("/api/jobs/{id}/reports/SANITIZATION-REPORT.md"),
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(report.status(), StatusCode::OK);
    let report_bytes = report.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&report_bytes).contains("Sanitization Report"));

    // The SSR job page announces status accessibly and links the download.
    let page = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/ui/jobs/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html =
        String::from_utf8_lossy(&page.into_body().collect().await.unwrap().to_bytes()).into_owned();
    assert!(html.contains("aria-live=\"polite\""), "html: {html}");
    assert!(html.contains("role=\"status\""));
    assert!(html.contains(&format!("/ui/jobs/{id}/download")));
    assert!(
        !html.contains("Cancel"),
        "completed jobs must not offer Cancel"
    );
}

#[tokio::test]
async fn upload_mode_extracts_an_archive_and_sanitizes() {
    let root = tempfile::tempdir().unwrap();
    let repo = git_repo(root.path());

    // Build a tar.gz of the repository (without the .git directory is fine for
    // a local test, but include .git so it is recognised as a repository).
    let archive_path = root.path().join("upload.tar.gz");
    {
        let file = std::fs::File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut tar = tar::Builder::new(encoder);
        tar.append_dir_all("repo", &repo).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
    }
    let archive_bytes = std::fs::read(&archive_path).unwrap();

    let state = test_state(root.path(), None, IntegrationsConfig::default());

    // Upload through the multipart API.
    let boundary = "XBOUNDARYX";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"upload.tar.gz\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/gzip\r\n\r\n");
    body.extend_from_slice(&archive_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let upload = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/uploads")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);
    let upload_id = body_json(upload).await["upload_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let create = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "upload", "upload_id": upload_id},
                "options": {}
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::ACCEPTED);
    let id = body_json(create).await["id"].as_str().unwrap().to_owned();

    for _ in 0..200 {
        let response = build_router(state.clone())
            .oneshot(json_request(
                "GET",
                &format!("/api/jobs/{id}"),
                serde_json::Value::Null,
                None,
            ))
            .await
            .unwrap();
        let status = body_json(response).await;
        match status["status"]["state"].as_str().unwrap_or_default() {
            "completed" => return,
            "failed" => panic!("upload job failed: {status}"),
            _ => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
    panic!("upload job did not finish");
}

#[tokio::test]
async fn url_and_integration_modes_construct_inputs_server_side() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    // URL mode validates and resolves before cloning (fake runner succeeds).
    let response = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "git_url", "url": "https://git.example.com/org/repo.git"},
                "options": {}
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    // Invalid private URL is rejected by validation.
    let bad = build_router(state.clone())
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "git_url", "url": "https://127.0.0.1/repo.git"},
                "options": {}
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    // GitHub mode requires owner/name.
    let bad_github = build_router(state)
        .oneshot(json_request(
            "POST",
            "/api/jobs",
            serde_json::json!({
                "input": {"mode": "github", "owner": "../etc", "repo": "passwd"},
                "options": {}
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(bad_github.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn integration_endpoints_proxy_the_mock_servers() {
    use axum::routing::get;
    use axum::{Json, Router};

    let forgejo = Router::new().route(
        "/api/v1/user/repos",
        get(|| async {
            Json(serde_json::json!([
                {"full_name":"itsulu/repo","clone_url":"https://git.example.com/itsulu/repo.git","default_branch":"main","private":true}
            ]))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, forgejo).await.unwrap() });

    let root = tempfile::tempdir().unwrap();
    let config = IntegrationsConfig {
        forgejo_base: Some(url::Url::parse(&format!("http://{addr}")).unwrap()),
        forgejo_token: Some("tok".into()),
        ..IntegrationsConfig::default()
    };
    let state = test_state(root.path(), None, config);
    let response = build_router(state.clone())
        .oneshot(json_request(
            "GET",
            "/api/integrations/forgejo/repos",
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let repos = body_json(response).await;
    assert_eq!(repos[0]["full_name"], "itsulu/repo");

    // Unconfigured GitHub returns 503.
    let github = build_router(state)
        .oneshot(json_request(
            "GET",
            "/api/integrations/github/repos",
            serde_json::Value::Null,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(github.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn ssr_ui_renders_accessible_controls() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    init_executor();
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes);
    assert!(
        html.contains("<!DOCTYPE html>") || html.contains("<html"),
        "html: {html}"
    );
    assert!(html.contains("lang=\"en\""));
    assert!(html.contains("Skip to main content"));
    assert!(html.contains("<label for=\"mode\""));
    assert!(html.contains("aria-describedby"));
    assert!(html.contains("action=\"/ui/jobs\""));
    assert!(html.contains("enctype=\"multipart/form-data\""));
    // Every option from the shared registry must be offered.
    assert!(html.contains("id=\"format\""));
    assert!(html.contains("id=\"password\""));
    // Responsive and keyboard-visible focus styling.
    assert!(html.contains("@media"));
    assert!(html.contains(":focus-visible"));
}

#[tokio::test]
async fn ssr_ui_covers_every_cli_and_gui_capability() {
    let root = tempfile::tempdir().unwrap();
    let state = test_state(root.path(), None, IntegrationsConfig::default());
    init_executor();
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
        .into_owned();
    for control in [
        // repository inputs (all five modes)
        "id=\"mode\"",
        "id=\"path\"",
        "id=\"url\"",
        "id=\"upload\"",
        "id=\"forgejo-repo\"",
        "id=\"github-repo\"",
        "id=\"git-ref\"",
        // output
        "id=\"format\"",
        "id=\"compression\"",
        "id=\"report\"",
        "id=\"output_name\"",
        "id=\"max_file_size\"",
        "id=\"timestamp_name\"",
        "id=\"include_untracked\"",
        // filters
        "id=\"includes\"",
        "id=\"excludes\"",
        // redaction
        "id=\"redact\"",
        "id=\"fail_on_secret\"",
        "id=\"dry_run\"",
        // security
        "id=\"password\"",
        "id=\"password_min_length\"",
        "id=\"password_require_uppercase\"",
        "id=\"password_require_lowercase\"",
        "id=\"password_require_number\"",
        "id=\"password_require_special\"",
    ] {
        assert!(html.contains(control), "web UI is missing {control}");
    }
    // Every archive format and compression from the shared registry is offered.
    for format in [
        "value=\"tar\"",
        "value=\"zip\"",
        "value=\"7z\"",
        "value=\"none\"",
    ] {
        assert!(html.contains(format), "format option missing: {format}");
    }
    for compression in ["value=\"zstd\"", "value=\"gzip\""] {
        assert!(
            html.contains(compression),
            "compression option missing: {compression}"
        );
    }
}
