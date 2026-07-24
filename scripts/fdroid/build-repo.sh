#!/usr/bin/env bash
#
# Assemble the self-hosted Keepbook F-Droid repository.
#
# Collects the signed release APKs from recent GitHub releases, lays out the
# fdroidserver directory structure using fdroid/config.yml, fdroid/metadata, and
# the shared fastlane metadata, then runs `fdroid update` to produce a signed
# index. The resulting directory is what gets deployed to GitHub Pages.
#
# Environment:
#   FDROID_OUT_DIR         Output directory (default: target/fdroid)
#   FDROID_RELEASE_COUNT   How many recent releases to index (default: 4). Each
#                          APK is roughly 80 MB and every published version is
#                          re-uploaded on each deploy, so this trades version
#                          history against the GitHub Pages 1 GB site limit.
#   FDROID_KEYSTORE_BASE64 Base64 PKCS#12 keystore used to sign the index
#   FDROID_KEYSTORE_FILE   Path to that keystore, as an alternative to the above
#   FDROID_KEY_ALIAS
#   FDROID_KEYSTORE_PASSWORD
#   FDROID_KEY_PASSWORD
#   GH_TOKEN               Token for `gh release` access
#
# The ANDROID_SIGNING_* variables already used by the release workflow are
# accepted as fallbacks, since the index is signed with the same keystore.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

out_dir="${FDROID_OUT_DIR:-target/fdroid}"
release_count="${FDROID_RELEASE_COUNT:-4}"
app_id="org.colonelpanic.keepbook.dioxus"
fastlane_root="fastlane/metadata/android"

# Fall back to the release-signing credentials; the repo index is signed with
# the same key so that installs from this repo and from GitHub releases are
# interchangeable.
: "${FDROID_KEYSTORE_BASE64:=${ANDROID_SIGNING_KEYSTORE_BASE64:-}}"
: "${FDROID_KEYSTORE_FILE:=${ANDROID_SIGNING_KEYSTORE_FILE:-}}"
: "${FDROID_KEY_ALIAS:=${ANDROID_SIGNING_KEY_ALIAS:-}}"
: "${FDROID_KEYSTORE_PASSWORD:=${ANDROID_SIGNING_KEYSTORE_PASSWORD:-}}"
: "${FDROID_KEY_PASSWORD:=${ANDROID_SIGNING_KEY_PASSWORD:-}}"
export FDROID_KEY_ALIAS FDROID_KEYSTORE_PASSWORD FDROID_KEY_PASSWORD

if [[ -z "$FDROID_KEYSTORE_BASE64" && -z "$FDROID_KEYSTORE_FILE" ]]; then
  echo "A signing keystore is required: set FDROID_KEYSTORE_BASE64 or FDROID_KEYSTORE_FILE" >&2
  exit 1
fi

for var in FDROID_KEY_ALIAS FDROID_KEYSTORE_PASSWORD FDROID_KEY_PASSWORD; do
  if [[ -z "${!var}" ]]; then
    echo "Signing the repo index requires $var" >&2
    exit 1
  fi
done

for tool in fdroid gh python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Required tool not found on PATH: $tool" >&2
    exit 1
  fi
done

