//! Regression tests for the unified `Rustrepo-sanitizer` executable.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_Rustrepo-sanitizer");
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(BIN).args(args).output().expect("binary runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn only_one_production_executable_is_declared() {
    let manifest = std::fs::read_to_string(Path::new(ROOT).join("Cargo.toml")).unwrap();
    assert_eq!(
        manifest.matches("[[bin]]").count(),
        1,
        "the package must declare exactly one binary"
    );
    assert!(manifest.contains("name = \"Rustrepo-sanitizer\""));
    assert!(!manifest.contains("rustrepo-sanitizer-web"));
    assert!(!manifest.contains("rustrepo-sanitizer-gui"));
}

#[test]
fn old_web_crate_and_artifact_are_absent() {
    assert!(
        !Path::new(ROOT).join("crates/web").exists(),
        "the separate web crate must be removed"
    );
    let workflow =
        std::fs::read_to_string(Path::new(ROOT).join(".forgejo/workflows/release-build.yml"))
            .unwrap();
    assert!(!workflow.contains("rustrepo-sanitizer-web"));
    assert!(!workflow.contains("web-x86_64"));
    let docs = std::fs::read_to_string(Path::new(ROOT).join("docs/release-process.md")).unwrap();
    assert!(!docs.contains("rustrepo-sanitizer-web"));
}

#[test]
fn version_uses_the_product_name() {
    let (code, stdout, _) = run(&["--version"]);
    assert_eq!(code, 0);
    assert!(
        stdout.starts_with("Rustrepo-sanitizer "),
        "version output: {stdout}"
    );
}

#[test]
fn help_aliases_document_the_launch_modes() {
    for alias in ["--help", "-h", "-help"] {
        let (code, stdout, _) = run(&[alias]);
        assert_eq!(code, 0, "`{alias}` must exit 0");
        assert!(
            stdout.contains("Launch"),
            "`{alias}` must show the Launch group"
        );
        assert!(stdout.contains("--gui"));
        assert!(stdout.contains("--web"));
        assert!(stdout.contains("Web server"));
    }
    // The sanitize subcommand help still works and keeps its groups.
    let (code, stdout, _) = run(&["sanitize", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Archive / compression"));
}

#[test]
fn web_options_require_the_web_flag() {
    let (code, _, stderr) = run(&["--web-bind", "127.0.0.1:9000"]);
    assert_ne!(code, 0, "web options without --web must fail");
    assert!(
        stderr.contains("--web"),
        "error should mention --web: {stderr}"
    );
}

#[test]
fn cli_sanitize_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        assert!(Command::new("git")
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "T"]);
    std::fs::write(repo.join("a.txt"), "password: secret-value\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "init"]);

    let (code, _, stderr) = run(&["sanitize", repo.to_str().unwrap(), "--dry-run", "--quiet"]);
    assert_eq!(code, 0, "dry-run sanitize failed: {stderr}");

    let (code, stdout, _) = run(&["list-formats"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("tar"));
}

#[test]
fn web_mode_serves_health_and_shuts_down_cleanly() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(BIN)
        .args([
            "--web",
            "--web-bind",
            &format!("127.0.0.1:{port}"),
            "--web-root",
            root.path().to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn unified web mode");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut healthy = false;
    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.write_all(
                b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            );
            let mut buffer = String::new();
            let _ = stream.read_to_string(&mut buffer);
            if buffer.contains("200") && buffer.contains("\"version\"") {
                healthy = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(healthy, "unified --web mode did not serve /api/health");

    // SIGTERM must trigger a graceful shutdown with a success exit.
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let status = child.wait().expect("child exits");
    assert!(status.success(), "web mode did not exit cleanly: {status}");
}
