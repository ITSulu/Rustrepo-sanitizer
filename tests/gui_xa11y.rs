//! Native semantic smoke test. Run with a graphical session and AT-SPI:
//! `cargo test --test gui_xa11y -- --ignored --nocapture`.
#[test]
#[ignore = "requires a running native GUI, D-Bus, and AT-SPI"]
fn discovers_slint_controls_semantically() {
    use xa11y::{App, AppExt};
    let apps = App::list().expect("AT-SPI application list must be readable");
    eprintln!(
        "AT-SPI applications: {:?}",
        apps.iter().map(|app| &app.name).collect::<Vec<_>>()
    );
    let expected_pid = std::env::var("RRS_GUI_PID")
        .ok()
        .and_then(|pid| pid.parse::<u32>().ok());
    let app = apps
        .into_iter()
        .find(|app| {
            expected_pid.map_or_else(
                || app.name.to_ascii_lowercase().contains("rustrepo"),
                |pid| app.pid == Some(pid),
            )
        })
        .expect("Slint application must be discoverable through AT-SPI");
    assert!(app.name.to_ascii_lowercase().contains("rustrepo"));
    app.locator(r##"text_field[name="Local path"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("repository input must be semantically discoverable");
    app.locator(r##"button[name="Browse for repository"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Browse control must be semantically discoverable");
    app.locator(r##"text_field[name="Output file path"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Output File input must be semantically discoverable");
    app.locator(r##"button[name="Browse for output folder"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Output Browse control must be semantically discoverable");
    let output_bounds = || {
        app.locator(r##"text_field[name="Output file path"]"##)
            .element()
            .expect("Output File control is accessible")
            .bounds
            .expect("Output File control has screen bounds")
    };
    let browse_bounds = || {
        app.locator(r##"button[name="Browse for output folder"]"##)
            .element()
            .expect("Browse control is accessible")
            .bounds
            .expect("Browse control has screen bounds")
    };
    let output = output_bounds();
    let browse = browse_bounds();
    assert!(
        output.width >= 400,
        "Output File field must use available row width: {output:?}"
    );
    assert!(
        output.x + output.width as i32 <= browse.x + 2,
        "Output File field must end at the Browse control: output={output:?}, browse={browse:?}"
    );
    app.locator(r##"combo_box[name="Archive format"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Archive selector must be semantically discoverable");
    app.locator(r##"combo_box[name="Compression"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Compression selector must be semantically discoverable");
    let tree = app.dump(Some(8)).expect("AT-SPI tree must be readable");
    assert!(
        tree.contains("Help"),
        "desktop Help menu must be represented in the accessibility tree"
    );
    assert!(
        tree.contains("Settings"),
        "desktop Settings menu must be represented in the accessibility tree"
    );
    assert!(
        tree.contains("About"),
        "desktop About menu must be represented in the accessibility tree"
    );
    app.locator(r##"button[name="Cancel sanitization"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Cancel control must be semantically discoverable");
    app.locator(r##"check_box[name="Advanced options"]"##)
        .press()
        .expect("Advanced options must be semantically activatable");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let include_glob = app.locator(r##"text_field[name="Custom include glob"]"##);
    include_glob
        .scroll_into_view()
        .expect("Include Glob editor must be scrollable into view");
    include_glob
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Include Glob editor must be semantically discoverable");
    app.locator(r##"button[name="Add include glob"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Glob Add controls must be semantically discoverable");
    app.locator(r##"check_box[name="Advanced options"]"##)
        .press()
        .expect("Advanced options must be closable semantically");
    app.locator(r##"button[name="Sanitize repository"]"##)
        .press()
        .expect("sanitize must be semantically activatable");
    // The status is reported through the accessible tree, so poll until the run
    // reports progress or settles rather than assuming a fixed delay. A busy
    // runner updates the status more slowly than a local one, and the
    // in-progress states are as valid as a settled one.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let tree = app.dump(Some(4)).expect("AT-SPI tree must remain readable");
        if tree.contains("Sanitizing")
            || tree.contains("Scanning")
            || tree.contains("Complete")
            || tree.contains("Error")
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "sanitization action must update the accessible status: {tree}"
        );
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    // Slint currently exposes its native menu labels as non-actionable static
    // text in AT-SPI. Verify the AboutWindow's actual metadata values through
    // its accessible text labels in this app tree.
    let about = app
        .dump(Some(12))
        .expect("About window must remain accessible");
    for content in [
        "Rustrepo-sanitizer",
        "Version 0.6.3",
        "Apache License 2.0",
        "Slint is used under its applicable selected Slint license.",
        "implementation support provided by AI agents including OpenAI Codex",
    ] {
        assert!(
            about.contains(content),
            "About must expose {content}: {about}"
        );
    }
}
