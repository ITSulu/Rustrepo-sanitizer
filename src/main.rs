use std::{io::Read, path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
use itsulu_repo_sanitizer::help;
use itsulu_repo_sanitizer::sanitizer::{
    default_output_path, run, ArchiveFormat, Compression, Config, PasswordPolicy, ReportFormat,
};

#[derive(Parser)]
#[command(
    name = "itsulu-repo-sanitizer",
    version,
    about = "Create a safe AI review archive from a Git repository"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Create a sanitized, reproducible review bundle from a Git repository")]
    Sanitize(SanitizeArgs),
    #[command(about = "List supported archive and compression formats")]
    ListFormats,
}

#[derive(Args)]
struct SanitizeArgs {
    #[arg(default_value = ".", help = help::REPOSITORY, help_heading = help::GROUP_INPUT)]
    repository: PathBuf,
    #[arg(
        long,
        help = help::INCLUDE_UNTRACKED,
        help_heading = help::GROUP_INPUT
    )]
    include_untracked: bool,
    #[arg(short, long, help = help::OUTPUT, help_heading = help::GROUP_OUTPUT)]
    output: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::TIMESTAMP_NAME,
        help_heading = help::GROUP_OUTPUT
    )]
    timestamp_name: bool,
    #[arg(
        long,
        visible_alias = "format",
        value_enum,
        default_value_t = ArchiveFormat::Tar,
        help = help::ARCHIVE,
        help_heading = help::GROUP_ARCHIVE
    )]
    archive: ArchiveFormat,
    #[arg(
        long,
        value_enum,
        default_value_t = Compression::Zstd,
        help = help::COMPRESSION,
        help_heading = help::GROUP_ARCHIVE
    )]
    compression: Compression,
    #[arg(long = "include", help = help::INCLUDE, help_heading = help::GROUP_FILTERS)]
    include: Vec<String>,
    #[arg(long = "exclude", help = help::EXCLUDE, help_heading = help::GROUP_FILTERS)]
    exclude: Vec<String>,
    #[arg(
        long = "max-file-size",
        default_value_t = 10 * 1024 * 1024,
        help = help::MAX_FILE_SIZE,
        help_heading = help::GROUP_FILTERS
    )]
    max_file_size: u64,
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::REDACT,
        help_heading = help::GROUP_REDACTION
    )]
    redact: bool,
    #[arg(
        long = "no-redact",
        action = clap::ArgAction::SetTrue,
        help = help::NO_REDACT,
        help_heading = help::GROUP_REDACTION
    )]
    no_redact: bool,
    #[arg(long, help = help::FAIL_ON_SECRET, help_heading = help::GROUP_REDACTION)]
    fail_on_secret: bool,
    #[arg(
        long,
        value_enum,
        default_value_t = CliReportFormat::Markdown,
        help = help::REPORT,
        help_heading = help::GROUP_REPORTS
    )]
    report: CliReportFormat,
    #[arg(long, help = help::DRY_RUN, help_heading = help::GROUP_REPORTS)]
    dry_run: bool,
    #[arg(
        long,
        conflicts_with = "password_stdin",
        help = help::PASSWORD_FILE,
        help_heading = help::GROUP_SECURITY
    )]
    password_file: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with = "password_file",
        help = help::PASSWORD_STDIN,
        help_heading = help::GROUP_SECURITY
    )]
    password_stdin: bool,
    #[arg(
        long = "password-min-length",
        default_value_t = 8,
        help = help::PASSWORD_MIN_LENGTH,
        help_heading = help::GROUP_SECURITY
    )]
    password_min_length: usize,
    #[arg(
        long = "password-require-uppercase",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_UPPERCASE,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_uppercase: bool,
    #[arg(
        long = "password-require-lowercase",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_LOWERCASE,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_lowercase: bool,
    #[arg(
        long = "password-require-number",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_NUMBER,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_number: bool,
    #[arg(
        long = "password-require-special",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_SPECIAL,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_special: bool,
    #[arg(short, long, help = help::VERBOSE, help_heading = help::GROUP_ADVANCED)]
    verbose: bool,
    #[arg(short, long, help = help::QUIET, help_heading = help::GROUP_ADVANCED)]
    quiet: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliReportFormat {
    Markdown,
    Json,
    None,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if matches!(cli.command, Command::ListFormats) {
        itsulu_repo_sanitizer::sanitizer::print_formats();
        return ExitCode::SUCCESS;
    }
    let Command::Sanitize(args) = cli.command else {
        unreachable!()
    };
    let format = args.archive;
    let report = match args.report {
        CliReportFormat::Markdown => ReportFormat::Markdown,
        CliReportFormat::Json => ReportFormat::Json,
        CliReportFormat::None => ReportFormat::None,
    };
    let output = match args.output {
        Some(path) => path,
        None => match default_output_path(
            &args.repository,
            format,
            args.compression,
            args.timestamp_name,
        ) {
            Ok(path) => path,
            Err(err) => {
                eprintln!("itsulu-repo-sanitizer: {err:#}");
                return ExitCode::from(if err.to_string().contains("compression") {
                    2
                } else {
                    3
                });
            }
        },
    };
    let password = match read_password(args.password_file.as_deref(), args.password_stdin) {
        Ok(password) => password,
        Err(err) => {
            eprintln!("itsulu-repo-sanitizer: {err}");
            return ExitCode::from(2);
        }
    };
    let config = Config {
        repository: args.repository,
        output,
        format,
        compression: args.compression,
        report,
        include_untracked: args.include_untracked,
        max_file_size: args.max_file_size,
        excludes: args.exclude,
        includes: args.include,
        redact: if args.no_redact { false } else { args.redact },
        fail_on_secret: args.fail_on_secret,
        dry_run: args.dry_run,
        password,
        password_policy: PasswordPolicy {
            minimum_length: args.password_min_length,
            require_uppercase: args.password_require_uppercase,
            require_lowercase: args.password_require_lowercase,
            require_number: args.password_require_number,
            require_special: args.password_require_special,
        },
        password_file: args.password_file,
        verbose: args.verbose,
        quiet: args.quiet,
    };
    match run(config) {
        Ok(summary) => {
            if !summary.quiet {
                println!(
                    "sanitized {} files ({} excluded, {} redactions){}",
                    summary.included,
                    summary.excluded,
                    summary.redactions,
                    if summary.dry_run { "; dry run" } else { "" }
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("itsulu-repo-sanitizer: {err:#}");
            let message = err.to_string().to_ascii_lowercase();
            let code = if message.contains("secret detected") {
                4
            } else if message.contains("creating ") || message.contains("output archive") {
                5
            } else if message.contains("git ") || message.contains("working tree") {
                3
            } else {
                2
            };
            ExitCode::from(code)
        }
    }
}

fn read_password(
    path: Option<&std::path::Path>,
    from_stdin: bool,
) -> Result<Option<String>, String> {
    let value = if let Some(path) = path {
        std::fs::read_to_string(path).map_err(|_| "unable to read password file".to_owned())?
    } else if from_stdin {
        let mut value = String::new();
        std::io::stdin()
            .read_to_string(&mut value)
            .map_err(|_| "unable to read password from stdin".to_owned())?;
        value
    } else {
        return Ok(None);
    };
    let value = value.strip_suffix('\n').unwrap_or(&value);
    let value = value.strip_suffix('\r').unwrap_or(value).to_owned();
    if value.is_empty() {
        Err("password must not be empty".to_owned())
    } else {
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_parser_preserves_safe_defaults_and_explicit_options() {
        let cli = Cli::try_parse_from([
            "itsulu-repo-sanitizer",
            "sanitize",
            "repo",
            "--archive",
            "none",
            "--compression",
            "gzip",
            "--report",
            "json",
            "--include",
            "src/**",
            "--exclude",
            "target/**",
            "--include-untracked",
            "--dry-run",
            "--no-redact",
        ])
        .unwrap();
        let Command::Sanitize(args) = cli.command else {
            panic!("expected sanitize command")
        };
        assert_eq!(args.repository, PathBuf::from("repo"));
        assert_eq!(args.archive, ArchiveFormat::None);
        assert_eq!(args.compression, Compression::Gzip);
        assert!(matches!(args.report, CliReportFormat::Json));
        assert_eq!(args.include, vec!["src/**"]);
        assert_eq!(args.exclude, vec!["target/**"]);
        assert!(args.include_untracked && args.dry_run && args.no_redact && args.redact);
    }

    #[test]
    fn password_file_and_stdin_are_mutually_exclusive() {
        assert!(Cli::try_parse_from([
            "itsulu-repo-sanitizer",
            "sanitize",
            "--password-file",
            "password.txt",
            "--password-stdin",
        ])
        .is_err());
    }

    #[test]
    fn sanitize_parser_uses_documented_defaults() {
        let cli = Cli::try_parse_from(["itsulu-repo-sanitizer", "sanitize"]).unwrap();
        let Command::Sanitize(args) = cli.command else {
            panic!("expected sanitize command")
        };
        assert_eq!(args.repository, PathBuf::from("."));
        assert_eq!(args.archive, ArchiveFormat::Tar);
        assert_eq!(args.compression, Compression::Zstd);
        assert!(matches!(args.report, CliReportFormat::Markdown));
        assert!(args.redact && args.timestamp_name);
        assert_eq!(args.password_min_length, 8);
    }
}
