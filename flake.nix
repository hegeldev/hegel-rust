{
  description = "Hegel Rust SDK";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    hegel.url = "git+ssh://git@github.com/antithesishq/hegel";
  };

  outputs =
    { nixpkgs, hegel, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "hegel";
        version = "0.1.0";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;

        # hegel binary on PATH so build.rs finds it
        nativeBuildInputs = [ hegel.packages.${system}.default ];

      };

      devShells.${system}.default = pkgs.mkShell {
        buildInputs = [
          pkgs.cargo
          pkgs.rustc
          hegel.packages.${system}.default
        ];
      };
    };
}
