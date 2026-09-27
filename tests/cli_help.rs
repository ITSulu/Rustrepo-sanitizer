//! Regression tests for the grouped CLI help output.
use std::collections::BTreeSet;
use std::process::{Command, Stdio};

use itsulu_repo_sanitizer::help;
use itsulu_repo_sanitizer::size::parse_size as parse_max_file_size;

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_Rustrepo-sanitizer"))
        .args(args)
        .output()
        .expect("the sanitizer binary must run");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const SANITIZE_FLAGS: &[&str] = &[
    "--output",
    "--archive",
    "--compression",
    "--report",
    "--include-untracked",
    "--max-file-size",
    "--exclude",
    "--include",
    "--redact",
    "--no-redact",
    "--fail-on-secret",
    "--dry-run",
    "--timestamp-name",
    "--password-file",
    "--password-stdin",
    "--password-min-length",
    "--password-require-uppercase",
    "--password-require-lowercase",
    "--password-require-number",
    "--password-require-special",
    "--verbose",
    "--quiet",
];

/// Extract every long option token (`--name`) from help text.
fn long_flags(text: &str) -> Vec<String> {
    let mut flags = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'-' && bytes[index + 1] == b'-' {
            let start = index + 2;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_lowercase()
                    || bytes[end] == b'-'
                    || bytes[end].is_ascii_digit())
            {
                end += 1;
            }
            if end > start {
                flags.push(text[start..end].to_owned());
                index = end;
                continue;
            }
        }
        index += 1;
    }
    flags
}

#[test]
fn all_help_aliases_exit_successfully() {
    for alias in ["--help", "-h", "-help"] {
        let (code, stdout, _) = run(&[alias]);
        assert_eq!(code, 0, "`{alias}` must exit 0");
        assert!(stdout.contains("Usage:"), "`{alias}` must print usage");
    }
}

#[test]
fn sanitize_help_alias_exits_successfully() {
    for alias in ["--help", "-h", "-help"] {
        let (code, stdout, _) = run(&["sanitize", alias]);
        assert_eq!(code, 0, "sanitize {alias}");
        assert!(stdout.contains("Usage:"));
        assert!(stdout.contains("Complete CLI example:"));
    }
}

