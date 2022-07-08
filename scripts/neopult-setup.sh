#!/bin/sh

NEOPULT_HOME="$(pwd)/neopult_home"
CHANNEL_HOME="$NEOPULT_HOME/channel-5"

echo "Creating neopult home"
mkdir -p "$HOME"
echo "Creating channel home"
mkdir -p "$CHANNEL_HOME"

echo "Linking init.lua to channel home"
ln -s "$(pwd)/init.lua" "$CHANNEL_HOME"

echo "Linking plugin directory to channel home"
ln -s "$(pwd)/plugins" "$CHANNEL_HOME"

echo "Linking vncstartup script to channel home"
ln -s "$(pwd)/assets/vnc/vncstartup" "$CHANNEL_HOME"

echo "Linking zathura config to channel home"
ZATHURA_CONFIG_DIR="$CHANNEL_HOME/.config/zathura"
mkdir -p "$ZATHURA_CONFIG_DIR"
ln -s "$(pwd)/assets/vnc/zathurarc" "$ZATHURA_CONFIG_DIR"

export HOME="$CHANNEL_HOME"

echo "Setting up default password for the VNC server"
vncpasswd
