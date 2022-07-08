#!/bin/sh

NEOPULT_HOME="$(pwd)/neopult_home"
DEFAULT_CHANNEL_HOME="$NEOPULT_HOME/channel-default"

echo "Creating neopult home"
mkdir -p "$HOME"
echo "Creating default channel home"
mkdir -p "$DEFAULT_CHANNEL_HOME"

echo "Linking init.lua to default channel home"
ln -s "$(pwd)/init.lua" "$DEFAULT_CHANNEL_HOME"

echo "Linking plugin directory to default channel home"
ln -s "$(pwd)/plugins" "$DEFAULT_CHANNEL_HOME"

echo "Linking vncstartup script to default channel home"
ln -s "$(pwd)/assets/vnc/vncstartup" "$DEFAULT_CHANNEL_HOME"

echo "Linking zathura config to default channel home"
ZATHURA_CONFIG_DIR="$DEFAULT_CHANNEL_HOME/.config/zathura"
mkdir -p "$ZATHURA_CONFIG_DIR"
ln -s "$(pwd)/assets/vnc/zathurarc" "$ZATHURA_CONFIG_DIR"

export HOME="$DEFAULT_CHANNEL_HOME"

echo "Setting up default password for the VNC server"
vncpasswd
