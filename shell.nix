{ pkgs ? import <nixpkgs>
  {
    overlays = [ (import <rust-overlay>) ];
  }
}:
  pkgs.mkShell {
    nativeBuildInputs = [
      pkgs.rust-bin.stable.latest.default
      pkgs.gnumake
    ];
  }
