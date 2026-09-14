#[cfg(feature = "gui")]
slint::include_modules!();

#[cfg(feature = "gui")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use itsulu_repo_sanitizer::sanitizer::{
        default_output_path, run, validate_config, ArchiveFormat, Compression, Config, ReportFormat,
    };
    use std::path::PathBuf;
    let window = MainWindow::new()?;
    let weak = window.as_weak();
    window.on_sanitize(
        move |repo,
              output,
              untracked,
              dry_run,
              format_index,
              compression_index,
              report_index,
              timestamp,
              redact,
              fail_secret,
              max_size,
              include,
              exclude,
              password| {
            let repo = PathBuf::from(repo.to_string());
            let format = match format_index {
                1 => ArchiveFormat::Zip,
                2 => ArchiveFormat::SevenZip,
                _ => ArchiveFormat::Tar,
            };
            let compression = match compression_index {
                1 => Compression::Gzip,
                2 => Compression::None,
                _ => Compression::Zstd,
            };
            let report = match report_index {
                1 => ReportFormat::Json,
                2 => ReportFormat::None,
                _ => ReportFormat::Markdown,
            };
            let output = if output.is_empty() {
                default_output_path(&repo, format, compression, timestamp)
                    .unwrap_or_else(|_| PathBuf::from("sanitized.tar.zst"))
            } else {
                PathBuf::from(output.to_string())
            };
            let config = Config {
                repository: repo,
                output,
                format,
                compression,
                report,
                include_untracked: untracked,
                max_file_size: max_size.parse().unwrap_or(10 * 1024 * 1024),
                excludes: if exclude.is_empty() {
                    vec![]
                } else {
                    vec![exclude.to_string()]
                },
                includes: if include.is_empty() {
                    vec![]
                } else {
                    vec![include.to_string()]
                },
                redact,
                fail_on_secret: fail_secret,
                dry_run,
                password: if password.is_empty() {
                    None
                } else {
                    Some(password.to_string())
                },
                password_file: None,
                verbose: false,
                quiet: true,
            };
            if let Err(error) = validate_config(&config) {
                let validation_ui = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = validation_ui.upgrade() {
                        window.set_status(format!("Invalid options: {error}").into());
                    }
                });
                return;
            }
            let ui = weak.clone();
            let final_ui = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = ui.upgrade() {
                    window.set_status("Sanitizing…".into());
                }
            });
            std::thread::spawn(move || {
                let result = run(config);
                let status = match result {
                    Ok(s) => format!(
                        "Complete: {} files, {} redactions",
                        s.included, s.redactions
                    ),
                    Err(e) => format!("Error: {e:#}"),
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = final_ui.upgrade() {
                        window.set_status(status.into());
                    }
                });
            });
        },
    );
    window.run()?;
    Ok(())
}
#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("GUI support is disabled; run with --features gui");
    std::process::exit(2);
}
