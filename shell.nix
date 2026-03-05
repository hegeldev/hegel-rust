let

  pkgs = import <nixpkgs> {};

in pkgs.mkShell {

  packages = with pkgs; [
    just
    uv
    cargo
    rustc
    rustfmt
    clippy
  ];

}
