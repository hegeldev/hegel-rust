{
  description = "Hegel Rust SDK";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    hegel.url = "git+ssh://git@github.com/antithesishq/hegel-core?dir=nix&ref=refs/tags/v0.4.0";
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
              pkgs.just
              hegel.packages.${system}.default
            ];
            HEGEL_SERVER_COMMAND = "${hegel.packages.${system}.default}/bin/hegel";
          };
        }
      );
    };
}
