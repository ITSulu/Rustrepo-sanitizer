//! Canonical user-facing help text shared by the CLI and the GUI.
//!
//! The CLI references these constants directly through clap attributes, and the
//! Slint frontend uses the same wording for its tooltips. `CLI_HELP` and
//! `GUI_HELP` name the surfaces that must stay in sync; the regression tests
//! fail if the CLI help or the Slint tooltips drift from these strings.

// Launch modes for the unified binary.
pub const LAUNCH_GUI: &str = "Start the desktop GUI.";
pub const LAUNCH_WEB: &str = "Start the web UI and HTTP API.";
pub const WEB_BIND: &str = "Web bind address, for example 127.0.0.1:8787.";
pub const WEB_TOKEN: &str = "Require this bearer token for /api and the web UI login.";
pub const WEB_ROOT: &str = "Directory for web job workspaces and uploads.";
pub const WEB_LOCAL_ROOTS: &str = "Colon-separated roots allowed for the server-local path input.";
pub const WEB_FORGEJO_BASE: &str = "Forgejo base URL for the repository selector.";
pub const WEB_FORGEJO_TOKEN: &str = "Forgejo token used server-side for cloning and listing.";
pub const WEB_GITHUB_API: &str = "GitHub API base URL for the repository selector.";
pub const WEB_GITHUB_TOKEN: &str = "GitHub token used server-side for cloning and listing.";

// Repository / input.
pub const REPOSITORY: &str = "Git repository to sanitize; defaults to the current directory.";
pub const INCLUDE_UNTRACKED: &str =
    "Also include files Git does not track yet, subject to safety filters.";

// Output.
pub const OUTPUT: &str =
    "Destination archive path; leave blank to derive a filename in the repository root.";
pub const TIMESTAMP_NAME: &str = "Add a timestamp and commit id to the automatic output filename.";

// Archive / compression.
pub const ARCHIVE: &str = "Archive container: none (JSONL stream), tar, zip, or 7z.";
pub const COMPRESSION: &str =
    "Compression codec; only codecs valid for the chosen archive are accepted.";

// Include / exclude.
pub const INCLUDE: &str = "Git-style glob of paths to include; repeat for more patterns.";
pub const EXCLUDE: &str = "Git-style glob of paths to exclude; repeat for more patterns.";
pub const MAX_FILE_SIZE: &str = "Maximum file size; accepts plain bytes or KiB/MiB/GiB.";

// Redaction / sanitization.
pub const REDACT: &str = "Replace high-confidence secret values with redaction markers.";
pub const NO_REDACT: &str = "Disable redaction of detected secret values.";
pub const FAIL_ON_SECRET: &str =
    "Stop with an error instead of producing output when a secret is found.";

// Reports / metadata.
pub const REPORT: &str = "Report format: markdown, json, or none.";
pub const DRY_RUN: &str = "Scan and report results without writing an archive.";

// Security / password.
pub const PASSWORD_FILE: &str = "Read the ZIP AES password from this file.";
pub const PASSWORD_STDIN: &str = "Read the ZIP AES password from standard input.";
pub const PASSWORD_MIN_LENGTH: &str = "Minimum number of characters required in a password.";
pub const PASSWORD_REQUIRE_UPPERCASE: &str =
    "Require at least one uppercase letter in the password.";
pub const PASSWORD_REQUIRE_LOWERCASE: &str =
    "Require at least one lowercase letter in the password.";
pub const PASSWORD_REQUIRE_NUMBER: &str = "Require at least one digit in the password.";
pub const PASSWORD_REQUIRE_SPECIAL: &str =
    "Require at least one special character in the password.";

// Advanced / general.
pub const VERBOSE: &str = "Print per-file progress details.";
pub const QUIET: &str = "Suppress the final summary line.";

// GUI-only controls.
pub const REPO_SOURCE: &str = "Choose how the repository is obtained.";
pub const GIT_URL_FIELD: &str = "Public https Git repository URL.";
pub const FORGEJO_REPO: &str = "Forgejo repository cloned server-side with the configured token.";
pub const GITHUB_REPO: &str = "GitHub repository cloned server-side with the configured token.";
pub const GIT_REF: &str = "Optional branch or tag for Forgejo and GitHub sources.";
pub const SIZE_UNIT: &str = "Unit for the maximum file size; the value is read in this unit.";
pub const MAX_FILE_SIZE_GUI: &str = "Maximum file size, read in the selected unit.";
pub const BROWSE_REPOSITORY: &str = "Select a repository folder using the system picker.";
pub const BROWSE_OUTPUT: &str = "Select the output folder; the filename is derived automatically.";
pub const ADVANCED_OPTIONS: &str = "Show redaction, size limit, filter, and password controls.";
pub const INCLUDE_GLOB_LABEL: &str = "Only files matching these patterns are packed.";
pub const INCLUDE_ENTRY: &str = "Type a Git-style glob pattern to include.";
pub const INCLUDE_ADD: &str = "Add the typed or selected include pattern.";
pub const INCLUDE_REMOVE: &str = "Remove this include pattern from the list.";
pub const EXCLUDE_GLOB_LABEL: &str = "Files matching these patterns are left out.";
pub const EXCLUDE_ENTRY: &str = "Type a Git-style glob pattern to exclude.";
pub const EXCLUDE_ADD: &str = "Add the typed or selected exclude pattern.";
pub const EXCLUDE_REMOVE: &str = "Remove this exclude pattern from the list.";
pub const PASSWORD: &str = "ZIP AES password; never stored in the archive or logs.";
pub const OPEN_RESULT: &str = "Open the folder containing the finished archive.";
pub const CANCEL: &str = "Stop the current sanitization before it finishes.";
pub const SANITIZE: &str = "Build the sanitized archive with the selected options.";
pub const LABEL_ARCHIVE: &str = "Choose the container format for the sanitized bundle.";
pub const LABEL_COMPRESSION: &str = "Choose the compressor used for the bundle.";
pub const LABEL_REPORT: &str = "Choose the optional human-readable or JSON report.";
pub const LABEL_PASSWORD: &str =
    "Optional encryption for ZIP archives; the password is never persisted.";
