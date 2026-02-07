{
  description = "keepbook frontend - React Native (Expo) app";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nixgl.url = "github:nix-community/nixGL";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    flake-utils,
    nixgl,
    fenix,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        config = {
          allowUnfree = true;
          android_sdk.accept_license = true;
        };
        overlays = [nixgl.overlay];
      };

      fenixPkgs = fenix.packages.${system};
      rustToolchainBase = fenixPkgs.stable.withComponents [
        "cargo"
        "clippy"
        "rust-analyzer"
        "rust-src"
        "rustc"
        "rustfmt"
      ];
      rustToolchain = fenixPkgs.combine [
        rustToolchainBase
        fenixPkgs.targets.aarch64-linux-android.stable.rust-std
        fenixPkgs.targets.x86_64-linux-android.stable.rust-std
      ];

      nodejs = pkgs.nodejs_22;

      # Mirrors the Android SDK setup used in /home/imalison/Projects/mova to keep
      # the RN/Expo Android workflow working consistently under Nix.
      buildToolsVersion = "36.0.0";
      cmdLineToolsVersion = "8.0";
      androidComposition = pkgs.androidenv.composeAndroidPackages {
        cmdLineToolsVersion = cmdLineToolsVersion;
        toolsVersion = "26.1.1";
        platformToolsVersion = "35.0.2";
        buildToolsVersions = [buildToolsVersion "35.0.0" "34.0.0"];
        includeEmulator = true;
        platformVersions = ["35" "36"];
        includeSources = false;
        includeSystemImages = true;
        systemImageTypes = ["google_apis_playstore"];
        abiVersions = ["x86_64"];
        includeNDK = true;
        ndkVersions = ["27.1.12297006" "27.0.12077973" "26.1.10909125"];
        cmakeVersions = ["3.22.1"];
        useGoogleAPIs = true;
        useGoogleTVAddOns = false;
      };

      android-sdk = androidComposition.androidsdk;
      android-home = "${androidComposition.androidsdk}/libexec/android-sdk";
      aapt2Binary = "${android-home}/build-tools/${buildToolsVersion}/aapt2";

      sharedDeps = with pkgs; [
        nodejs
        yarn
        watchman
        alejandra
        just
        rustToolchain
        cargo-ndk
        perl
        cmake
        pkg-config
      ];
    in {
      devShells = {
        android = pkgs.mkShell {
          buildInputs =
            sharedDeps
            ++ [pkgs.jdk17]
            ++ (
              if system == "x86_64-linux"
              then [pkgs.nixgl.auto.nixGLDefault pkgs.nixgl.nixGLIntel]
              else []
            );

          LC_ALL = "en_US.UTF-8";
          LANG = "en_US.UTF-8";

          ANDROID_HOME = android-home;
          ANDROID_SDK_ROOT = android-home;
          ANDROID_NDK_HOME = "${android-home}/ndk/27.1.12297006";
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${aapt2Binary}";

          shellHook = ''
            export JAVA_HOME=${pkgs.jdk17.home}
            export PATH=${android-home}/emulator:${android-home}/cmdline-tools/${cmdLineToolsVersion}/bin:$PWD/node_modules/.bin:$PATH
            export KEEPBOOK_ROOT="$PWD/.."

            echo "keepbook frontend Android dev shell"
            echo "  node: $(node --version)"
            echo "  yarn: $(yarn --version)"
            echo ""
            echo "Commands:"
            echo "  yarn start       - Start Expo dev server"
            echo "  yarn android     - Build + run on Android"
            echo "  yarn web         - Run in browser"
            echo "  just emulator    - Start Android emulator (uses nixGLIntel)"
            echo "  just --list      - Show all just commands"
          '';
        };

        ios = (pkgs.mkShell.override {stdenv = pkgs.stdenvNoCC;}) {
          disallowedRequisites = [pkgs.xcbuild pkgs.xcbuild.xcrun];

          buildInputs =
            sharedDeps
            ++ [
              pkgs.cocoapods
              pkgs.ruby
              pkgs.bundler
            ];

          LC_ALL = "en_US.UTF-8";
          LANG = "en_US.UTF-8";

          shellHook = ''
            unset DEVELOPER_DIR
            export PATH="$PWD/node_modules/.bin:$PWD/vendor/bundle/bin:$PATH"
            export BUNDLE_PATH="$PWD/vendor/bundle"
            export GEM_HOME="$PWD/vendor/bundle"
            export KEEPBOOK_ROOT="$PWD/.."

            echo "keepbook frontend iOS dev shell"
            echo "  node: $(node --version)"
            echo "  yarn: $(yarn --version)"
            echo "  ruby: $(ruby --version)"
            echo ""
            echo "Commands:"
            echo "  yarn start       - Start Expo dev server"
            echo "  yarn ios         - Build + run on iOS"
            echo "  yarn web         - Run in browser"
            echo "  just --list      - Show all just commands"
          '';
        };

        default = pkgs.mkShell {
          buildInputs = sharedDeps;
          LC_ALL = "en_US.UTF-8";
          LANG = "en_US.UTF-8";

          shellHook = ''
            export PATH="$PWD/node_modules/.bin:$PATH"
            export KEEPBOOK_ROOT="$PWD/.."

            echo "keepbook frontend dev shell (use 'nix develop .#ios' or 'nix develop .#android' for platform-specific shells)"
            echo "  node: $(node --version)"
            echo "  yarn: $(yarn --version)"
            echo "  rustc: $(rustc --version)"
          '';
        };
      };
    });
}
