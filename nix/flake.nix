{
  description = "Hegel for Rust";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    # note: this version is automatically bumped when we update hegel-core, do not update manually
    hegel.url = "git+https://github.com/hegeldev/hegel-core?dir=nix&ref=refs/tags/v0.2.1"; # git+https instead of github so that we can use the ref parameter
    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.tar.gz";
  };

  outputs =
    {
      self,
      nixpkgs,
      hegel,
      ...
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ hegel.packages.${system}.default ];
            buildInputs = [
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
              pkgs.just
              hegel.packages.${system}.default
            ];
            HEGEL_SERVER_COMMAND = "${hegel.packages.${system}.default}/bin/hegel";
          };
        }
      );
    };
}
