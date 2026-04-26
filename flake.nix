{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        fenixPkgs = fenix.packages.${system};
        lib = pkgs.lib;
        sourceRoot = ./.;
        cleanSrc = pkgs.lib.cleanSourceWith {
          src = sourceRoot;
          filter = path: type:
            let
              pathStr = toString path;
              rootStr = toString sourceRoot;
              rel =
                if pathStr == rootStr then
                  ""
                else
                  pkgs.lib.removePrefix "${rootStr}/" pathStr;
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
              "frontend"
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
        addDarwinInstallNameTool = tool:
          if isDarwin then
            tool.overrideAttrs (old: {
              nativeBuildInputs =
                (old.nativeBuildInputs or [ ]) ++ [ pkgs.darwin.cctools ];
            })
          else
            tool;
        toolchain = fenixPkgs.combine (map addDarwinInstallNameTool [
          fenixPkgs.stable.cargo
          fenixPkgs.stable.clippy
          fenixPkgs.stable.rust-src
          fenixPkgs.stable.rustc
          fenixPkgs.stable.rustfmt
          fenixPkgs.stable.rust-analyzer
          fenixPkgs.targets.wasm32-unknown-unknown.stable.rust-std
        ]);
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
        mkKeepbookPackage = {
          pname,
          cargoPackage ? "keepbook",
          buildFeatures ? [ ],
          extraBuildInputs ? [ ],
          extraNativeBuildInputs ? [ ],
        }:
          rustPlatform.buildRustPackage {
            inherit pname buildFeatures;
            version = "0.1.1";
            src = cleanSrc;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            cargoBuildFlags = [ "-p" cargoPackage ];
            cargoTestFlags = [ "-p" cargoPackage ];
            checkFlags = [ "--skip" "contracts_match_both_clis" ];
            nativeBuildInputs = [ pkgs.pkg-config ] ++ extraNativeBuildInputs;
            buildInputs = extraBuildInputs;
          };
      in
      {
        packages = {
          default = mkKeepbookPackage { pname = "keepbook"; };
          keepbook = mkKeepbookPackage { pname = "keepbook"; };
        } // lib.optionalAttrs isLinux {
          keepbook-tray = mkKeepbookPackage {
            pname = "keepbook-tray";
            buildFeatures = [ "tray" ];
            extraBuildInputs = [ pkgs.dbus ];
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            toolchain
            pkgs.pkg-config
            pkgs.dioxus-cli
            pkgs.openssl
            pkgs.just
            pkgs.jq
            pkgs.nodejs_22
            pkgs.yarn
          ] ++ lib.optionals isLinux [
            pkgs.dbus
          ];

          OPENSSL_NO_VENDOR = "1";
        };
      }
    );
}
