//! Regression tests for GUI hover help and its shared description source.
use std::collections::BTreeSet;

const UI: &str = include_str!("../ui/main.slint");

/// Collect every non-empty `help: "..."` value in the Slint source.
fn help_strings(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = source;
    while let Some(index) = rest.find("help:") {
        let after = &rest[index + "help:".len()..];
        let after = after.trim_start();
        if let Some(after) = after.strip_prefix('"') {
            if let Some(end) = after.find('"') {
                let value = &after[..end];
                if !value.is_empty() {
                    found.insert(value.to_owned());
                }
            }
        }
        rest = &rest[index + "help:".len()..];
    }
    found
}

/// Every user-facing control in `ui/main.slint` must be a Helpful* variant
/// carrying a non-empty help string.
fn control_lines_without_help(source: &str) -> Vec<String> {
    const CONTROLS: [&str; 5] = [
        "HelpfulButton",
        "HelpfulCheckBox",
        "HelpfulComboBox",
        "HelpfulLineEdit",
        "HelpfulLabel",
    ];
    source
        .lines()
        .filter(|line| !line.contains("component "))
        .filter(|line| CONTROLS.iter().any(|control| line.contains(control)))
        .filter(|line| !line.contains("help:"))
        .map(str::trim)
        .map(str::to_owned)
        .collect()
}

#[test]
fn gui_tooltips_use_the_shared_description_source() {
    let ui = help_strings(UI);
    let shared: BTreeSet<String> = itsulu_repo_sanitizer::help::GUI_HELP
        .iter()
        .map(|text| (*text).to_owned())
        .collect();
    assert_eq!(
        ui, shared,
        "every GUI tooltip must come from help::GUI_HELP and every entry must be used"
    );
}

#[test]
fn every_gui_control_has_a_tooltip() {
    let missing = control_lines_without_help(UI);
    assert!(
        missing.is_empty(),
        "these controls have no help text: {missing:#?}"
    );
}

#[test]
fn tooltip_delay_is_approximately_one_second() {
    assert!(
        UI.contains("in property <duration> tip-delay: 1s;"),
        "the shared tooltip delay constant must be one second"
    );
    assert!(
        UI.contains("interval: Help.tip-delay;"),
        "the dwell timer must use the shared delay constant"
    );
}

#[test]
fn tooltip_appears_after_dwell_and_clears_on_leave() {
    assert!(
        UI.contains("Help.tip-visible = true;"),
        "the dwell timer must show the tooltip"
    );
    assert!(
        UI.contains("Help.tip-visible = false;"),
        "leaving the control must hide the tooltip"
    );
    assert!(
        UI.contains("changed hovered => {"),
        "hover transitions must drive tooltip visibility"
    );
}

#[test]
fn conditional_password_control_retains_help() {
    let start = UI
        .find("if format.current-index == 1:")
        .expect("conditional ZIP password block must exist");
    let block = &UI[start..];
    let end = block.find("}}\n").unwrap_or(block.len());
    let block = &block[..end];
    assert!(
        block.contains("help: \"ZIP AES password; never stored in the archive or logs.\""),
        "the conditional password field must keep its tooltip"
    );
    assert!(
        block.contains(
            "help: \"Optional encryption for ZIP archives; the password is never persisted.\""
        ),
        "the conditional password label must keep its tooltip"
    );
}

/// Extract a `name: "value"` field from a single Slint line.
fn field_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}: \"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