# fdroidserver signs the v2 index with apksigner, which it locates through
# ANDROID_HOME. Failing here beats failing after every APK has been downloaded.
if ! command -v apksigner >/dev/null 2>&1; then
  shopt -s nullglob
  sdk_apksigners=("${ANDROID_HOME:-/nonexistent}"/build-tools/*/apksigner)
  shopt -u nullglob

  if [[ "${#sdk_apksigners[@]}" -eq 0 ]]; then
    echo "apksigner not found: put it on PATH or set ANDROID_HOME to an SDK with build-tools" >&2
    exit 1
  fi
fi

echo "==> Refreshing fastlane changelogs from CHANGELOG.md"
python3 scripts/fdroid/changelogs.py

echo "==> Preparing $out_dir"
rm -rf "$out_dir"
mkdir -p "$out_dir/repo" "$out_dir/metadata"

cp fdroid/config.yml "$out_dir/config.yml"
cp fdroid/metadata/*.yml "$out_dir/metadata/"

# fdroidserver reads localized listings from metadata/<appid>/<locale>/, which
# is the same layout fastlane uses. Sharing one source keeps the self-hosted
# repo, IzzyOnDroid, and the F-Droid main repo listings identical.
for locale_dir in "$fastlane_root"/*/; do
  [[ -d "$locale_dir" ]] || continue
  locale="$(basename "$locale_dir")"
  mkdir -p "$out_dir/metadata/$app_id"
  cp -r "$locale_dir" "$out_dir/metadata/$app_id/$locale"
done

# `repo_icon` is resolved relative to the fdroid root; fdroid update copies it
# into repo/icons/ itself and wipes anything already placed there.
if [[ -f "$fastlane_root/en-US/images/icon.png" ]]; then
  cp "$fastlane_root/en-US/images/icon.png" "$out_dir/icon.png"
fi

echo "==> Collecting signed APKs from the $release_count most recent releases"
mapfile -t tags < <(
  gh release list --limit "$release_count" --json tagName,isDraft,isPrerelease \
    --jq '.[] | select(.isDraft == false and .isPrerelease == false) | .tagName'
)

if [[ "${#tags[@]}" -eq 0 ]]; then
  echo "No published GitHub releases were found" >&2
  exit 1
fi

download_dir="$(mktemp -d)"
trap 'rm -rf "$download_dir"' EXIT

apk_count=0
for tag in "${tags[@]}"; do
  tag_dir="$download_dir/$tag"
  mkdir -p "$tag_dir"

  # Older releases predate the Android artifacts, so a miss here is expected.
  if ! gh release download "$tag" --pattern '*-signed.apk' --dir "$tag_dir" 2>/dev/null; then
    echo "  $tag: no signed APK asset, skipping"
    continue
  fi

  shopt -s nullglob
  apks=("$tag_dir"/*-signed.apk)
  shopt -u nullglob

  for apk in "${apks[@]}"; do
    # Release assets share a filename across tags, so namespace by tag to keep
    # every version present in the index.
    dest="$out_dir/repo/keepbook-${tag}-$(basename "$apk")"
    cp "$apk" "$dest"
    echo "  $tag: $(basename "$dest")"
    apk_count=$((apk_count + 1))
  done
done

if [[ "$apk_count" -eq 0 ]]; then
  echo "No signed APKs were found in the last $release_count releases" >&2
  exit 1
fi

echo "==> Recording collected versions in the app metadata"
python3 scripts/fdroid/apply-versions.py "$out_dir" "$app_id"

echo "==> Installing index signing keystore"
keystore="$out_dir/keystore.p12"
if [[ -n "$FDROID_KEYSTORE_FILE" ]]; then
  cp "$FDROID_KEYSTORE_FILE" "$keystore"
else
  printf '%s' "$FDROID_KEYSTORE_BASE64" | base64 -d > "$keystore"
fi
chmod 600 "$keystore"

echo "==> Running fdroid update ($apk_count APK(s))"
(
  cd "$out_dir"
  fdroid update --pretty --verbose
)

# Clients pin a repo by the SHA-256 of its signing certificate, so the URL to
# hand out is repo_url?fingerprint=<this>. Best effort: keytool is present in CI
# but is not worth failing the build over.
fingerprint=""
if command -v keytool >/dev/null 2>&1; then
  fingerprint="$(
    keytool -list -v \
      -keystore "$keystore" \
      -storetype PKCS12 \
      -alias "$FDROID_KEY_ALIAS" \
      -storepass "$FDROID_KEYSTORE_PASSWORD" 2>/dev/null \
      | sed -n 's/^[[:space:]]*SHA256:[[:space:]]*//p' \
      | head -1 \
      | tr -d ':' \
      | tr '[:upper:]' '[:lower:]'
  )"
fi

# The keystore must not reach the deployed site.
rm -f "$keystore"

echo
echo "F-Droid repository built in $out_dir"
echo "  APKs indexed: $apk_count"
if [[ -n "$fingerprint" ]]; then
  echo "  Fingerprint:  $fingerprint"
  printf '%s\n' "$fingerprint" > "$out_dir/fingerprint.txt"
fi