pub const LABEL_MAX_FILE_SIZE: &str = "Files larger than this limit are excluded.";
pub const HELP_CLOSE: &str = "Close this help window.";
pub const SETTINGS_APPLY: &str = "Apply these password rules to the main window.";
pub const SETTINGS_CLOSE: &str = "Close this window without applying changes.";
pub const ABOUT_WEBSITE: &str = "Open the project website in a browser.";
pub const ABOUT_FORGEJO: &str = "Open the authoritative Forgejo repository.";
pub const ABOUT_CLOSE: &str = "Close this window.";

/// CLI argument descriptions grouped by purpose.
pub const CLI_HELP: &[&str] = &[
    REPOSITORY,
    INCLUDE_UNTRACKED,
    OUTPUT,
    TIMESTAMP_NAME,
    ARCHIVE,
    COMPRESSION,
    INCLUDE,
    EXCLUDE,
    MAX_FILE_SIZE,
    REDACT,
    NO_REDACT,
    FAIL_ON_SECRET,
    REPORT,
    DRY_RUN,
    PASSWORD_FILE,
    PASSWORD_STDIN,
    PASSWORD_MIN_LENGTH,
    PASSWORD_REQUIRE_UPPERCASE,
    PASSWORD_REQUIRE_LOWERCASE,
    PASSWORD_REQUIRE_NUMBER,
    PASSWORD_REQUIRE_SPECIAL,
    VERBOSE,
    QUIET,
];

/// GUI tooltip descriptions. Every `help:` string in `ui/main.slint` must come
/// from this list, and every entry must be used by the UI.
pub const GUI_HELP: &[&str] = &[
    REPOSITORY,
    REPO_SOURCE,
    GIT_URL_FIELD,
    FORGEJO_REPO,
    GITHUB_REPO,
    GIT_REF,
    BROWSE_REPOSITORY,
    OUTPUT,
    BROWSE_OUTPUT,
    INCLUDE_UNTRACKED,
    DRY_RUN,
    TIMESTAMP_NAME,
    LABEL_ARCHIVE,
    ARCHIVE,
    LABEL_COMPRESSION,
    COMPRESSION,
    LABEL_REPORT,
    REPORT,
    ADVANCED_OPTIONS,
    REDACT,
    FAIL_ON_SECRET,
    LABEL_MAX_FILE_SIZE,
    MAX_FILE_SIZE_GUI,
    SIZE_UNIT,
    INCLUDE_GLOB_LABEL,
    INCLUDE_ENTRY,
    INCLUDE_ADD,
    INCLUDE_REMOVE,
    EXCLUDE_GLOB_LABEL,
    EXCLUDE_ENTRY,
    EXCLUDE_ADD,
    EXCLUDE_REMOVE,
    LABEL_PASSWORD,
    PASSWORD,
    OPEN_RESULT,
    CANCEL,
    SANITIZE,
    HELP_CLOSE,
    PASSWORD_MIN_LENGTH,
    PASSWORD_REQUIRE_UPPERCASE,
    PASSWORD_REQUIRE_LOWERCASE,
    PASSWORD_REQUIRE_NUMBER,
    PASSWORD_REQUIRE_SPECIAL,
    SETTINGS_APPLY,
    SETTINGS_CLOSE,
    ABOUT_WEBSITE,
    ABOUT_FORGEJO,
    ABOUT_CLOSE,
];

/// Launch and web option descriptions for the unified binary's top-level help.
pub const LAUNCH_HELP: &[&str] = &[
    LAUNCH_GUI,
    LAUNCH_WEB,
    WEB_BIND,
    WEB_TOKEN,
    WEB_ROOT,
    WEB_LOCAL_ROOTS,
    WEB_FORGEJO_BASE,
    WEB_FORGEJO_TOKEN,
    WEB_GITHUB_API,
    WEB_GITHUB_TOKEN,
];

/// Group headings used by the CLI help, in display order.
pub const GROUP_LAUNCH: &str = "Launch";
pub const GROUP_WEB: &str = "Web server";
pub const GROUP_INPUT: &str = "Repository / input";
pub const GROUP_OUTPUT: &str = "Output";
pub const GROUP_ARCHIVE: &str = "Archive / compression";
pub const GROUP_FILTERS: &str = "Include / exclude";
pub const GROUP_REDACTION: &str = "Redaction / sanitization";
pub const GROUP_REPORTS: &str = "Reports / metadata";
pub const GROUP_SECURITY: &str = "Security / password";
pub const GROUP_ADVANCED: &str = "Advanced";

/// The CLI help groups in the exact order they must appear.
pub const CLI_GROUPS: &[&str] = &[
    GROUP_INPUT,
    GROUP_OUTPUT,
    GROUP_ARCHIVE,
    GROUP_FILTERS,
    GROUP_REDACTION,
    GROUP_REPORTS,
    GROUP_SECURITY,
    GROUP_ADVANCED,
];
