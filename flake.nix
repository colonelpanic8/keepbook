{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    fenix,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        fenixPkgs = fenix.packages.${system};
        lib = pkgs.lib;
        appVersion = "0.7.2";
        androidVersionCode = "700";
        appCommit = self.rev or self.dirtyRev or "unknown";
        keepbookDioxusAppId = "org.colonelpanic.keepbook.dioxus";
        keepbookDioxusDesktopAlias = "keepbook-dioxus";
        sourceRoot = ./.;
        cleanSrc = pkgs.lib.cleanSourceWith {
          src = sourceRoot;
          filter = path: type: let
            pathStr = toString path;
            rootStr = toString sourceRoot;
            rel =
              if pathStr == rootStr
              then ""
              else pkgs.lib.removePrefix "${rootStr}/" pathStr;
            base = builtins.baseNameOf pathStr;
          in
            !(builtins.elem base [
              ".git"
              ".github"
              ".direnv"
              ".worktrees"
              ".playwright-cli"
              "target"
              "dist"
              "plans"
              "docs"
              "contracts"
              "scripts"
              ".envrc"
            ])
            && !(pkgs.lib.hasPrefix ".tmp_" base)
            && !(pkgs.lib.hasPrefix ".worktrees/" rel);
        };
        isLinux = pkgs.stdenv.hostPlatform.isLinux;
        isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
        androidRustTargets = lib.optionals isLinux [
          fenixPkgs.targets.aarch64-linux-android.stable.rust-std
          fenixPkgs.targets.x86_64-linux-android.stable.rust-std
        ];
        rustTargets =
          [
            fenixPkgs.targets.wasm32-unknown-unknown.stable.rust-std
          ]
          ++ lib.optionals isDarwin [
            fenixPkgs.targets.aarch64-apple-ios-sim.stable.rust-std
          ]
          ++ androidRustTargets;
        addDarwinInstallNameTool = tool:
          if isDarwin
          then
            tool.overrideAttrs (old: {
              nativeBuildInputs =
                (old.nativeBuildInputs or []) ++ [pkgs.darwin.cctools];
            })
          else tool;
        toolchain = fenixPkgs.combine (map addDarwinInstallNameTool [
            fenixPkgs.stable.cargo
            fenixPkgs.stable.clippy
            fenixPkgs.stable.rust-src
            fenixPkgs.stable.rustc
            fenixPkgs.stable.rustfmt
            fenixPkgs.stable.rust-analyzer
          ]
          ++ rustTargets);
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
        androidBuildToolsVersion = "36.1.0";
        androidCmdLineToolsVersion = "19.0";
        androidCompileSdkVersion = "36";
        androidGradlePluginVersion = "8.13.2";
        androidKotlinPluginVersion = "2.2.21";
        androidNdkVersion = "29.0.14206865";
        androidPlatformToolsVersion = "36.0.2";
        androidTargetSdkVersion = "36";
        androidComposition = pkgs.androidenv.composeAndroidPackages {
          cmdLineToolsVersion = androidCmdLineToolsVersion;
          toolsVersion = "26.1.1";
          platformToolsVersion = androidPlatformToolsVersion;
          buildToolsVersions = ["34.0.0" androidBuildToolsVersion];
          includeEmulator = true;
          # Dioxus still generates an initial Gradle project pinned to older
          # SDKs before the wrapper patches and rebuilds with the latest
          # versions below. Include those generated SDKs in the immutable SDK.
          platformVersions = ["33" "34" androidCompileSdkVersion];
          includeSources = false;
          includeSystemImages = false;
          systemImageTypes = ["google_apis_playstore"];
          abiVersions = ["arm64-v8a" "x86_64"];
          includeNDK = true;
          ndkVersions = [androidNdkVersion];
          cmakeVersions = ["3.22.1"];
          useGoogleAPIs = true;
          useGoogleTVAddOns = false;
        };
        androidHome = "${androidComposition.androidsdk}/libexec/android-sdk";
        androidNdkHome = "${androidHome}/ndk/${androidNdkVersion}";
        androidAapt2 = "${androidHome}/build-tools/${androidBuildToolsVersion}/aapt2";
        androidLlvmBin = "${androidNdkHome}/toolchains/llvm/prebuilt/linux-x86_64/bin";
        android16KbPageRustFlags = "-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384";
        dioxusAndroidEnv = {
          ANDROID_HOME = androidHome;
          ANDROID_SDK_ROOT = androidHome;
          ANDROID_NDK_HOME = androidNdkHome;
          NDK_HOME = androidNdkHome;
          CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${androidLlvmBin}/aarch64-linux-android24-clang";
          CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS = android16KbPageRustFlags;
          CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = "${androidLlvmBin}/x86_64-linux-android24-clang";
          CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS = android16KbPageRustFlags;
          CC_aarch64_linux_android = "${androidLlvmBin}/aarch64-linux-android24-clang";
          CC_x86_64_linux_android = "${androidLlvmBin}/x86_64-linux-android24-clang";
          AR_aarch64_linux_android = "${androidLlvmBin}/llvm-ar";
          AR_x86_64_linux_android = "${androidLlvmBin}/llvm-ar";
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${androidAapt2}";
          JAVA_HOME = pkgs.jdk17.home;
          OPENSSL_NO_VENDOR = "0";
        };
        dioxusLinuxBuildInputs = lib.optionals isLinux [
          pkgs.dbus
          pkgs.glib
          pkgs.gtk3
          pkgs.libappindicator-gtk3
          pkgs.webkitgtk_4_1
          pkgs.xdotool
        ];
        dioxusLinuxLibraryPathInputs = lib.optionals isLinux [
          pkgs.cairo
          pkgs.gdk-pixbuf
          pkgs.glib
          pkgs.gtk3
          pkgs.harfbuzz
          pkgs.libappindicator-gtk3
          pkgs.libsoup_3
          pkgs.openssl
          pkgs.pango
          pkgs.webkitgtk_4_1
          pkgs.xdotool
          pkgs.zlib
        ];
        dioxusLinuxGSettingsDataDirs = lib.concatMapStringsSep ":" (pkg: "${pkg}/share/gsettings-schemas/${pkg.name}") [
          pkgs.gsettings-desktop-schemas
          pkgs.gtk3
        ];
        dioxusLinuxXdgDataDirs = lib.concatStringsSep ":" [
          dioxusLinuxGSettingsDataDirs
          (lib.makeSearchPath "share" [
            pkgs.adwaita-icon-theme
            pkgs.gtk3
            pkgs.hicolor-icon-theme
            pkgs.shared-mime-info
          ])
        ];
        dioxusDarwinBuildInputs = lib.optionals isDarwin [
          pkgs.apple-sdk_15
        ];
        dioxusDesktopPackageType =
          if isDarwin
          then "macos"
          else "deb";
        dioxusDesktopPlatformArg =
          if isDarwin
          then "--macos"
          else "--desktop";
        keepbookDioxusDarwinInfoPlist = pkgs.writeText "Keepbook-Info.plist" ''
          <?xml version="1.0" encoding="UTF-8"?>
          <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
            "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
          <plist version="1.0">
          <dict>
            <key>CFBundleDevelopmentRegion</key>
            <string>en</string>
            <key>CFBundleDisplayName</key>
            <string>Keepbook</string>
            <key>CFBundleExecutable</key>
            <string>Keepbook</string>
            <key>CFBundleIconFile</key>
            <string>keepbook-icon.png</string>
            <key>CFBundleIdentifier</key>
            <string>${keepbookDioxusAppId}</string>
            <key>CFBundleInfoDictionaryVersion</key>
            <string>6.0</string>
            <key>CFBundleName</key>
            <string>Keepbook</string>
            <key>CFBundlePackageType</key>
            <string>APPL</string>
            <key>CFBundleShortVersionString</key>
            <string>${appVersion}</string>
            <key>CFBundleVersion</key>
            <string>${appVersion}</string>
            <key>LSMinimumSystemVersion</key>
            <string>13.0</string>
            <key>NSHighResolutionCapable</key>
            <true/>
          </dict>
          </plist>
        '';
        dioxusAndroidBuildScript = release:
          pkgs.writeShellApplication {
            name = "keepbook-dioxus-android-${
              if release
              then "release"
              else "debug"
            }";
            runtimeInputs = [
              pkgs.coreutils
              pkgs.findutils
              pkgs.imagemagick
              pkgs.gnused
              pkgs.jq
              pkgs.nix
            ];
            text = ''
              set -euo pipefail

              repo="''${KEEPBOOK_ROOT:-$PWD}"
              cd "$repo"

              profile=${
                if release
                then "release"
                else "debug"
              }

              args=(
                dx ${
                if release
                then "bundle"
                else "build"
              } --android
                --target aarch64-linux-android
                --package keepbook-dioxus
                --no-default-features
                --features android
              )

              if ${
                if release
                then "true"
                else "false"
              }; then
                args+=(--release)
              fi

              patch_android_project() {
                local gradle_root="target/dx/keepbook-dioxus/$profile/android/app"
                local root_gradle="$gradle_root/build.gradle.kts"
                local app_gradle="$gradle_root/app/build.gradle.kts"
                local gradle_properties="$gradle_root/gradle.properties"
                local manifest="$gradle_root/app/src/main/AndroidManifest.xml"
                local res="$gradle_root/app/src/main/res"

                if [[ -f "$root_gradle" ]]; then
                  sed -i \
                    -e 's/com\.android\.tools\.build:gradle:[^"]*/com.android.tools.build:gradle:${androidGradlePluginVersion}/' \
                    -e 's/org\.jetbrains\.kotlin:kotlin-gradle-plugin:[^"]*/org.jetbrains.kotlin:kotlin-gradle-plugin:${androidKotlinPluginVersion}/' \
                    "$root_gradle"
                fi

                if [[ -f "$app_gradle" ]]; then
                  sed -i \
                    -e 's/compileSdk = [0-9][0-9]*/compileSdk = ${androidCompileSdkVersion}\n    buildToolsVersion = "${androidBuildToolsVersion}"/' \
                    -e 's/targetSdk = [0-9][0-9]*/targetSdk = ${androidTargetSdkVersion}/' \
                    -e 's/versionCode = [0-9][0-9]*/versionCode = ${androidVersionCode}/' \
                    -e 's/versionName = "[^"]*"/versionName = "${appVersion}"/' \
                    -e '/^[[:space:]]*kotlinOptions[[:space:]]*{/,/^[[:space:]]*}/c\    kotlin {\n        compilerOptions {\n            jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)\n        }\n    }' \
                    "$app_gradle"
                fi

                if [[ -f "$gradle_properties" ]]; then
                  sed -i '/^android\.defaults\.buildfeatures\.buildconfig=/d' "$gradle_properties"
                fi

                if [[ -f "$manifest" ]]; then
                  sed -i '/android:extractNativeLibs=/d' "$manifest"
                fi

                if [[ -d "$res" && -f "$repo/assets/keepbook-icon.svg" ]]; then
                  # Adaptive launcher icon (API 26+): a solid brand-green background
                  # with the logo as a separate foreground layer. This replaces the
                  # bare legacy bitmap, which modern launchers auto-shrink into a
                  # small masked badge; the adaptive foreground lets the logo fill
                  # more of the tile so it reads noticeably larger.
                  mkdir -p "$res/mipmap-anydpi-v26" "$res/values"
                  printf '%s\n' \
                    '<?xml version="1.0" encoding="utf-8"?>' \
                    '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">' \
                    '    <background android:drawable="@color/ic_launcher_background" />' \
                    '    <foreground android:drawable="@mipmap/ic_launcher_foreground" />' \
                    '</adaptive-icon>' \
                    > "$res/mipmap-anydpi-v26/ic_launcher.xml"

                  # Background color = the logo's brighter accent green.
                  if [[ ! -f "$res/values/colors.xml" ]]; then
                    printf '%s\n' '<?xml version="1.0" encoding="utf-8"?>' '<resources>' '</resources>' \
                      > "$res/values/colors.xml"
                  fi
                  if grep -q 'ic_launcher_background' "$res/values/colors.xml"; then
                    sed -i 's|<color name="ic_launcher_background">[^<]*</color>|<color name="ic_launcher_background">#638A68</color>|' "$res/values/colors.xml"
                  else
                    sed -i 's|</resources>|    <color name="ic_launcher_background">#638A68</color>\n</resources>|' "$res/values/colors.xml"
                  fi

                  # Adaptive foreground: transparent 108dp canvas per density with the
                  # logo sized to 55% of the canvas — comfortably inside the safe zone so
                  # modern launchers do not make the mark feel oversized inside their masks.
                  for density in mdpi:108 hdpi:162 xhdpi:216 xxhdpi:324 xxxhdpi:432; do
                    local qualifier="''${density%%:*}"
                    local canvas="''${density##*:}"
                    local logo_size="$((canvas * 55 / 100))"
                    local dir="$res/mipmap-$qualifier"
                    mkdir -p "$dir"
                    magick -background none "$repo/assets/keepbook-icon.svg" \
                      -alpha set -trim +repage \
                      -resize "''${logo_size}x''${logo_size}" \
                      -gravity center -background none -extent "''${canvas}x''${canvas}" \
                      "$dir/ic_launcher_foreground.png"
                  done

                  # Legacy square icon for pre-API-26 launchers (unmasked): the logo
                  # on the same brand-green background with modest edge padding.
                  for density in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
                    local qualifier="''${density%%:*}"
                    local size="''${density##*:}"
                    local icon_inner_size="$((size * 88 / 100))"
                    local dir="$res/mipmap-$qualifier"
                    mkdir -p "$dir"
                    rm -f "$dir/ic_launcher.webp"
                    magick -background none "$repo/assets/keepbook-icon.svg" \
                      -alpha set -trim +repage \
                      -resize "''${icon_inner_size}x''${icon_inner_size}" \
                      -gravity center -background '#638A68' -extent "''${size}x''${size}" \
                      -alpha off \
                      "$dir/ic_launcher.png"
                  done
                fi
              }

              sign_release_apks() {
                if [[ -z "''${ANDROID_SIGNING_KEYSTORE_BASE64:-}" && -z "''${ANDROID_SIGNING_KEYSTORE_FILE:-}" ]]; then
                  echo "Android release signing skipped: no signing keystore was provided"
                  return
                fi

                local required=(
                  ANDROID_SIGNING_KEY_ALIAS
                  ANDROID_SIGNING_KEYSTORE_PASSWORD
                  ANDROID_SIGNING_KEY_PASSWORD
                )

                for var in "''${required[@]}"; do
                  if [[ -z "''${!var:-}" ]]; then
                    echo "Android release signing requires $var" >&2
                    exit 1
                  fi
                done

                local signing_dir
                signing_dir="$(mktemp -d)"
                trap 'rm -rf "$signing_dir"' RETURN

                local keystore="$signing_dir/keepbook-release.keystore"
                if [[ -n "''${ANDROID_SIGNING_KEYSTORE_FILE:-}" ]]; then
                  cp "$ANDROID_SIGNING_KEYSTORE_FILE" "$keystore"
                else
                  printf '%s' "$ANDROID_SIGNING_KEYSTORE_BASE64" | base64 -d > "$keystore"
                fi

                local apk_dir="target/dx/keepbook-dioxus/release/android/app/app/build/outputs/apk/release"
                shopt -s nullglob
                local unsigned_apks=("$apk_dir"/*-unsigned.apk)
                shopt -u nullglob

                if ((''${#unsigned_apks[@]} == 0)); then
                  echo "No unsigned release APKs were found to sign in $apk_dir" >&2
                  exit 1
                fi

                for unsigned_apk in "''${unsigned_apks[@]}"; do
                  local apk_base="''${unsigned_apk%-unsigned.apk}"
                  local aligned_apk
                  aligned_apk="$signing_dir/$(basename "$apk_base")-aligned.apk"
                  local signed_apk="$apk_base-signed.apk"

                  # shellcheck disable=SC2016
                  nix develop "$repo#android" --command bash -lc '
                    set -euo pipefail
                    unsigned_apk="$1"
                    aligned_apk="$2"
                    signed_apk="$3"
                    keystore="$4"
                    key_alias="$5"
                    storepass="$6"
                    keypass="$7"

                    "$ANDROID_HOME/build-tools/${androidBuildToolsVersion}/zipalign" -p -f 4 "$unsigned_apk" "$aligned_apk"
                    "$ANDROID_HOME/build-tools/${androidBuildToolsVersion}/apksigner" sign \
                      --ks "$keystore" \
                      --ks-key-alias "$key_alias" \
                      --ks-pass "pass:$storepass" \
                      --key-pass "pass:$keypass" \
                      --out "$signed_apk" \
                      "$aligned_apk"
                    "$ANDROID_HOME/build-tools/${androidBuildToolsVersion}/apksigner" verify --verbose "$signed_apk"
                  ' bash \
                    "$unsigned_apk" \
                    "$aligned_apk" \
                    "$signed_apk" \
                    "$keystore" \
                    "$ANDROID_SIGNING_KEY_ALIAS" \
                    "$ANDROID_SIGNING_KEYSTORE_PASSWORD" \
                    "$ANDROID_SIGNING_KEY_PASSWORD"
                done
              }

              rm -rf "target/dx/keepbook-dioxus/$profile/android"
              nix develop "$repo#android" --command "''${args[@]}" "$@"
              patch_android_project

              if ${
                if release
                then "true"
                else "false"
              }; then
                nix develop "$repo#android" --command bash -lc \
                  'cd target/dx/keepbook-dioxus/release/android/app && ./gradlew :app:bundleRelease :app:assembleRelease --no-daemon --console plain'
                sign_release_apks
              else
                nix develop "$repo#android" --command bash -lc \
                  'cd target/dx/keepbook-dioxus/debug/android/app && ./gradlew :app:assembleDebug --no-daemon --console plain'
              fi

              if ${
                if release
                then "true"
                else "false"
              }; then
                find "$repo/target/dx/keepbook-dioxus/release/android" \
                  \( -path '*/build/outputs/apk/release/*.apk' -o -path '*/build/outputs/bundle/release/*.aab' \) \
                  -print
              else
                find "$repo/target/dx/keepbook-dioxus/$profile/android" \
                  -path '*/build/outputs/apk/debug/*.apk' \
                  -print
              fi
            '';
          };
        dioxusDesktopBuildScript = pkgs.writeShellApplication {
          name = "keepbook-dioxus-desktop-release";
          runtimeInputs =
            [
              pkgs.coreutils
              pkgs.findutils
              pkgs.nix
            ]
            ++ lib.optionals isLinux [
              pkgs.dpkg
            ];
          text = ''
            set -euo pipefail

            repo="''${KEEPBOOK_ROOT:-$PWD}"
            out_dir="''${KEEPBOOK_DIOXUS_DESKTOP_OUT_DIR:-$repo/target/release-artifacts/desktop}"

            patch_linux_deb_bundles() {
              if ! ${if isLinux then "true" else "false"}; then
                return
              fi

              find "$out_dir" -type f -name '*.deb' -print0 |
                while IFS= read -r -d $'\0' deb; do
                  work_dir="$(mktemp -d)"
                  package_dir="$work_dir/package"

                  dpkg-deb -R "$deb" "$package_dir"

                  install_icon_set() {
                    icon_name="$1"
                    install -Dm644 "$repo/assets/keepbook-icon.svg" \
                      "$package_dir/usr/share/icons/hicolor/scalable/apps/$icon_name.svg"
                    install -Dm644 "$repo/assets/keepbook-icon-32.png" \
                      "$package_dir/usr/share/icons/hicolor/32x32/apps/$icon_name.png"
                    install -Dm644 "$repo/assets/keepbook-icon-48.png" \
                      "$package_dir/usr/share/icons/hicolor/48x48/apps/$icon_name.png"
                    install -Dm644 "$repo/assets/keepbook-icon-64.png" \
                      "$package_dir/usr/share/icons/hicolor/64x64/apps/$icon_name.png"
                  }

                  write_desktop_entry() {
                    desktop_path="$1"
                    icon_name="$2"
                    startup_wm_class="$3"
                    install -Dm644 /dev/null "$desktop_path"
                    cat > "$desktop_path" <<EOF
[Desktop Entry]
Categories=Office;Finance
Comment=Local-first personal finance toolkit
Exec=keepbook-dioxus
GenericName=Personal finance toolkit
Icon=$icon_name
Name=Keepbook
StartupNotify=true
StartupWMClass=$startup_wm_class
Terminal=false
Type=Application
EOF
                  }

                  install_icon_set "${keepbookDioxusAppId}"
                  install_icon_set "${keepbookDioxusDesktopAlias}"
                  install_icon_set "keepbook"

                  write_desktop_entry \
                    "$package_dir/usr/share/applications/${keepbookDioxusAppId}.desktop" \
                    "${keepbookDioxusAppId}" \
                    "${keepbookDioxusAppId}"
                  write_desktop_entry \
                    "$package_dir/usr/share/applications/${keepbookDioxusDesktopAlias}.desktop" \
                    "${keepbookDioxusDesktopAlias}" \
                    "${keepbookDioxusDesktopAlias}"

                  (
                    cd "$package_dir"
                    find usr -type f ! -path 'usr/share/doc/*' -print0 |
                      sort -z |
                      xargs -0 md5sum > DEBIAN/md5sums
                  )

                  rm -f "$deb"
                  dpkg-deb --root-owner-group -b "$package_dir" "$deb"
                  rm -rf "$work_dir"
                done
            }

            cd "$repo"
            rm -rf "$out_dir"
            mkdir -p "$out_dir"

            nix develop "$repo" --command dx bundle \
              ${dioxusDesktopPlatformArg} \
              --package-types ${dioxusDesktopPackageType} \
              --out-dir "$out_dir" \
              --package keepbook-dioxus \
              --no-default-features \
              --features desktop \
              --release \
              --locked \
              "$@"

            patch_linux_deb_bundles

            find "$out_dir" -type f -print
          '';
        };
        keepbookDioxusDesktopItem = pkgs.makeDesktopItem {
          name = keepbookDioxusAppId;
          desktopName = "Keepbook";
          genericName = "Personal finance toolkit";
          comment = "Local-first personal finance toolkit";
          exec = "keepbook-dioxus";
          icon = keepbookDioxusAppId;
          categories = ["Office" "Finance"];
          startupNotify = true;
          startupWMClass = keepbookDioxusAppId;
        };
        keepbookDioxusDesktopAliasItem = pkgs.makeDesktopItem {
          name = keepbookDioxusDesktopAlias;
          desktopName = "Keepbook";
          genericName = "Personal finance toolkit";
          comment = "Local-first personal finance toolkit";
          exec = "keepbook-dioxus";
          icon = keepbookDioxusDesktopAlias;
          categories = ["Office" "Finance"];
          startupNotify = true;
          startupWMClass = keepbookDioxusDesktopAlias;
        };
        keepbookDioxusDesktopLinuxPostInstall = lib.optionalString isLinux ''
          wrapProgram "$out/bin/keepbook-dioxus" \
            --set WEBKIT_DISABLE_DMABUF_RENDERER 1 \
            --prefix XDG_DATA_DIRS : "$out/share:${dioxusLinuxXdgDataDirs}" \
            --run 'if [ -z "''${WAYLAND_DISPLAY:-}" ] && [ -n "''${XDG_RUNTIME_DIR:-}" ]; then for socket in "''${XDG_RUNTIME_DIR}"/wayland-*; do [ -S "$socket" ] || continue; export WAYLAND_DISPLAY="''${socket##*/}"; break; done; fi' \
            --run 'if [ -n "''${WAYLAND_DISPLAY:-}" ]; then export XDG_SESSION_TYPE="''${XDG_SESSION_TYPE:-wayland}"; export GDK_BACKEND="''${GDK_BACKEND:-wayland,x11}"; fi' \
            --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath dioxusLinuxLibraryPathInputs}

          install -Dm644 ${cleanSrc}/assets/keepbook-icon.svg \
            "$out/share/icons/hicolor/scalable/apps/${keepbookDioxusAppId}.svg"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-32.png \
            "$out/share/icons/hicolor/32x32/apps/${keepbookDioxusAppId}.png"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-48.png \
            "$out/share/icons/hicolor/48x48/apps/${keepbookDioxusAppId}.png"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-64.png \
            "$out/share/icons/hicolor/64x64/apps/${keepbookDioxusAppId}.png"

          install -Dm644 ${cleanSrc}/assets/keepbook-icon.svg \
            "$out/share/icons/hicolor/scalable/apps/keepbook.svg"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-32.png \
            "$out/share/icons/hicolor/32x32/apps/keepbook.png"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-48.png \
            "$out/share/icons/hicolor/48x48/apps/keepbook.png"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-64.png \
            "$out/share/icons/hicolor/64x64/apps/keepbook.png"

          install -Dm644 ${cleanSrc}/assets/keepbook-icon.svg \
            "$out/share/icons/hicolor/scalable/apps/${keepbookDioxusDesktopAlias}.svg"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-32.png \
            "$out/share/icons/hicolor/32x32/apps/${keepbookDioxusDesktopAlias}.png"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-48.png \
            "$out/share/icons/hicolor/48x48/apps/${keepbookDioxusDesktopAlias}.png"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-64.png \
            "$out/share/icons/hicolor/64x64/apps/${keepbookDioxusDesktopAlias}.png"
          install -Dm644 ${keepbookDioxusDesktopItem}/share/applications/${keepbookDioxusAppId}.desktop \
            "$out/share/applications/${keepbookDioxusAppId}.desktop"
          install -Dm644 ${keepbookDioxusDesktopAliasItem}/share/applications/${keepbookDioxusDesktopAlias}.desktop \
            "$out/share/applications/${keepbookDioxusDesktopAlias}.desktop"
        '';
        keepbookDioxusDesktopDarwinPostInstall = lib.optionalString isDarwin ''
          app="$out/Applications/Keepbook.app"
          contents="$app/Contents"
          macos="$contents/MacOS"
          resources="$contents/Resources"

          mkdir -p "$macos" "$resources"
          cp "$out/bin/keepbook-dioxus" "$macos/Keepbook"
          chmod +x "$macos/Keepbook"
          install -Dm644 ${cleanSrc}/assets/keepbook-icon-64.png "$resources/keepbook-icon.png"
          install -Dm644 ${keepbookDioxusDarwinInfoPlist} "$contents/Info.plist"
        '';
        keepbookAgeRecipientsScript = pkgs.writeShellApplication {
          name = "keepbook-age-recipients";
          runtimeInputs = [
            pkgs.coreutils
            pkgs.jq
            pkgs.nix
          ];
          text = ''
            set -euo pipefail

            keys_file="''${KEEPBOOK_AGE_KEYS_FILE:-keys.nix}"
            keys_attr="''${KEEPBOOK_AGE_KEYS_ATTR:-agenixKeys}"

            usage() {
              cat <<'EOF'
            Usage: keepbook-age-recipients [--keys-file PATH] [--attr ATTR]

            Prints SSH public keys from a keepbook data repo keys file in age
            recipient-file format.

            Defaults:
              --keys-file  keys.nix
              --attr       agenixKeys
            EOF
            }

            while [[ $# -gt 0 ]]; do
              case "$1" in
                --keys-file)
                  keys_file="''${2:?missing value for --keys-file}"
                  shift 2
                  ;;
                --attr)
                  keys_attr="''${2:?missing value for --attr}"
                  shift 2
                  ;;
                -h|--help)
                  usage
                  exit 0
                  ;;
                *)
                  echo "unknown argument: $1" >&2
                  usage >&2
                  exit 2
                  ;;
              esac
            done

            if [[ ! -f "$keys_file" ]]; then
              echo "keys file does not exist: $keys_file" >&2
              exit 1
            fi

            keys_file="$(realpath "$keys_file")"
            keys_path_json="$(jq -Rnr --arg path "$keys_file" '$path | @json')"
            keys_attr_json="$(jq -Rnr --arg attr "$keys_attr" '$attr | @json')"
            nix_expr="let keys = import (builtins.toPath $keys_path_json); in builtins.getAttr $keys_attr_json keys"

            nix eval --impure --json --expr "$nix_expr" \
              | jq -r '.[]' \
              | grep -v '^[[:space:]]*$' \
              | sort -u
          '';
        };
        keepbookAgeEncryptScript = pkgs.writeShellApplication {
          name = "keepbook-age-encrypt";
          runtimeInputs = [
            pkgs.age
            pkgs.coreutils
            pkgs.jq
            pkgs.nix
          ];
          text = ''
            set -euo pipefail

            keys_file="''${KEEPBOOK_AGE_KEYS_FILE:-keys.nix}"
            keys_attr="''${KEEPBOOK_AGE_KEYS_ATTR:-agenixKeys}"
            output=""
            input=""

            usage() {
              cat <<'EOF'
            Usage: keepbook-age-encrypt [--keys-file PATH] [--attr ATTR] [-o OUTPUT] [PLAINTEXT]

            Encrypts a pass-style credential payload to SSH public keys from a
            keepbook data repo keys file. If PLAINTEXT is omitted, stdin is used.

            Defaults:
              --keys-file  keys.nix
              --attr       agenixKeys
            EOF
            }

            while [[ $# -gt 0 ]]; do
              case "$1" in
                --keys-file)
                  keys_file="''${2:?missing value for --keys-file}"
                  shift 2
                  ;;
                --attr)
                  keys_attr="''${2:?missing value for --attr}"
                  shift 2
                  ;;
                -o|--output)
                  output="''${2:?missing value for --output}"
                  shift 2
                  ;;
                -h|--help)
                  usage
                  exit 0
                  ;;
                -*)
                  echo "unknown argument: $1" >&2
                  usage >&2
                  exit 2
                  ;;
                *)
                  if [[ -n "$input" ]]; then
                    echo "only one plaintext path may be provided" >&2
                    exit 2
                  fi
                  input="$1"
                  shift
                  ;;
              esac
            done

            recipients_file="$(mktemp)"
            trap 'rm -f "$recipients_file"' EXIT

            ${keepbookAgeRecipientsScript}/bin/keepbook-age-recipients \
              --keys-file "$keys_file" \
              --attr "$keys_attr" \
              > "$recipients_file"

            if [[ ! -s "$recipients_file" ]]; then
              echo "no age recipients derived from $keys_file attr $keys_attr" >&2
              exit 1
            fi

            args=(--armor --recipients-file "$recipients_file")
            if [[ -n "$output" ]]; then
              mkdir -p "$(dirname "$output")"
              args+=(--output "$output")
            fi
            if [[ -n "$input" ]]; then
              args+=("$input")
            fi

            age "''${args[@]}"
          '';
        };
        rustWarningsShim = pkgs.writeShellScriptBin "warnings" ''
          exec env "RUSTFLAGS=-D warnings" "$@"
        '';
        mkKeepbookPackage = {
          pname,
          cargoPackage ? "keepbook",
          buildFeatures ? [],
          buildNoDefaultFeatures ? false,
          extraBuildInputs ? [],
          extraNativeBuildInputs ? [],
          checkLibraryPathInputs ? [],
          postInstall ? "",
        }: let
          runtimeLibraryPathInputs = [pkgs.openssl] ++ checkLibraryPathInputs;
          linuxRuntimeWrapper = lib.optionalString isLinux ''
            for program in "$out"/bin/*; do
              if [ -f "$program" ] && [ -x "$program" ]; then
                wrapProgram "$program" \
                  --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibraryPathInputs}
              fi
            done
          '';
        in
          rustPlatform.buildRustPackage {
            inherit pname buildFeatures buildNoDefaultFeatures;
            version = appVersion;
            src = cleanSrc;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            cargoBuildFlags = ["-p" cargoPackage];
            cargoTestFlags = ["-p" cargoPackage];
            checkFlags = [
              "--skip"
              "contracts_match_both_clis"
              "--skip"
              "contracts_match_rust_cli"
            ];
            nativeBuildInputs =
              [pkgs.pkg-config]
              ++ lib.optionals isLinux [pkgs.makeWrapper]
              ++ extraNativeBuildInputs;
            buildInputs = [pkgs.openssl] ++ extraBuildInputs;
            LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibraryPathInputs;
            OPENSSL_NO_VENDOR = "1";
            KEEPBOOK_GIT_COMMIT_HASH = appCommit;
            RUST_MIN_STACK = "33554432";
            postInstall = postInstall + linuxRuntimeWrapper;
          };
        keepbookDioxusDesktop = mkKeepbookPackage {
          pname = "keepbook-dioxus";
          cargoPackage = "keepbook-dioxus";
          buildNoDefaultFeatures = true;
          buildFeatures = ["desktop"];
          extraNativeBuildInputs = [pkgs.makeWrapper];
          extraBuildInputs = dioxusLinuxBuildInputs ++ dioxusDarwinBuildInputs ++ [pkgs.openssl];
          checkLibraryPathInputs = dioxusLinuxLibraryPathInputs;
          postInstall = keepbookDioxusDesktopLinuxPostInstall + keepbookDioxusDesktopDarwinPostInstall;
        };
      in {
        formatter = pkgs.alejandra;

        packages =
          {
            default = mkKeepbookPackage {pname = "keepbook";};
            keepbook = mkKeepbookPackage {pname = "keepbook";};
            keepbook-age-recipients = keepbookAgeRecipientsScript;
            keepbook-age-encrypt = keepbookAgeEncryptScript;
          }
          // lib.optionalAttrs (isLinux || isDarwin) {
            keepbook-dioxus-desktop = keepbookDioxusDesktop;
            keepbook-dioxus-desktop-release-runner = dioxusDesktopBuildScript;
          }
          // lib.optionalAttrs isLinux {
            keepbook-tray = mkKeepbookPackage {
              pname = "keepbook-tray";
              buildFeatures = ["tray"];
              extraBuildInputs = [pkgs.dbus];
            };
            keepbook-dioxus-android-debug-runner = dioxusAndroidBuildScript false;
            keepbook-dioxus-android-release-runner = dioxusAndroidBuildScript true;
          };

        apps =
          {
            keepbook-age-recipients = {
              type = "app";
              program = "${keepbookAgeRecipientsScript}/bin/keepbook-age-recipients";
            };
            keepbook-age-encrypt = {
              type = "app";
              program = "${keepbookAgeEncryptScript}/bin/keepbook-age-encrypt";
            };
          }
          // lib.optionalAttrs (isLinux || isDarwin) {
            dioxus-desktop-release = {
              type = "app";
              program = "${dioxusDesktopBuildScript}/bin/keepbook-dioxus-desktop-release";
            };
            keepbook-dioxus-desktop = {
              type = "app";
              program =
                if isDarwin
                then "${keepbookDioxusDesktop}/Applications/Keepbook.app/Contents/MacOS/Keepbook"
                else "${keepbookDioxusDesktop}/bin/keepbook-dioxus";
            };
            keepbook-dioxus-desktop-release = {
              type = "app";
              program = "${dioxusDesktopBuildScript}/bin/keepbook-dioxus-desktop-release";
            };
          }
          // lib.optionalAttrs isLinux {
            dioxus-android-debug = {
              type = "app";
              program = "${dioxusAndroidBuildScript false}/bin/keepbook-dioxus-android-debug";
            };
            dioxus-android-release = {
              type = "app";
              program = "${dioxusAndroidBuildScript true}/bin/keepbook-dioxus-android-release";
            };
            keepbook-dioxus-android-debug = {
              type = "app";
              program = "${dioxusAndroidBuildScript false}/bin/keepbook-dioxus-android-debug";
            };
            keepbook-dioxus-android-release = {
              type = "app";
              program = "${dioxusAndroidBuildScript true}/bin/keepbook-dioxus-android-release";
            };
          };

        devShells = {
          default = pkgs.mkShell {
            buildInputs =
              [
                toolchain
                pkgs.pkg-config
                pkgs.binaryen
                pkgs.dioxus-cli
                pkgs.wasm-bindgen-cli_0_2_118
                pkgs.openssl
                pkgs.just
                pkgs.jq
                pkgs.age
                rustWarningsShim
              ]
              ++ dioxusLinuxBuildInputs;

            LD_LIBRARY_PATH = lib.optionalString isLinux (lib.makeLibraryPath dioxusLinuxLibraryPathInputs);

            OPENSSL_NO_VENDOR = "1";
            WEBKIT_DISABLE_DMABUF_RENDERER = "1";
          };

          android = pkgs.mkShell (dioxusAndroidEnv
            // {
              buildInputs =
                [
                  toolchain
                  pkgs.dioxus-cli
                  pkgs.wasm-bindgen-cli_0_2_118
                  pkgs.jdk17
                  pkgs.pkg-config
                  pkgs.binaryen
                  pkgs.openssl
                  pkgs.just
                  pkgs.jq
                  pkgs.gradle_9
                ]
                ++ dioxusLinuxBuildInputs;

              LD_LIBRARY_PATH = lib.optionalString isLinux (lib.makeLibraryPath dioxusLinuxLibraryPathInputs);
              WEBKIT_DISABLE_DMABUF_RENDERER = "1";

              shellHook = ''
                export PATH=${androidHome}/emulator:${androidHome}/platform-tools:${androidHome}/cmdline-tools/${androidCmdLineToolsVersion}/bin:$PATH

                echo "keepbook Dioxus Android dev shell"
                echo "  dx: $(dx --version)"
                echo "  ANDROID_HOME: $ANDROID_HOME"
                echo ""
                echo "Commands:"
                echo "  just dioxus-android-build"
                echo "  just dioxus-android-release"
                echo "  nix run .#dioxus-android-release"
              '';
            });
        };
      }
    );
}
