//! Regression tests for the grouped CLI help output.
use std::collections::BTreeSet;
use std::process::Command;

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
    let (code, stdout, _) = run(&["sanitize", "-help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Usage:"));
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
    let flags: Vec<String> = long_flags(&stdout)
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
    for flag in long_flags(&stdout) {
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
