{
  description = "Hegel for Rust";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.tar.gz";
  };

  outputs =
    {
      self,
      nixpkgs,
      ...
    }:
    let
      forAllSystems = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;
    in
    {
      # Build the native engine cdylib (`libhegel.so`) used by the C-ABI
      # bindings and other language bindings (e.g. hegel-ocaml's ctypes
      # loader). Callers may override `cargoDeps` to plug in their own
      # vendoring; the default uses `importCargoLock` against the workspace `Cargo.lock`.
      lib.mkLibhegel =
        {
          pkgs,
          cargoDeps ? pkgs.rustPlatform.importCargoLock { lockFile = ../Cargo.lock; },
        }:
        let
          cargoTomlLines = builtins.filter builtins.isString (
            builtins.split "\n" (builtins.readFile ../hegel-c/Cargo.toml)
          );
          versionLine = builtins.head (
            builtins.filter (l: builtins.match ''version = "[^"]+"'' l != null) cargoTomlLines
          );
          version = builtins.elemAt (builtins.match ''version = "([^"]+)"'' versionLine) 0;
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = "libhegel";
          inherit version cargoDeps;
          src = ../.;
          cargoBuildFlags = [
            "-p"
            "hegeltest-c"
          ];
          doCheck = false;
          # buildRustPackage's default install only handles binaries; we need
          # the cdylib. The artifact may land in target/<triple>/release/ or
          # target/release/ depending on whether --target is in play.
          postInstall = ''
            mkdir -p $out/lib
            find target -name 'libhegel.so' -path '*/release/*' -exec cp {} $out/lib/ \;
          '';
        };

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
              pkgs.rust-analyzer
              pkgs.just
              pkgs.cargo-expand
              pkgs.python3
            ];
          };
        }
      );
    };
}
