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
              "node_modules"
              "dist"
              "ts"
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
          # Dioxus 0.7.3 still generates an initial Gradle project pinned to
          # SDK 33/Build Tools 34. The wrapper patches and rebuilds with the
          # latest versions below, but the generated first pass still needs
          # these packages available in the immutable SDK.
          platformVersions = ["33" androidCompileSdkVersion];
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
                --features mobile
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
                    -e '/^[[:space:]]*kotlinOptions[[:space:]]*{/,/^[[:space:]]*}/c\    kotlin {\n        compilerOptions {\n            jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8)\n        }\n    }' \
                    "$app_gradle"
                fi

                if [[ -f "$gradle_properties" ]]; then
                  sed -i '/^android\.defaults\.buildfeatures\.buildconfig=/d' "$gradle_properties"
                fi

                if [[ -f "$manifest" ]]; then
                  sed -i '/android:extractNativeLibs=/d' "$manifest"
                fi
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
        mkKeepbookPackage = {
          pname,
          cargoPackage ? "keepbook",
          buildFeatures ? [],
          extraBuildInputs ? [],
          extraNativeBuildInputs ? [],
        }:
          rustPlatform.buildRustPackage {
            inherit pname buildFeatures;
            version = "0.1.1";
            src = cleanSrc;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            cargoBuildFlags = ["-p" cargoPackage];
            cargoTestFlags = ["-p" cargoPackage];
            checkFlags = ["--skip" "contracts_match_both_clis"];
            nativeBuildInputs = [pkgs.pkg-config] ++ extraNativeBuildInputs;
            buildInputs = extraBuildInputs;
          };
      in {
        packages =
          {
            default = mkKeepbookPackage {pname = "keepbook";};
            keepbook = mkKeepbookPackage {pname = "keepbook";};
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

        apps = lib.optionalAttrs isLinux {
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
                pkgs.openssl
                pkgs.just
                pkgs.jq
                pkgs.nodejs_22
                pkgs.yarn
              ]
              ++ lib.optionals isLinux [
                pkgs.dbus
                pkgs.glib
                pkgs.gtk3
                pkgs.webkitgtk_4_1
                pkgs.xdotool
              ];

            LD_LIBRARY_PATH = lib.optionalString isLinux (lib.makeLibraryPath [
              pkgs.cairo
              pkgs.gdk-pixbuf
              pkgs.glib
              pkgs.gtk3
              pkgs.harfbuzz
              pkgs.libsoup_3
              pkgs.openssl
              pkgs.pango
              pkgs.webkitgtk_4_1
              pkgs.xdotool
            ]);

            OPENSSL_NO_VENDOR = "1";
          };

          android = pkgs.mkShell (dioxusAndroidEnv
            // {
              buildInputs = [
                toolchain
                pkgs.dioxus-cli
                pkgs.jdk17
                pkgs.pkg-config
                pkgs.binaryen
                pkgs.openssl
                pkgs.just
                pkgs.jq
                pkgs.gradle_9
              ];

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
