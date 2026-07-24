# F-Droid distribution

Keepbook publishes an F-Droid repository from its own GitHub Pages site. It
indexes the signed APKs already attached to GitHub releases, so a build is never
duplicated and an install from the repo upgrades in place from a manually
installed APK.

- Landing page: <https://colonelpanic8.github.io/keepbook/>
- Repository address: `https://colonelpanic8.github.io/keepbook/fdroid/repo`

## How publication works

`.github/workflows/fdroid-repo.yml` runs after **Release App Artifacts**
succeeds, so the release's APK exists before it is indexed. The workflow:

1. Installs `fdroidserver` from PyPI.
2. Runs `scripts/fdroid/build-repo.sh`, which
   - regenerates the fastlane changelogs from `CHANGELOG.md`,
   - lays out `fdroid/config.yml`, `fdroid/metadata/`, and the shared
     `fastlane/metadata/android/` listing into a scratch directory,
   - downloads the signed APKs from the most recent releases,
   - records their versions via `scripts/fdroid/apply-versions.py`, and
   - runs `fdroid update` to produce a signed index.
3. Assembles `site/` (the repo plus a landing page) and deploys it to Pages.

No manual step is involved: publishing a release is enough.

### Required repository setup

- **GitHub Pages** must be enabled with *Source: GitHub Actions*.
- The existing release-signing secrets are reused to sign the repo index:
  `ANDROID_SIGNING_KEYSTORE_BASE64`, `ANDROID_SIGNING_KEY_ALIAS`,
  `ANDROID_SIGNING_KEYSTORE_PASSWORD`, `ANDROID_SIGNING_KEY_PASSWORD`.

The signing fingerprint clients should verify is printed in each run's job
summary and rendered on the landing page.

### Retention

Each deployment replaces the whole Pages site, so every version to be kept is
re-collected on every run. `FDROID_RELEASE_COUNT` (default 4) bounds how many
recent releases are indexed; at roughly 80 MB per APK this keeps the site well
under the 1 GB Pages limit. Older versions stay available on the GitHub
releases page.

## Version codes

`androidVersionCode` in `flake.nix` is derived from `appVersion` as
`major * 10000 + minor * 100 + patch` (0.10.0 becomes 1000). Bumping the version
is therefore the only step needed, and duplicate version codes — which an
F-Droid index rejects — cannot be introduced by forgetting to update a second
constant.

## Store listing

`fastlane/metadata/android/` is the single source for the app title, summary,
description, icon, and per-version release notes. It is the layout F-Droid,
IzzyOnDroid, and the self-hosted repo all read, so the listing stays identical
across them.

Changelogs are generated from `CHANGELOG.md` rather than hand-written:

```bash
just fdroid-changelogs           # regenerate
just fdroid-changelogs --check   # verify (CI runs this)
```

Entries are keyed by version code and trimmed to the 500-character limit F-Droid
applies to release notes.

## Building the repository locally

Requires `fdroidserver`, `gh`, and an Android SDK providing `apksigner`.

```bash
python3 -m venv /tmp/fdroid-venv
/tmp/fdroid-venv/bin/pip install fdroidserver
export PATH="/tmp/fdroid-venv/bin:$PATH"

# A throwaway key is fine for a local index; only CI signs the published one.
keytool -genkeypair -keystore /tmp/test.p12 -storetype PKCS12 -alias testkey \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -storepass testpass -keypass testpass -dname "CN=Keepbook Test"

nix develop .#android --command env \
  FDROID_KEYSTORE_FILE=/tmp/test.p12 \
  FDROID_KEY_ALIAS=testkey \
  FDROID_KEYSTORE_PASSWORD=testpass \
  FDROID_KEY_PASSWORD=testpass \
  FDROID_RELEASE_COUNT=2 \
  just fdroid-repo
```

The result lands in `target/fdroid`. Serving that directory and pointing an
F-Droid client at `<url>/repo` exercises the real client path.

Note that `nixpkgs#fdroidserver` currently fails to build, which is why the
workflow and these instructions install it from PyPI.

## Other distribution channels

The self-hosted repo is the one Keepbook controls end to end. Two other routes
build on the same metadata:

**IzzyOnDroid** pulls the maintainer-signed APKs straight from GitHub releases
and reads `fastlane/metadata/android/`, so no build recipe is needed. Keepbook
already satisfies its requirements — an OSI license, no proprietary
dependencies, and no trackers. Getting listed is a submission request against
their repository.

**The F-Droid main repository** builds from source on its own Debian
buildserver, where Nix is unavailable. That requires work this setup does not
provide:

- a non-Nix Android build script that uses the buildserver's SDK/NDK and builds
  `dioxus-cli` from source, replacing `nix run .#dioxus-android-release`;
- the `dx`-generated Gradle project vendored into the repository instead of
  generated and `sed`-patched at build time (see `flake.nix`), so the recipe can
  be an ordinary Gradle build;
- a decision on signing. F-Droid signs with its own key by default, which means
  users cannot upgrade in place from an APK installed from GitHub releases or
  from this repo. Avoiding that requires reproducible builds so F-Droid can
  verify and ship the maintainer signature.

The listing would carry the `NonFreeNet` anti-feature, as it does here, because
account syncing and price refresh talk to proprietary financial services.
