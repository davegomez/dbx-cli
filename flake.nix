{
  description = "Agent-first Dropbox CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rootCargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        cliCargoToml = builtins.fromTOML (builtins.readFile ./crates/dbx-cli/Cargo.toml);
        version = rootCargoToml.workspace.package.version;

        dbx = pkgs.rustPlatform.buildRustPackage {
          pname = "dbx";
          inherit version;

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          cargoBuildFlags = [ "--package" "dbx-cli" ];
          cargoCheckFlags = [ "--workspace" "--all-targets" ];

          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];

          meta = with pkgs.lib; {
            description = cliCargoToml.package.description;
            homepage = rootCargoToml.workspace.package.homepage;
            license = licenses.agpl3Plus;
            mainProgram = "dbx";
          };
        };
      in
      {
        packages.default = dbx;
        packages.dbx = dbx;

        apps.default = flake-utils.lib.mkApp {
          drv = dbx;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ dbx ];
          packages = with pkgs; [
            cargo
            clippy
            rust-analyzer
            rustc
            rustfmt
          ];
        };
      });
}
