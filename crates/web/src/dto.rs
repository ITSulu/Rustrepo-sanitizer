//! Request/response types and the mapping from web options onto the shared
//! sanitizer [`Config`]. Validation is delegated to the shared core so the web
//! frontend cannot diverge from the CLI or GUI.

use std::path::PathBuf;

use itsulu_repo_sanitizer::sanitizer::{
    self, ArchiveFormat, Compression, Config, PasswordPolicy, ReportFormat,
};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_format() -> ArchiveFormat {
    ArchiveFormat::Tar
}

fn default_compression() -> Compression {
    Compression::Zstd
}

fn default_report() -> ReportFormat {
    ReportFormat::Markdown
}

fn default_max_file_size() -> u64 {
    10 * 1024 * 1024
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct PasswordPolicyDto {
    pub minimum_length: usize,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_number: bool,
    pub require_special: bool,
}

impl Default for PasswordPolicyDto {
    fn default() -> Self {
        let policy = PasswordPolicy::default();
        Self {
            minimum_length: policy.minimum_length,
            require_uppercase: policy.require_uppercase,
            require_lowercase: policy.require_lowercase,
            require_number: policy.require_number,
            require_special: policy.require_special,
        }
    }
}

impl From<PasswordPolicyDto> for PasswordPolicy {
    fn from(dto: PasswordPolicyDto) -> Self {
        PasswordPolicy {
            minimum_length: dto.minimum_length,
            require_uppercase: dto.require_uppercase,
            require_lowercase: dto.require_lowercase,
            require_number: dto.require_number,
            require_special: dto.require_special,
        }
    }
}

/// The sanitization options shared by every input mode.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct OptionsDto {
    #[serde(default = "default_format")]
    pub format: ArchiveFormat,
    #[serde(default = "default_compression")]
    pub compression: Compression,
    #[serde(default = "default_report")]
    pub report: ReportFormat,
    pub include_untracked: bool,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    #[serde(default = "default_true")]
    pub redact: bool,
    pub fail_on_secret: bool,
    pub dry_run: bool,
    #[serde(default = "default_true")]
    pub timestamp_name: bool,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub password_policy: PasswordPolicyDto,
    /// Optional explicit output filename (validated; no directories allowed).
    #[serde(default)]
    pub output_name: Option<String>,
}

impl Default for OptionsDto {
    fn default() -> Self {
        Self {
            format: default_format(),
            compression: default_compression(),
            report: default_report(),
            include_untracked: false,
            max_file_size: default_max_file_size(),
            includes: Vec::new(),
            excludes: Vec::new(),
            redact: true,
            fail_on_secret: false,
            dry_run: false,
            timestamp_name: true,
            password: None,
            password_policy: PasswordPolicyDto::default(),
            output_name: None,
        }
    }
}

impl OptionsDto {
    /// Builds a core [`Config`], delegating capability/password validation to
    /// the shared core. `output` is the resolved, workspace-contained target.
    pub fn to_config(&self, repository: PathBuf, output: PathBuf) -> Result<Config, String> {
        for pattern in self.includes.iter().chain(self.excludes.iter()) {
            if pattern.trim().is_empty() {
                return Err("glob patterns must not be empty".to_owned());
            }
            globset::Glob::new(pattern)
                .map_err(|err| format!("invalid glob `{pattern}`: {err}"))?;
        }
        let config = Config {
            repository,
            output,
            format: self.format,
            compression: self.compression,
            report: self.report,
            include_untracked: self.include_untracked,
            max_file_size: self.max_file_size,
            excludes: self.excludes.clone(),
            includes: self.includes.clone(),
            redact: self.redact,
            fail_on_secret: self.fail_on_secret,
            dry_run: self.dry_run,
            password: self.password.clone(),
            password_policy: self.password_policy.clone().into(),
            password_file: None,
            verbose: false,
            quiet: true,
        };
        sanitizer::validate_config(&config).map_err(|err| format!("{err:#}"))?;
        Ok(config)
    }
}

/// Which repository source the request is using.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    LocalPath,
    GitUrl,
    Upload,
    Forgejo,
    #[serde(rename = "github")]
    GitHub,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum InputSpec {
    LocalPath {
        path: String,
    },
    GitUrl {
        url: String,
    },
    Upload {
        upload_id: String,
    },
    Forgejo {
        owner: String,
        repo: String,
        git_ref: Option<String>,
    },
    #[serde(rename = "github")]
    GitHub {
        owner: String,
        repo: String,
        git_ref: Option<String>,
    },
}

