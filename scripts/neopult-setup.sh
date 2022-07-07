#!/bin/sh

NEOPULT_HOME="$(pwd)/neopult_home"
DEFAULT_CHANNEL_HOME="$NEOPULT_HOME/channel-default"

echo "Creating neopult home"
mkdir -p "$HOME"
echo "Creating default channel home"
mkdir -p "$DEFAULT_CHANNEL_HOME"

echo "Copying zathura config"
ZATHURA_CONFIG_DIR="$DEFAULT_CHANNEL_HOME/.config/zathura"
mkdir -p "$ZATHURA_CONFIG_DIR"
cp -p assets/vnc/zathurarc "$ZATHURA_CONFIG_DIR"

export HOME="$DEFAULT_CHANNEL_HOME"

echo "Setting up default password for the VNC server"
vncpasswd
