{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer
    pkg-config

    # Wayland (опционально, если оставляете wayland backend)
    wayland
    wayland-protocols
    libxkbcommon

    # X11 и OpenGL (необходимо для glow backend)
    xorg.libX11
    xorg.libXrandr
    xorg.libXinerama
    xorg.libXcursor
    xorg.libXi
    mesa
    libglvnd

    # Дополнительно
    fontconfig
    noto-fonts-color-emoji
  ];

  # Правильная настройка LD_LIBRARY_PATH
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.xorg.libX11
    pkgs.mesa
    pkgs.libglvnd
  ];

  # (Необязательно) принудительно использовать X11 backend
  # WINIT_UNIX_BACKEND = "x11";
}