#[test]
fn top_level_help_snapshot() {
    let (code, stdout, _) = run(&["--help"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("top-level-help", stdout);
}

#[test]
fn sanitize_help_snapshot() {
    let (code, stdout, _) = run(&["sanitize", "--help"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("sanitize-help", stdout);
}

#[test]
fn every_public_argument_appears_exactly_once() {
    let (_, stdout, _) = run(&["sanitize", "--help"]);
    let arguments = stdout.split("Complete CLI example:").next().unwrap();
    let flags: Vec<String> = long_flags(arguments)
        .into_iter()
        .filter(|flag| flag != "format" && flag != "help")
        .collect();
    for expected in SANITIZE_FLAGS {
        let name = expected.trim_start_matches("--");
        let count = flags.iter().filter(|flag| flag.as_str() == name).count();
        assert_eq!(count, 1, "`{expected}` must appear exactly once");
    }
}

#[test]
fn no_stale_or_unknown_options_are_documented() {
    let (_, stdout, _) = run(&["sanitize", "--help"]);
    let known: BTreeSet<&str> = SANITIZE_FLAGS
        .iter()
        .map(|flag| flag.trim_start_matches("--"))
        .chain(["help", "format"])
        .collect();
    for flag in long_flags(stdout.split("Complete CLI example:").next().unwrap()) {
        assert!(
            known.contains(flag.as_str()),
            "help documents unknown option `--{flag}`"
        );
    }
}

/// Collapse whitespace so line wrapping in the help output does not hide a
/// description.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn every_argument_has_a_description() {
    let (_, stdout, _) = run(&["sanitize", "--help"]);
    let help_text = normalize(&stdout);
    for description in help::CLI_HELP {
        assert!(
            help_text.contains(&normalize(description)),
            "help is missing the description: {description}"
        );
    }
}

#[test]
fn help_stays_readable_at_normal_terminal_widths() {
    let (_, stdout, _) = run(&["sanitize", "--help"]);
    let widest = stdout.lines().map(str::len).max().unwrap_or(0);
    assert!(
        widest <= 100,
        "help lines must not exceed a normal terminal width (widest was {widest})"
    );
}

#[test]
fn max_file_size_accepts_binary_units() {
    for (input, expected) in [
        ("512", 512u64),
        ("1KiB", 1024),
        ("2MiB", 2 * 1024 * 1024),
        ("1GiB", 1024 * 1024 * 1024),
    ] {
        let parsed = parse_max_file_size(input)
            .unwrap_or_else(|e| panic!("{input}: {e}"))
            .bytes();
        assert_eq!(parsed, expected, "for {input}");
    }
}

#[test]
fn max_file_size_rejects_invalid_units() {
    for bad in ["", "10MB", "abc", "10 MiB extra"] {
        assert!(parse_max_file_size(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn help_documents_binary_size_syntax() {
    let (code, stdout, _) = run(&["sanitize", "--help"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("KiB") && stdout.contains("MiB") && stdout.contains("GiB"),
        "sanitize help must document binary size units"
    );
    let text = normalize(&stdout);
    assert!(
        text.contains("Maximum file size; accepts plain bytes or KiB/MiB/GiB"),
        "help must describe the size syntax"
    );
}

#[test]
fn top_level_help_documents_launch_and_web_groups() {
    let (code, stdout, _) = run(&["--help"]);
    assert_eq!(code, 0);
    let launch = stdout
        .find(help::GROUP_LAUNCH)
        .expect("top-level help must show the Launch group");
    let web = stdout
        .find(help::GROUP_WEB)
        .expect("top-level help must show the Web server group");
    assert!(launch < web, "Launch must precede Web server");
    let text = normalize(&stdout);
    for description in help::LAUNCH_HELP {
        assert!(
            text.contains(&normalize(description)),
            "top-level help is missing: {description}"
        );
    }
}

#[test]
fn bare_help_contains_every_sanitize_group_argument_and_description() {
    for alias in ["--help", "-h"] {
        let (code, stdout, _) = run(&[alias]);
        assert_eq!(code, 0);
        for group in help::CLI_GROUPS {
            assert!(stdout.contains(group), "{alias} is missing group {group}");
        }
        let normalized = normalize(&stdout);
        for flag in SANITIZE_FLAGS {
            assert!(stdout.contains(flag), "{alias} is missing {flag}");
        }
        for description in help::CLI_HELP {
            assert!(
                normalized.contains(&normalize(description)),
                "{alias} is missing {description}"
            );
        }
        assert!(stdout.contains("Complete CLI example:"));
    }
}

#[test]
fn groups_appear_in_the_documented_order() {
    let (_, stdout, _) = run(&["sanitize", "--help"]);
    let mut last = 0;
    for group in help::CLI_GROUPS {
        let position = stdout
            .find(group)
            .unwrap_or_else(|| panic!("help is missing the group `{group}`"));
        assert!(
            position >= last,
            "group `{group}` is out of the documented order"
        );
        last = position;
    }
}

#[test]
fn complete_cli_only_example_is_shown_and_parses() {
    let (code, stdout, _) = run(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Complete CLI example:"));
    let example = stdout
        .split("Complete CLI example:")
        .nth(1)
        .expect("complete CLI-only example")
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    let dir = fixture_repo();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
    let archive = dir.path().join("example.tar.gz");
    let mut args = example
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(args.remove(0), "Rustrepo-sanitizer");
    assert_eq!(args[0], "sanitize");
    args[1] = repo.to_string_lossy().into_owned();
    let output_index = args.iter().position(|arg| arg == "--output").unwrap() + 1;
    args[output_index] = archive.to_string_lossy().into_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_Rustrepo-sanitizer"))
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .expect("complete CLI example runs without a prompt");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        archive.is_file(),
        "example must create its sanitized output"
    );
}

#[test]
fn sanitize_arguments_only_can_create_a_complete_archive_without_prompts() {
    let dir = fixture_repo();
    let repo = dir.path().join("repo");
    std::fs::write(repo.join("notes.txt"), "agent supplied options\n").unwrap();
    let archive = dir.path().join("review.tar.gz");
    std::fs::create_dir_all(repo.join("target")).unwrap();
    std::fs::write(repo.join("target/output.txt"), "must be excluded\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_Rustrepo-sanitizer"))
        .args([
            "sanitize",
            repo.to_str().unwrap(),
            "--output",
            archive.to_str().unwrap(),
            "--archive",
            "tar",
            "--compression",
            "gzip",
            "--report",
            "json",
            "--include-untracked",
            "--max-file-size",
            "2MiB",
            "--include",
            "*.txt",
            "--exclude",
            "target/**",
            "--no-redact",
            "--timestamp-name",
            "false",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("CLI-only sanitize invocation");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(archive.is_file(), "the requested archive must be created");
    let listing = Command::new("tar")
        .args(["-tzf", archive.to_str().unwrap()])
        .output()
        .expect("tar can inspect the generated archive");
    assert!(listing.status.success());
    let members = String::from_utf8_lossy(&listing.stdout);
    assert!(
        members.contains("notes.txt"),
        "include glob takes effect: {members}"
    );
    assert!(
        !members.contains("README.md"),
        "unmatched tracked file excluded: {members}"
    );
    assert!(
        !members.contains("target/output.txt"),
        "exclude glob takes effect: {members}"
    );
    assert!(
        members.contains("SANITIZATION-REPORT.json"),
        "JSON report option takes effect: {members}"
    );
    let note = Command::new("tar")
        .args(["-xOzf", archive.to_str().unwrap(), "notes.txt"])
        .output()
        .expect("tar can inspect sanitized file content");
    assert!(note.status.success());
    assert_eq!(note.stdout, b"agent supplied options\n");
}

/// Builds a throwaway repository with one tracked file.
fn fixture_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "# fixture\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "init"]);
    dir
}

#[test]
fn sanitize_reports_the_effective_limit_in_binary_units() {
    let dir = fixture_repo();
    let repo = dir.path().join("repo");
    for (argument, expected) in [
        ("2MiB", "2 MiB"),
        ("1KiB", "1 KiB"),
        ("1024", "1 KiB"),
        ("1GiB", "1 GiB"),
    ] {
        let (code, stdout, stderr) = run(&[
            "sanitize",
            repo.to_str().unwrap(),
            "--dry-run",
            "--max-file-size",
            argument,
        ]);
        assert_eq!(code, 0, "{argument}: {stderr}");
        assert!(
            stdout.contains(&format!("max file size {expected}")),
            "{argument} should report '{expected}', got: {stdout}"
        );
    }
}

#[test]
fn sanitize_rejects_a_size_with_an_unknown_unit() {
    let dir = fixture_repo();
    let repo = dir.path().join("repo");
    let (code, _stdout, stderr) = run(&[
        "sanitize",
        repo.to_str().unwrap(),
        "--dry-run",
        "--max-file-size",
        "10MB",
    ]);
    assert_ne!(code, 0, "an unknown unit must be rejected");
    assert!(
        stderr.contains("unknown size unit"),
        "the error should explain the problem, got: {stderr}"
    );
}
