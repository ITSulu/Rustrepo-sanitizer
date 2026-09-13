# Release process

All ITSulu software CI, review gates, release builds, and release orchestration
run through ITSulu Forgejo. GitHub is a downstream source and release mirror
only. Forgejo runners and Garuda workstations produce and validate artifacts;
GitHub Actions and GitHub-hosted runners are not part of this process.

Create an annotated `vX.Y.Z` tag from merged Forgejo main, verify the exact tag
and binary version, build artifacts with `release/build-artifacts.sh`, verify
`SHA256SUMS`, and publish the directory with `release/publish-release.sh`.
The publisher reconciles both release objects and verifies public download
hashes; rerunning it retains matching assets and repairs missing or mismatched
assets.

For 0.3.2, macOS source and Cargo installation are supported. Prebuilt macOS
artifacts are deferred until ITSulu has a native macOS build and test
environment. Native Windows 11 VM validation is planned for future ITSulu CI;
0.3.2 uses a Garuda cross-build validated with Wine.
