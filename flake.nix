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
        toolchain = fenixPkgs.stable.withComponents [
          "cargo"
          "clippy"
          "rust-src"
          "rustc"
          "rustfmt"
          "rust-analyzer"
        ];
        mkKeepbookPackage = {
          pname,
          buildFeatures ? [ ],
          extraBuildInputs ? [ ],
          extraNativeBuildInputs ? [ ],
        }:
          pkgs.rustPlatform.buildRustPackage {
            inherit pname buildFeatures;
            version = "0.1.0";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            nativeBuildInputs = [ pkgs.pkg-config ] ++ extraNativeBuildInputs;
            buildInputs = extraBuildInputs;
          };
      in
      {
        packages = {
          default = mkKeepbookPackage { pname = "keepbook"; };
          keepbook = mkKeepbookPackage { pname = "keepbook"; };
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
            pkgs.just
            pkgs.nodejs_22
            pkgs.yarn
            pkgs.dbus
          ];
        };
      }
    );
}
