{ pkgs ? import <nixpkgs>
  {
    overlays = [ (import <rust-overlay>) ];
  }
}:
  pkgs.mkShell {
    nativeBuildInputs = [
      pkgs.rust-bin.stable.latest.default
      pkgs.gnumake
      pkgs.gmock
      pkgs.glxinfo
      pkgs.vulkan-tools
      pkgs.xorg.libX11
      pkgs.xorg.libXrandr
    ];
    LD_LIBRARY_PATH = with pkgs.xlibs; "${pkgs.vulkan-loader}/lib:${pkgs.mesa}/lib:${libX11}/lib:${libXcursor}/lib:${libXxf86vm}/lib:${libXi}/lib:${libXrandr}/lib";
    DISPLAY = ":0";
  }
