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
    let app = apps
        .into_iter()
        .find(|app| app.name.to_ascii_lowercase().contains("rustrepo"))
        .expect("Slint application must be discoverable through AT-SPI");
    let tree = app.dump(Some(4)).expect("AT-SPI tree must be readable");
    assert!(tree.contains("Rustrepo Sanitizer"));
    app.locator(r##"[id="repository-path"]"##)
        .type_text(".")
        .expect("repository input must accept semantic text input");
    app.locator(r##"[id="sanitize"]"##)
        .press()
        .expect("sanitize must be semantically activatable");
}
