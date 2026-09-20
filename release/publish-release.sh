#!/usr/bin/env bash
set -euo pipefail

version=${1:?usage: publish-release.sh VERSION ARTIFACT_DIR}
assets=${2:?usage: publish-release.sh VERSION ARTIFACT_DIR}
tag="v${version}"
target_commit=${RELEASE_COMMIT:-$(git rev-parse HEAD)}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "invalid version" >&2; exit 2; }
: "${FORGEJO_TOKEN:?FORGEJO_TOKEN is required}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"
[[ -d "$assets" ]] || { echo "artifact directory missing" >&2; exit 1; }
mapfile -t files < <(find "$assets" -maxdepth 1 -type f ! -name SHA256SUMS -printf '%f\n' | sort)
[[ ${#files[@]} -gt 0 ]] || { echo "no release assets" >&2; exit 1; }
[[ -f "$assets/SHA256SUMS" ]] || { echo "SHA256SUMS missing" >&2; exit 1; }
mapfile -t manifest_files < <(awk '{print $2}' "$assets/SHA256SUMS" | sort -u)
[[ "${files[*]}" == "${manifest_files[*]}" ]] || { echo "SHA256SUMS does not exactly cover release assets" >&2; exit 1; }
if [[ -n "${RELEASE_EXPECTED_ASSETS:-}" ]]; then
  mapfile -t expected_files < <(printf '%s\n' "$RELEASE_EXPECTED_ASSETS" | tr ',' '\n' | sed '/^$/d' | sort -u)
  [[ "${files[*]}" == "${expected_files[*]}" ]] || { echo "release asset set does not match RELEASE_EXPECTED_ASSETS" >&2; exit 1; }
fi
(cd "$assets" && sha256sum -c SHA256SUMS)
upload_files=("${files[@]}" SHA256SUMS)

forgejo_api=https://git.itsulu.com/api/v1/repos/itsulu/Rustrepo-sanitizer
github_api=https://api.github.com/repos/ITSulu/Rustrepo-sanitizer
notes=${RELEASE_NOTES_FILE:-}
if [[ -z "$notes" && -f "release/${version}.md" ]]; then
  notes="release/${version}.md"
fi
if [[ -n "$notes" ]]; then
  notes=$(<"$notes")
else
  notes=$(cat <<EOF
# Rustrepo-sanitizer ${version}

## Summary

This release contains the versioned Rustrepo-sanitizer command-line and GUI
build from the exact immutable v${version} tag. It includes the repository
sanitization pipeline, deterministic archive generation, redaction safeguards,
reports, and the platform packaging produced by the release workflow.

## Verification and downloads

Forgejo Actions built and checked the exact tag. The published Linux x86_64
archive, Debian package, and RPM package are listed in SHA256SUMS. Verify the
manifest before installing or extracting an asset. The same assets and
manifest are mirrored on Forgejo and GitHub.
EOF
)
fi
cfg=$(mktemp); chmod 600 "$cfg"; trap 'rm -f "$cfg"' EXIT
configure() { printf 'header = "Authorization: %s %s"\n' "$1" "$2" > "$cfg"; }
get() { curl -fsS --config "$cfg" "$1"; }
retry() {
  # Release hosts occasionally fail DNS/connectivity; retry transient errors.
  local attempt
  for attempt in 1 2 3 4 5; do
    if "$@"; then return 0; fi
    sleep 5
  done
  return 1
}
ensure_release() {
  local api=$1 scheme=$2 token=$3; configure "$scheme" "$token"
  local body code id payload
  payload=$(jq -n --arg t "$tag" --arg c "$target_commit" --arg b "$notes" \
    '{tag_name:$t,name:$t,target_commitish:$c,body:$b,draft:false,prerelease:false}')
  body=$(mktemp)
  code=$(curl -sS --config "$cfg" -o "$body" -w '%{http_code}' "$api/releases/tags/$tag")
  if [[ "$code" == 200 ]]; then
    id=$(jq -r '.id' <"$body")
  elif [[ "$code" == 404 ]]; then
    # The tag endpoint hides draft releases, so a draft for this tag can exist
    # while the lookup reports 404. Reuse that draft instead of creating a
    # second release for the same tag.
    id=$(curl -fsS --config "$cfg" "$api/releases?per_page=100&limit=100" \
      | jq -r --arg t "$tag" '[.[] | select(.tag_name == $t)][0].id // empty')
  else
    cat "$body" >&2; rm -f "$body"; return 1
  fi
  rm -f "$body"
  if [[ -n "$id" ]]; then
    curl -fsS --config "$cfg" -H 'Content-Type: application/json' -X PATCH "$api/releases/$id" --data "$payload" >/dev/null
  else
    curl -fsS --config "$cfg" -H 'Content-Type: application/json' -X POST "$api/releases" --data "$payload" >/dev/null
  fi
}
publish_assets() {
  local api=$1 scheme=$2 token=$3 upload=$4 forgejo=$5; configure "$scheme" "$token"
  local release id name file expected asset_id url remote actual
  release=$(get "$api/releases/tags/$tag"); id=$(jq -r '.id' <<<"$release")
  for name in "${upload_files[@]}"; do
    file="$assets/$name"; expected=$(awk -v n="$name" '$2 == n {print $1}' "$assets/SHA256SUMS")
    asset_id=$(jq -r --arg n "$name" '.assets[] | select(.name == $n) | .id' <<<"$release" | head -1)
    if [[ -n "$asset_id" ]]; then
      url=$(jq -r --arg n "$name" '.assets[] | select(.name == $n) | .browser_download_url' <<<"$release")
      remote=$(mktemp); retry curl -fsS -L "$url" -o "$remote"
      if [[ "$name" == SHA256SUMS ]]; then
        cmp -s "$remote" "$assets/SHA256SUMS" && { rm -f "$remote"; continue; }
      else
        actual=$(sha256sum "$remote" | awk '{print $1}')
        [[ "$actual" == "$expected" ]] && { rm -f "$remote"; continue; }
      fi
      rm -f "$remote"
      if [[ "$forgejo" == true ]]; then
        retry curl -fsS --config "$cfg" -X DELETE "$api/releases/$id/assets/$asset_id" >/dev/null
      else
        retry curl -fsS --config "$cfg" -X DELETE "https://api.github.com/repos/ITSulu/Rustrepo-sanitizer/releases/assets/$asset_id" >/dev/null
      fi
    fi
    if [[ "$forgejo" == true ]]; then
      retry curl -fsS --config "$cfg" -F "attachment=@$file" "$upload/$id/assets?name=$name" >/dev/null
    else
      retry curl -fsS --config "$cfg" -H 'Content-Type: application/octet-stream' --data-binary "@$file" "$upload/$id/assets?name=$name" >/dev/null
    fi
  done
}
prune_assets() {
  local api=$1 scheme=$2 token=$3 forgejo=$4; configure "$scheme" "$token"
  local release id name asset_id keep u
  release=$(get "$api/releases/tags/$tag"); id=$(jq -r '.id' <<<"$release")
  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    keep=false
    for u in "${upload_files[@]}"; do
      [[ "$name" == "$u" ]] && keep=true
    done
    [[ "$keep" == true ]] && continue
    asset_id=$(jq -r --arg n "$name" '.assets[] | select(.name == $n) | .id' <<<"$release" | head -1)
    [[ -n "$asset_id" ]] || continue
    if [[ "$forgejo" == true ]]; then
      retry curl -fsS --config "$cfg" -X DELETE "$api/releases/$id/assets/$asset_id" >/dev/null
    else
      retry curl -fsS --config "$cfg" -X DELETE "https://api.github.com/repos/ITSulu/Rustrepo-sanitizer/releases/assets/$asset_id" >/dev/null
    fi
  done < <(jq -r '.assets[].name' <<<"$release")
}

ensure_release "$forgejo_api" token "$FORGEJO_TOKEN"
ensure_release "$github_api" Bearer "$GITHUB_TOKEN"
publish_assets "$forgejo_api" token "$FORGEJO_TOKEN" "$forgejo_api/releases" true
publish_assets "$github_api" Bearer "$GITHUB_TOKEN" "https://uploads.github.com/repos/ITSulu/Rustrepo-sanitizer/releases" false
prune_assets "$forgejo_api" token "$FORGEJO_TOKEN" true
prune_assets "$github_api" Bearer "$GITHUB_TOKEN" false
remote_matches() {
  # After an asset is replaced, downloads (and cached release metadata) can keep
  # serving the previous copy for several minutes. Re-resolve the asset each
  # attempt and keep retrying until the mirror reports the expected content.
  local api=$1 name=$2 expected=$3 attempt release url dest actual
  for attempt in $(seq 1 20); do
    release=$(get "$api/releases/tags/$tag")
    if ! jq -e --arg n "$name" '.assets[] | select(.name == $n)' <<<"$release" >/dev/null; then
      sleep 15; continue
    fi
    url=$(jq -r --arg n "$name" '.assets[] | select(.name == $n) | .browser_download_url' <<<"$release")
    dest=$(mktemp)
    if curl -fsS -L "$url" -o "$dest"; then
      if [[ "$name" == SHA256SUMS ]]; then
        cmp -s "$dest" "$assets/SHA256SUMS" && { rm -f "$dest"; return 0; }
      else
        actual=$(sha256sum "$dest" | awk '{print $1}')
        [[ "$actual" == "$expected" ]] && { rm -f "$dest"; return 0; }
      fi
    fi
    rm -f "$dest"
    sleep 15
  done
  return 1
}
verify_release() {
  local api=$1 scheme=$2 token=$3 repo_url=$4; configure "$scheme" "$token"; release=$(get "$api/releases/tags/$tag")
  jq -e --arg t "$tag" '.tag_name == $t and (.draft|not) and (.prerelease|not)' <<<"$release" >/dev/null
  remote_commit=$(git ls-remote "$repo_url" "refs/tags/$tag^{}" | awk 'NR==1 {print $1}')
  # Lightweight tags have no peeled ^{} ref; verify their direct target too.
  if [[ -z "$remote_commit" ]]; then
    remote_commit=$(git ls-remote "$repo_url" "refs/tags/$tag" | awk 'NR==1 {print $1}')
  fi
  [[ "$remote_commit" == "$target_commit" ]] || { echo "remote tag commit $remote_commit != $target_commit" >&2; exit 1; }
  for name in "${upload_files[@]}"; do
    expected=$(awk -v n="$name" '$2 == n {print $1}' "$assets/SHA256SUMS")
    remote_matches "$api" "$name" "$expected" || { echo "checksum mismatch for $name" >&2; exit 1; }
  done
}
verify_release "$forgejo_api" token "$FORGEJO_TOKEN" https://git.itsulu.com/itsulu/Rustrepo-sanitizer.git
verify_release "$github_api" Bearer "$GITHUB_TOKEN" https://github.com/ITSulu/Rustrepo-sanitizer.git
