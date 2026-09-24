//! Regression tests for the unified `Rustrepo-sanitizer` executable.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
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

/// Kills the child on drop so a failing assertion cannot leak a server process.
struct Guard(Child);

impl Guard {
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.0.try_wait()
    }
    fn pid(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn health_ok(port: u16) -> bool {
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
        let _ = stream
            .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let mut buffer = String::new();
        let _ = stream.read_to_string(&mut buffer);
        return buffer.contains("200") && buffer.contains("\"version\"");
    }
    false
}

fn wait_healthy(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if health_ok(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Spawns `--web` on a free port, retrying if the port was taken in the race
/// between `free_port()` and the child binding it.
fn spawn_web_with_retry(root: &Path) -> Option<(Guard, u16, std::path::PathBuf)> {
    for attempt in 0..5 {
        let port = free_port();
        let log = root.join(format!("web-{attempt}.log"));
        let stderr = std::fs::File::create(&log).unwrap();
        let child = Command::new(BIN)
            .args([
                "--web",
                "--web-bind",
                &format!("127.0.0.1:{port}"),
                "--web-root",
                root.to_str().unwrap(),
            ])
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .expect("spawn unified web mode");
        let guard = Guard(child);
        if wait_healthy(port, Duration::from_secs(25)) {
            return Some((guard, port, log));
        }
        // Port may have been taken; drop (kills) and retry.
        drop(guard);
    }
    None
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
    // Launch flags are documented even alongside a subcommand listing.
    let (code, stdout, _) = run(&["--gui", "--web", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--gui") && stdout.contains("--web"));
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
fn launch_flags_cannot_be_combined_with_a_subcommand() {
    for args in [["--gui", "sanitize"], ["--web", "sanitize"]] {
        let (code, _, stderr) = run(&args);
        assert_eq!(code, 2, "{args:?} must be rejected");
        assert!(stderr.contains("subcommand"), "{stderr}");
    }
}

#[test]
fn unavailable_modes_are_reported_when_the_feature_is_off() {
    if !cfg!(feature = "gui") {
        let (code, _, stderr) = run(&["--gui"]);
        assert_eq!(code, 2);
        assert!(stderr.contains("no GUI support"), "{stderr}");
    }
    if !cfg!(feature = "web") {
        let (code, _, stderr) = run(&["--web"]);
        assert_eq!(code, 2);
        assert!(stderr.contains("no web support"), "{stderr}");
    }
}

#[test]
fn launcher_composes_gui_and_web_in_one_process() {
    // Static guard: the GUI+Web path exists and never spawns a second binary.
    let main = std::fs::read_to_string(Path::new(ROOT).join("src/main.rs")).unwrap();
    assert!(main.contains("run_gui_and_web"));
    assert!(main.contains("spawn_web"));
    assert!(!main.contains("Command::new"));
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
    let root = tempfile::tempdir().unwrap();
    let (mut guard, port, log) = spawn_web_with_retry(root.path())
        .unwrap_or_else(|| panic!("web mode never became healthy"));
    let _ = port;

    // SIGTERM must trigger a graceful shutdown with a success exit.
    unsafe {
        libc::kill(guard.pid() as i32, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut status = None;
    while Instant::now() < deadline {
        match guard.try_wait() {
            Ok(Some(s)) => {
                status = Some(s);
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(err) => panic!("waiting for the server failed: {err}"),
        }
    }
    let status = status.unwrap_or_else(|| {
        panic!(
            "web mode did not exit within the timeout; log:\n{}",
            std::fs::read_to_string(&log).unwrap_or_default()
        )
    });
    assert!(status.success(), "web mode did not exit cleanly: {status}");
}
