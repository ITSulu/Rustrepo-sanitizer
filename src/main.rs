use std::{io::Read, path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
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
    Sanitize(SanitizeArgs),
    ListFormats,
}

#[derive(Args)]
struct SanitizeArgs {
    #[arg(default_value = ".")]
    repository: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long, visible_alias = "format", value_enum, default_value_t = ArchiveFormat::Tar)]
    archive: ArchiveFormat,
    #[arg(long, value_enum, default_value_t = Compression::Zstd)]
    compression: Compression,
    #[arg(long, value_enum, default_value_t = CliReportFormat::Markdown)]
    report: CliReportFormat,
    #[arg(long)]
    include_untracked: bool,
    #[arg(long = "max-file-size", default_value_t = 10 * 1024 * 1024)]
    max_file_size: u64,
    #[arg(long = "exclude")]
    exclude: Vec<String>,
    #[arg(long = "include")]
    include: Vec<String>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    redact: bool,
    #[arg(long = "no-redact", action = clap::ArgAction::SetTrue)]
    no_redact: bool,
    #[arg(long)]
    fail_on_secret: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    timestamp_name: bool,
    #[arg(long, conflicts_with = "password_stdin")]
    password_file: Option<PathBuf>,
    #[arg(long, conflicts_with = "password_file")]
    password_stdin: bool,
    #[arg(long = "password-min-length", default_value_t = 8)]
    password_min_length: usize,
    #[arg(long = "password-require-uppercase", default_value_t = true, action = clap::ArgAction::Set)]
    password_require_uppercase: bool,
    #[arg(long = "password-require-lowercase", default_value_t = true, action = clap::ArgAction::Set)]
    password_require_lowercase: bool,
    #[arg(long = "password-require-number", default_value_t = true, action = clap::ArgAction::Set)]
    password_require_number: bool,
    #[arg(long = "password-require-special", default_value_t = true, action = clap::ArgAction::Set)]
    password_require_special: bool,
    #[arg(short, long)]
    verbose: bool,
    #[arg(short, long)]
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