#[test]
fn each_control_maps_to_the_intended_description() {
    use itsulu_repo_sanitizer::help;
    let expected: BTreeSet<(&str, &str)> = [
        ("Repository label", help::REPOSITORY),
        ("Repository path", help::REPOSITORY),
        ("Browse for repository", help::BROWSE_REPOSITORY),
        ("Output file label", help::OUTPUT),
        ("Output file path", help::OUTPUT),
        ("Browse for output folder", help::BROWSE_OUTPUT),
        ("Include untracked files", help::INCLUDE_UNTRACKED),
        ("Dry run", help::DRY_RUN),
        ("Timestamp output filename", help::TIMESTAMP_NAME),
        ("Archive label", help::LABEL_ARCHIVE),
        ("Archive format", help::ARCHIVE),
        ("Compression label", help::LABEL_COMPRESSION),
        ("Compression", help::COMPRESSION),
        ("Report label", help::LABEL_REPORT),
        ("Report format", help::REPORT),
        ("Advanced options", help::ADVANCED_OPTIONS),
        ("Redact secrets", help::REDACT),
        ("Fail on secret", help::FAIL_ON_SECRET),
        ("Maximum file size label", help::LABEL_MAX_FILE_SIZE),
        ("Maximum file size", help::MAX_FILE_SIZE),
        ("Include glob label", help::INCLUDE_GLOB_LABEL),
        ("Common include glob patterns", help::INCLUDE_GLOB_LABEL),
        ("Custom include glob", help::INCLUDE_ENTRY),
        ("Add", help::INCLUDE_ADD),
        ("Selected include glob", help::INCLUDE_GLOB_LABEL),
        ("Remove selected include glob", help::INCLUDE_REMOVE),
        ("Exclude glob label", help::EXCLUDE_GLOB_LABEL),
        ("Common exclude glob patterns", help::EXCLUDE_GLOB_LABEL),
        ("Custom exclude glob", help::EXCLUDE_ENTRY),
        ("Add", help::EXCLUDE_ADD),
        ("Selected exclude glob", help::EXCLUDE_GLOB_LABEL),
        ("Remove selected exclude glob", help::EXCLUDE_REMOVE),
        ("Password label", help::LABEL_PASSWORD),
        ("Password", help::PASSWORD),
        ("Open output folder", help::OPEN_RESULT),
        ("Cancel sanitization", help::CANCEL),
        ("Sanitize repository", help::SANITIZE),
        ("Close help", help::HELP_CLOSE),
        ("Minimum password length", help::PASSWORD_MIN_LENGTH),
        ("Require uppercase", help::PASSWORD_REQUIRE_UPPERCASE),
        ("Require lowercase", help::PASSWORD_REQUIRE_LOWERCASE),
        ("Require number", help::PASSWORD_REQUIRE_NUMBER),
        ("Require special character", help::PASSWORD_REQUIRE_SPECIAL),
        ("Apply password rules", help::SETTINGS_APPLY),
        ("Close settings", help::SETTINGS_CLOSE),
        ("Open project website", help::ABOUT_WEBSITE),
        ("Open Forgejo repository", help::ABOUT_FORGEJO),
        ("Close about", help::ABOUT_CLOSE),
    ]
    .into_iter()
    .collect();

    let actual: BTreeSet<(&str, &str)> = UI
        .lines()
        .filter_map(|line| {
            let label = field_value(line, "a11y-label")?;
            let help = field_value(line, "help")?;
            Some((label, help))
        })
        .collect();

    assert_eq!(
        actual, expected,
        "each accessible control must map to its intended shared description"
    );
}

#[test]
fn maximum_file_size_has_a_unit_selector_and_label() {
    assert!(
        UI.contains("id=\"max-file-size-unit\""),
        "the GUI needs a Maximum file size unit selector"
    );
    for unit in ["KiB", "MiB", "GiB"] {
        assert!(
            UI.contains(&format!("\"{unit}\"")),
            "GUI unit option missing: {unit}"
        );
    }
    assert!(
        UI.contains("\"Maximum File Size\""),
        "the GUI label must use title case"
    );
}

#[test]
fn gui_offers_every_repository_source_with_a_branch_field() {
    for label in [
        "\"Local Path\"",
        "\"Git URL\"",
        "\"Forgejo Repository\"",
        "\"GitHub Repository\"",
        "\"Branch Or Tag\"",
    ] {
        assert!(UI.contains(label), "missing GUI source label: {label}");
    }
    assert!(
        UI.contains("id=\"repo-source\""),
        "the GUI needs a repository source selector"
    );
    assert!(
        UI.contains("id=\"git-ref\""),
        "the GUI needs a branch or tag field"
    );
}

#[test]
fn gui_input_text_uses_a_readable_light_grey() {
    assert!(
        UI.contains("color: #d1d5db") || UI.contains("#d4d4d8") || UI.contains("#cbd5e1"),
        "input text must be mildly light grey, not black"
    );
}

#[test]
fn accessibility_names_are_forwarded_to_the_wrapped_controls() {
    assert!(
        UI.contains("accessible-id: root.a11y-id;"),
        "wrappers must forward accessible-id to the real control"
    );
    assert!(
        UI.contains("accessible-label: root.a11y-label;"),
        "wrappers must forward accessible-label to the real control"
    );
}
