//! Regression tests for the web UI structure, semantics, and shared size units.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use itsulu_repo_sanitizer::web::routes::build_router;
use itsulu_repo_sanitizer::web::state::AppState;
use tower::ServiceExt;

async fn index_html() -> String {
    itsulu_repo_sanitizer::web::init_executor();
    let root = tempfile::tempdir().unwrap();
    let state = AppState::for_tests(root.path());
    let response = build_router(Arc::new(state))
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn headings_and_labels_use_title_case() {
    let html = index_html().await;
    for text in [
        "Repository Source",
        "Server Local Path",
        "Git URL",
        "Forgejo Repository",
        "GitHub Repository",
        "Branch Or Tag",
        "Maximum File Size",
        "Include Globs",
        "Exclude Globs",
        "Option Reference",
        "Supported Formats",
    ] {
        assert!(html.contains(text), "missing title-cased text: {text}");
    }
}

/// Renders the form after a validation error, where the alert banner appears.
async fn error_html() -> String {
    itsulu_repo_sanitizer::web::init_executor();
    let root = tempfile::tempdir().unwrap();
    let state = AppState::for_tests(root.path());
    let body = concat!(
        "--xyz\r\n",
        "Content-Disposition: form-data; name=\"mode\"\r\n\r\nforgejo\r\n",
        "--xyz\r\n",
        "Content-Disposition: form-data; name=\"forgejo_repo\"\r\n\r\n../etc\r\n",
        "--xyz--\r\n",
    );
    let response = build_router(Arc::new(state))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ui/jobs")
                .header("content-type", "multipart/form-data; boundary=xyz")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn repository_sources_and_branch_field_are_present() {
    let html = index_html().await;
    // Forgejo repository on its own line below the local path, branch to the right.
    let path_at = html.find("id=\"path\"").expect("local path field");
    let forgejo_at = html
        .find("id=\"forgejo-repo\"")
        .expect("forgejo repository field");
    let github_at = html
        .find("id=\"github-repo\"")
        .expect("github repository field");
    let branch_at = html.find("id=\"git-ref\"").expect("branch or tag field");
    assert!(path_at < forgejo_at, "Forgejo field must follow local path");
    assert!(
        forgejo_at < github_at,
        "GitHub field must follow Forgejo field"
    );
    assert!(github_at < branch_at, "branch/tag field must come last");
    // Each field is grouped in its own row.
    assert!(
        html.contains("id=\"forgejo-row\""),
        "Forgejo needs its own row"
    );
    assert!(
        html.contains("id=\"branch-row\""),
        "branch needs its own row"
    );
}

#[tokio::test]
async fn repository_and_tag_fields_do_not_trigger_email_autofill() {
    let html = index_html().await;
    for field in ["path", "url", "forgejo-repo", "github-repo", "git-ref"] {
        let marker = format!("id=\"{field}\"");
        let start = html
            .find(&marker)
            .unwrap_or_else(|| panic!("missing field {field}"));
        let tag_start = html[..start].rfind('<').expect("tag start");
        let tag_end = html[start..].find('>').expect("tag end") + start;
        let tag = &html[tag_start..=tag_end];
        assert!(
            tag.contains("autocomplete=\"off\""),
            "{field} must disable autofill: {tag}"
        );
        assert!(
            !tag.contains("type=\"email\""),
            "{field} must not use an email input: {tag}"
        );
    }
    assert!(
        html.contains("inputmode=\"url\"") || html.contains("inputmode=\"text\""),
        "URL fields should declare a non-email input mode"
    );
}

#[tokio::test]
async fn maximum_file_size_has_a_unit_selector() {
    let html = index_html().await;
    assert!(html.contains("id=\"max_file_size\""), "numeric size field");
    assert!(
        html.contains("id=\"max_file_size_unit\""),
        "unit selector field"
    );
    for unit in ["KiB", "MiB", "GiB"] {
        assert!(
            html.contains(&format!("value=\"{unit}\"")),
            "unit option missing: {unit}"
        );
    }
    // The byte equivalent is carried in a hidden field so the server can parse
    // it without re-deriving the unit.
    assert!(
        html.contains("id=\"max_file_size_bytes\""),
        "byte-equivalent hidden field"
    );
}

#[tokio::test]
async fn supported_formats_sits_inside_the_output_section() {
    let html = index_html().await;
    let output_at = html
        .find(">Output<")
        .or_else(|| html.find("\"Output\""))
        .expect("Output section");
    let formats_at = html
        .find("Supported Formats")
        .expect("Supported Formats heading");
    let size_at = html
        .find("Maximum File Size")
        .expect("Maximum File Size heading");
    assert!(
        output_at < size_at && size_at < formats_at,
        "Supported Formats must follow Maximum File Size inside Output"
    );
}

#[tokio::test]
async fn top_navigation_offers_sanitize_and_option_reference() {
    let html = index_html().await;
    assert!(
        html.contains("id=\"nav\""),
        "a top navigation landmark is required"
    );
    assert!(html.contains("href=\"#sanitize\""), "Sanitize nav entry");
    assert!(
        html.contains("href=\"#option-reference\""),
        "Option Reference nav entry"
    );
    assert!(html.contains("id=\"sanitize\""), "Sanitize section");
    assert!(
        html.contains("id=\"option-reference\""),
        "Option Reference section"
    );
}

#[tokio::test]
async fn option_reference_fields_have_hover_tooltips() {
    let html = index_html().await;
    assert!(
        html.contains("tooltip"),
        "the option reference needs tooltip elements"
    );
    assert!(
        html.contains("2s") || html.contains("2000ms"),
        "tooltips must appear after a 2 second hover"
    );
    // Every documented field carries a one-line description.
    for field in ["repository", "output", "format", "max_file_size"] {
        assert!(
            html.contains(&format!("id=\"tip-{field}\"")),
            "missing tooltip for {field}"
        );
    }
}

#[tokio::test]
async fn filters_offer_common_globs_and_an_add_button() {
    let html = index_html().await;
    assert!(
        html.contains("id=\"include-choice\""),
        "include preset dropdown"
    );
    assert!(
        html.contains("id=\"exclude-choice\""),
        "exclude preset dropdown"
    );
    assert!(html.contains("id=\"include-add\""), "include Add button");
    assert!(html.contains("id=\"exclude-add\""), "exclude Add button");
    assert!(
        html.contains("id=\"include-entry\""),
        "custom include entry"
    );
    assert!(
        html.contains("id=\"exclude-entry\""),
        "custom exclude entry"
    );
    // Presets mirror the desktop GUI options.
    for glob in [
        "docs/**/*.md",
        "src/**/*.rs",
        "target/**",
        "node_modules/**",
    ] {
        assert!(html.contains(glob), "missing common glob: {glob}");
    }
}

#[tokio::test]
async fn dropdown_options_are_readable_in_both_themes() {
    let html = index_html().await;
    assert!(
        html.contains("option,") || html.contains("option {"),
        "options need explicit colors"
    );
    assert!(
        html.contains("color-scheme"),
        "color scheme must be declared"
    );
    assert!(
        html.contains("select option"),
        "select options must be styled so unselected entries stay readable"
    );
}

#[tokio::test]
async fn filter_lists_are_submitted_from_hidden_fields() {
    let html = index_html().await;
    // The Add buttons drive a list, so the applied patterns are carried in the
    // newline separated fields the server already parses.
    assert!(
        html.contains("id=\"include-globs\" name=\"includes\""),
        "include list must be submitted as includes"
    );
    assert!(
        html.contains("id=\"exclude-globs\" name=\"excludes\""),
        "exclude list must be submitted as excludes"
    );
}

#[tokio::test]
async fn custom_pattern_fields_and_add_buttons_are_labelled() {
    let html = index_html().await;
    assert!(
        html.contains("for=\"include-entry\""),
        "the custom include field needs a label"
    );
    assert!(
        html.contains("for=\"exclude-entry\""),
        "the custom exclude field needs a label"
    );
    // Two indistinguishable "Add" buttons are unusable with a screen reader.
    assert!(
        html.contains("aria-label=\"Add include glob\""),
        "include Add button needs a distinct name"
    );
    assert!(
        html.contains("aria-label=\"Add exclude glob\""),
        "exclude Add button needs a distinct name"
    );
}

#[tokio::test]
async fn supported_formats_uses_title_cased_cells() {
    let html = index_html().await;
    assert!(
        html.contains("<td>Yes</td>"),
        "password column must be title case"
    );
    assert!(
        html.contains("<td>No</td>"),
        "password column must be title case"
    );
}

#[tokio::test]
async fn status_banners_keep_their_style_class() {
    // A validation error re-renders the form, so the banner is the only place
    // the class is observable.
    let html = error_html().await;
    // Leptos drops a static `class` merged with a dynamic one, which would
    // leave the banner unstyled.
    assert!(
        html.contains("class=\"status\""),
        "status banners must keep the status class"
    );
}

#[tokio::test]
async fn global_enhancement_scripts_are_present() {
    let html = index_html().await;
    assert!(html.contains("max_file_size_bytes"), "size sync script");
    assert!(html.contains("include-add"), "glob list script");
    assert!(html.contains("exclude-add"), "glob list script");
}
