#[cfg(feature = "gui")]
slint::include_modules!();

#[cfg(feature = "gui")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use itsulu_repo_sanitizer::sanitizer::{
        add_pattern, default_output_path, remove_pattern, run_with_progress, validate_config,
        ArchiveFormat, Compression, Config, PasswordPolicy, ProgressEvent, ReportFormat,
    };
    use slint::{ComponentHandle, Model, ModelRc, VecModel};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let window = MainWindow::new()?;
    let include_ui = window.as_weak();
    window.on_add_include(move |pattern| {
        if let Some(window) = include_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_include_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            match add_pattern(&mut patterns, pattern.as_str()) {
                Ok(true) => {
                    window.set_include_patterns(ModelRc::new(VecModel::from(
                        patterns
                            .iter()
                            .map(|pattern| pattern.clone().into())
                            .collect::<Vec<slint::SharedString>>(),
                    )));
                    window.set_include_glob(patterns.join("\n").into());
                }
                Ok(false) => window.set_status("Include pattern already selected".into()),
                Err(error) => window.set_status(format!("Invalid include glob: {error:#}").into()),
            }
        }
    });
    let include_remove_ui = window.as_weak();
    window.on_remove_include(move |pattern| {
        if let Some(window) = include_remove_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_include_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            if remove_pattern(&mut patterns, pattern.as_str()) {
                window.set_include_patterns(ModelRc::new(VecModel::from(
                    patterns
                        .iter()
                        .map(|pattern| pattern.clone().into())
                        .collect::<Vec<slint::SharedString>>(),
                )));
                window.set_include_glob(patterns.join("\n").into());
            }
        }
    });
    let exclude_ui = window.as_weak();
    window.on_add_exclude(move |pattern| {
        if let Some(window) = exclude_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_exclude_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            match add_pattern(&mut patterns, pattern.as_str()) {
                Ok(true) => {
                    window.set_exclude_patterns(ModelRc::new(VecModel::from(
                        patterns
                            .iter()
                            .map(|pattern| pattern.clone().into())
                            .collect::<Vec<slint::SharedString>>(),
                    )));
                    window.set_exclude_glob(patterns.join("\n").into());
                }
                Ok(false) => window.set_status("Exclude pattern already selected".into()),
                Err(error) => window.set_status(format!("Invalid exclude glob: {error:#}").into()),
            }
        }
    });
    let exclude_remove_ui = window.as_weak();
    window.on_remove_exclude(move |pattern| {
        if let Some(window) = exclude_remove_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_exclude_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            if remove_pattern(&mut patterns, pattern.as_str()) {
                window.set_exclude_patterns(ModelRc::new(VecModel::from(
                    patterns
                        .iter()
                        .map(|pattern| pattern.clone().into())
                        .collect::<Vec<slint::SharedString>>(),
                )));
                window.set_exclude_glob(patterns.join("\n").into());
            }
        }
    });
    let help_window = Rc::new(HelpWindow::new()?);
    let settings_window = Rc::new(SettingsWindow::new()?);
    let about_window = Rc::new(AboutWindow::new()?);
    about_window.set_version(env!("CARGO_PKG_VERSION").into());
    about_window.set_build_date(
        option_env!("SOURCE_DATE_EPOCH")
            .map(|value| format!("SOURCE_DATE_EPOCH={value}"))
            .unwrap_or_else(|| "reproducible build metadata unavailable".to_owned())
            .into(),
    );
    let help_for_callback = help_window.clone();
    window.on_show_help(move || {
        let _ = help_for_callback.show();
    });
    let settings_for_callback = settings_window.clone();
    window.on_show_settings(move || {
        let _ = settings_for_callback.show();
    });
    let policy_main = window.as_weak();
    let policy_settings = settings_window.clone();
    settings_window.on_apply_policy(move || {
        if let Some(window) = policy_main.upgrade() {
            window.set_password_policy_minimum_length(policy_settings.get_minimum_length());
            window.set_password_policy_uppercase(policy_settings.get_require_uppercase());
            window.set_password_policy_lowercase(policy_settings.get_require_lowercase());
            window.set_password_policy_number(policy_settings.get_require_number());
            window.set_password_policy_special(policy_settings.get_require_special());
        }
    });
    let about_for_callback = about_window.clone();
    window.on_show_about(move || {
        let _ = about_for_callback.show();
    });
    let about_links = about_window.clone();
    about_links.on_open_website(move |url| {
        let _ = webbrowser::open(url.as_str());
    });
    window.on_quit_requested(|| {
        let _ = slint::quit_event_loop();
    });
    let cancel_shortcut_ui = window.as_weak();
    window.on_cancel_shortcut(move || {
        if let Some(window) = cancel_shortcut_ui.upgrade() {
            window.invoke_cancel();
        }
    });
    let weak = window.as_weak();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let cancel_ui = weak.clone();
    let cancel_for_callback = cancel_token.clone();
    window.on_cancel(move || {
        cancel_for_callback.store(true, Ordering::Relaxed);
        let _ = slint::invoke_from_event_loop({
            let cancel_ui = cancel_ui.clone();
            move || {
                if let Some(window) = cancel_ui.upgrade() {
                    window.set_status("Cancelling…".into());
                }
            }
        });
    });
    let browse_ui = weak.clone();
    window.on_browse(move || {
        let browse_ui = browse_ui.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Choose Git repository")
                .pick_folder()
            {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = browse_ui.upgrade() {
                        window.set_repository_path(path.display().to_string().into());
                        if window.get_output_path().is_empty() {
                            if let Ok(output) = default_output_path(
                                &path,
                                ArchiveFormat::Tar,
                                Compression::Zstd,
                                true,
                            ) {
                                window.set_output_path(output.display().to_string().into());
                            }
                        }
                        window
                            .set_status(format!("Selected repository: {}", path.display()).into());
                    }
                });
            }
        });
    });
    let output_ui = weak.clone();
    window.on_browse_output(move |repo, format_index, compression_index, timestamp| {
        let output_ui = output_ui.clone();
        let repo = PathBuf::from(repo.to_string());
        std::thread::spawn(move || {
            if let Some(directory) = rfd::FileDialog::new()
                .set_title("Choose output folder")
                .pick_folder()
            {
                let format = match format_index {
                    1 => ArchiveFormat::Zip,
                    2 => ArchiveFormat::SevenZip,
                    3 => ArchiveFormat::None,
                    _ => ArchiveFormat::Tar,
                };
                let compression = match format_index {
                    1 => match compression_index {
                        0 => Compression::Gzip,
                        _ => Compression::Zstd,
                    },
                    2 => Compression::None,
                    3 => match compression_index {
                        0 => Compression::Gzip,
                        1 => Compression::Zstd,
                        2 => Compression::Lz4,
                        3 => Compression::Xz,
                        4 => Compression::Zlib,
                        5 => Compression::Brotli,
                        6 => Compression::Snappy,
                        _ => Compression::Bzip2,
                    },
                    _ => match compression_index {
                        1 => Compression::Gzip,
                        2 => Compression::None,
                        3 => Compression::Lzip,
                        4 => Compression::Lzma,
                        5 => Compression::Lzo,
                        6 => Compression::Lrzip,
                        7 => Compression::Xz,
                        _ => Compression::Zstd,
                    },
                };
                let filename = default_output_path(&repo, format, compression, timestamp)
                    .ok()
                    .and_then(|path| path.file_name().map(|name| name.to_owned()))
                    .unwrap_or_else(|| "sanitized.tar.zst".into());
                let path = directory.join(filename);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = output_ui.upgrade() {
                        window.set_status(
                            format!("Output folder selected: {}", directory.display()).into(),
                        );
                        window.set_output_path(path.display().to_string().into());
                    }
                });
            }
        });
    });
    let result_ui = weak.clone();
    window.on_open_result(move || {
        if let Some(window) = result_ui.upgrade() {
            let path = window.get_result_path().to_string();
            if let Some(parent) = PathBuf::from(path).parent() {
                let _ = webbrowser::open(parent.to_string_lossy().as_ref());
            }
        }
    });
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
                3 => ArchiveFormat::None,
                _ => ArchiveFormat::Tar,
            };
            let compression = match format_index {
                1 => match compression_index {
                    0 => Compression::Gzip,
                    _ => Compression::Zstd,
                },
                2 => Compression::None,
                3 => match compression_index {
                    0 => Compression::Gzip,
                    1 => Compression::Zstd,
                    2 => Compression::Lz4,
                    3 => Compression::Xz,
                    4 => Compression::Zlib,
                    5 => Compression::Brotli,
                    6 => Compression::Snappy,
                    _ => Compression::Bzip2,
                },
                _ => match compression_index {
                    1 => Compression::Gzip,
                    2 => Compression::None,
                    3 => Compression::Lzip,
                    4 => Compression::Lzma,
                    5 => Compression::Lzo,
                    6 => Compression::Lrzip,
                    7 => Compression::Xz,
                    _ => Compression::Zstd,
                },
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
            let password_policy = weak
                .upgrade()
                .map(|window| PasswordPolicy {
                    minimum_length: window
                        .get_password_policy_minimum_length()
                        .parse()
                        .unwrap_or(8),
                    require_uppercase: window.get_password_policy_uppercase(),
                    require_lowercase: window.get_password_policy_lowercase(),
                    require_number: window.get_password_policy_number(),
                    require_special: window.get_password_policy_special(),
                })
                .unwrap_or_default();
            let config = Config {
                repository: repo,
                output,
                format,
                compression,
                report,
                include_untracked: untracked,
                max_file_size: max_size.parse().unwrap_or(10 * 1024 * 1024),
                excludes: exclude
                    .lines()
                    .map(str::trim)
                    .filter(|pattern| !pattern.is_empty())
                    .map(str::to_owned)
                    .collect(),
                includes: include
                    .lines()
                    .map(str::trim)
                    .filter(|pattern| !pattern.is_empty())
                    .map(str::to_owned)
                    .collect(),
                redact,
                fail_on_secret: fail_secret,
                dry_run,
                password: if password.is_empty() {
                    None
                } else {
                    Some(password.to_string())
                },
                password_policy,
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
            let cancel = cancel_token.clone();
            cancel.store(false, Ordering::Relaxed);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = ui.upgrade() {
                    window.set_running(true);
                    window.set_status("Sanitizing…".into());
                }
            });
            std::thread::spawn(move || {
                let progress_ui = final_ui.clone();
                let result_path = config.output.clone();
                let result = run_with_progress(
                    config,
                    move |event| {
                        let status = match event {
                            ProgressEvent::Scanning { examined, .. } => {
                                format!("Scanning… {examined} files")
                            }
                            ProgressEvent::Writing { included } => {
                                format!("Writing… {included} files")
                            }
                            ProgressEvent::Finished => "Finishing…".to_owned(),
                        };
                        let progress_ui = progress_ui.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = progress_ui.upgrade() {
                                w.set_status(status.into());
                            }
                        });
                    },
                    || cancel.load(Ordering::Relaxed),
                );
                let status = match result {
                    Ok(s) => format!(
                        "Complete: {} files, {} redactions",
                        s.included, s.redactions
                    ),
                    Err(e) => format!("Error: {e:#}"),
                };
                let succeeded = status.starts_with("Complete:");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = final_ui.upgrade() {
                        window.set_running(false);
                        if succeeded {
                            window.set_result_path(result_path.display().to_string().into());
                        }
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

#[cfg(test)]
mod tests {
    use super::result_path_for_outcome;
    use std::path::Path;

    #[test]
    fn failed_run_clears_result_path() {
        assert_eq!(result_path_for_outcome(Path::new("bundle.tar.zst"), false), "");
        assert_eq!(
            result_path_for_outcome(Path::new("bundle.tar.zst"), true),
            "bundle.tar.zst"
        );
    }
}
