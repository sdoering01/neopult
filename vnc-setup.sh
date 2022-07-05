#!/bin/sh

export HOME="$(pwd)/vnc_home"

echo "Creating vnc home"
mkdir -p "$HOME"

echo "Copying zathura config"
ZATHURA_CONFIG_DIR="$HOME/.config/zathura"
mkdir -p "$ZATHURA_CONFIG_DIR"
cp -p assets/vnc/zathurarc "$ZATHURA_CONFIG_DIR"

echo "Setting up password for the VNC server"
vncpasswd