impl InputSpec {
    pub fn mode(&self) -> InputMode {
        match self {
            InputSpec::LocalPath { .. } => InputMode::LocalPath,
            InputSpec::GitUrl { .. } => InputMode::GitUrl,
            InputSpec::Upload { .. } => InputMode::Upload,
            InputSpec::Forgejo { .. } => InputMode::Forgejo,
            InputSpec::GitHub { .. } => InputMode::GitHub,
        }
    }
}

/// One capability as presented to the UI (id, label, surface).
#[derive(Clone, Debug, Serialize)]
pub struct CapabilityView {
    pub id: &'static str,
    pub label: &'static str,
    pub surface: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct FormatView {
    pub name: String,
    pub label: &'static str,
    pub extension: &'static str,
    pub password_encryption: bool,
    pub compressions: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompressionView {
    pub name: String,
    pub label: &'static str,
    pub extension: &'static str,
}

/// Help text and capability metadata shared by CLI, GUI, and web.
#[derive(Clone, Debug, Serialize)]
pub struct CapabilitiesView {
    pub version: String,
    pub capabilities: Vec<CapabilityView>,
    pub formats: Vec<FormatView>,
    pub compressions: Vec<CompressionView>,
    pub help: HelpView,
    pub defaults: OptionsDto,
}

#[derive(Clone, Debug, Serialize)]
pub struct HelpView {
    pub groups: Vec<GroupView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GroupView {
    pub heading: &'static str,
    pub entries: Vec<HelpEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HelpEntry {
    pub field: &'static str,
    pub text: &'static str,
}

fn compression_name(compression: Compression) -> String {
    serde_json::to_value(compression)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn format_name(format: ArchiveFormat) -> String {
    serde_json::to_value(format)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Builds the capability/help view the UI renders from, reusing the shared
/// registries in the core crate.
pub fn capabilities_view() -> CapabilitiesView {
    use itsulu_repo_sanitizer::help as h;

    let capabilities = itsulu_repo_sanitizer::CAPABILITIES
        .iter()
        .map(|capability| CapabilityView {
            id: capability.id,
            label: capability.label,
            surface: match capability.surface {
                itsulu_repo_sanitizer::CapabilitySurface::CliGui => "cli_gui",
                itsulu_repo_sanitizer::CapabilitySurface::CliOnly => "cli_only",
                itsulu_repo_sanitizer::CapabilitySurface::Internal => "internal",
            },
        })
        .collect();

    let formats = sanitizer::ARCHIVE_CAPABILITIES
        .iter()
        .map(|capability| FormatView {
            name: format_name(capability.format),
            label: capability.label,
            extension: capability.extension,
            password_encryption: capability.password_encryption,
            compressions: sanitizer::compatible_compressions(capability.format)
                .into_iter()
                .map(compression_name)
                .collect(),
        })
        .collect();

    let compressions = sanitizer::COMPRESSION_CAPABILITIES
        .iter()
        .map(|capability| CompressionView {
            name: compression_name(capability.compression),
            label: capability.label,
            extension: capability.extension,
        })
        .collect();

    let group = |heading: &'static str, entries: Vec<HelpEntry>| GroupView { heading, entries };
    let entry = |field: &'static str, text: &'static str| HelpEntry { field, text };
    let help = HelpView {
        groups: vec![
            group(
                h::GROUP_INPUT,
                vec![
                    entry("repository", h::REPOSITORY),
                    entry("include_untracked", h::INCLUDE_UNTRACKED),
                ],
            ),
            group(
                h::GROUP_OUTPUT,
                vec![
                    entry("output", h::OUTPUT),
                    entry("timestamp_name", h::TIMESTAMP_NAME),
                ],
            ),
            group(
                h::GROUP_ARCHIVE,
                vec![
                    entry("format", h::ARCHIVE),
                    entry("compression", h::COMPRESSION),
                ],
            ),
            group(
                h::GROUP_FILTERS,
                vec![
                    entry("include", h::INCLUDE),
                    entry("exclude", h::EXCLUDE),
                    entry("max_file_size", h::MAX_FILE_SIZE),
                ],
            ),
            group(
                h::GROUP_REDACTION,
                vec![
                    entry("redact", h::REDACT),
                    entry("no_redact", h::NO_REDACT),
                    entry("fail_on_secret", h::FAIL_ON_SECRET),
                ],
            ),
            group(
                h::GROUP_REPORTS,
                vec![entry("report", h::REPORT), entry("dry_run", h::DRY_RUN)],
            ),
            group(
                h::GROUP_SECURITY,
                vec![
                    entry("password", h::PASSWORD),
                    entry("password_min_length", h::PASSWORD_MIN_LENGTH),
                    entry("password_require_uppercase", h::PASSWORD_REQUIRE_UPPERCASE),
                    entry("password_require_lowercase", h::PASSWORD_REQUIRE_LOWERCASE),
                    entry("password_require_number", h::PASSWORD_REQUIRE_NUMBER),
                    entry("password_require_special", h::PASSWORD_REQUIRE_SPECIAL),
                ],
            ),
            // `--verbose`/`--quiet` are CLI-only diagnostics; the server always
            // runs quietly and reports through the job API, so they are not
            // advertised here.
        ],
    };

    CapabilitiesView {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        capabilities,
        formats,
        compressions,
        help,
        defaults: OptionsDto::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itsulu_repo_sanitizer::sanitizer::{compatible_compressions, output_extension};

    #[test]
    fn defaults_match_cli_defaults() {
        let opts = OptionsDto::default();
        assert_eq!(opts.format, ArchiveFormat::Tar);
        assert_eq!(opts.compression, Compression::Zstd);
        assert_eq!(opts.report, ReportFormat::Markdown);
        assert!(opts.redact);
        assert!(opts.timestamp_name);
        assert!(!opts.include_untracked);
        assert_eq!(opts.max_file_size, 10 * 1024 * 1024);
        assert_eq!(opts.password_policy.minimum_length, 8);
    }

    #[test]
    fn sevenzip_serializes_as_7z() {
        assert_eq!(
            serde_json::to_string(&ArchiveFormat::SevenZip).unwrap(),
            "\"7z\""
        );
        let parsed: ArchiveFormat = serde_json::from_str("\"7z\"").unwrap();
        assert_eq!(parsed, ArchiveFormat::SevenZip);
    }

    #[test]
    fn options_map_to_valid_config() {
        let opts = OptionsDto::default();
        let config = opts
            .to_config(PathBuf::from("/repo"), PathBuf::from("/out/x.tar.zst"))
            .unwrap();
        assert_eq!(config.format, ArchiveFormat::Tar);
        assert!(config.redact);
    }

    #[test]
    fn invalid_compression_is_rejected_for_format() {
        let opts = OptionsDto {
            format: ArchiveFormat::Zip,
            compression: Compression::Lz4,
            ..OptionsDto::default()
        };
        let err = opts
            .to_config(PathBuf::from("/repo"), PathBuf::from("/out/x"))
            .err()
            .unwrap();
        assert!(err.contains("compression"), "{err}");
    }

    #[test]
    fn password_only_allowed_for_zip() {
        let opts = OptionsDto {
            format: ArchiveFormat::Tar,
            password: Some("Str0ng!Pass".into()),
            ..OptionsDto::default()
        };
        let err = opts
            .to_config(PathBuf::from("/repo"), PathBuf::from("/out/x.tar"))
            .err()
            .unwrap();
        assert!(err.contains("ZIP"), "{err}");
    }

    #[test]
    fn empty_glob_is_rejected() {
        let opts = OptionsDto {
            includes: vec!["  ".into()],
            ..OptionsDto::default()
        };
        assert!(opts
            .to_config(PathBuf::from("/repo"), PathBuf::from("/out/x"))
            .is_err());
    }

    #[test]
    fn capabilities_view_reuses_shared_metadata() {
        let view = capabilities_view();
        assert!(!view.capabilities.is_empty());
        assert!(view.capabilities.iter().any(|c| c.id == "repository"));
        // Every compatible compression is advertised for each format.
        let zip = view.formats.iter().find(|f| f.name == "zip").unwrap();
        assert!(zip.password_encryption);
        assert_eq!(
            zip.compressions.len(),
            compatible_compressions(ArchiveFormat::Zip).len()
        );
        assert!(output_extension(ArchiveFormat::Zip, Compression::Zstd).is_ok());
        assert!(view
            .help
            .groups
            .iter()
            .any(|g| g.heading == "Archive / compression"));
    }
}
