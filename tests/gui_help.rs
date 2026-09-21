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
