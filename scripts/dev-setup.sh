#!/bin/sh

NEOPULT_HOME="$(pwd)/neopult_home"
CHANNEL_HOME="$NEOPULT_HOME/channel-5"
NEOPULT_SOURCE="$(pwd)/neopult"

echo "Creating neopult home"
mkdir -p "$HOME"
echo "Creating channel home"
mkdir -p "$CHANNEL_HOME"

echo "Linking init.lua to channel home"
ln -s "$NEOPULT_SOURCE/init.lua" "$CHANNEL_HOME"

echo "Linking plugin directory to channel home"
ln -s "$NEOPULT_SOURCE/plugins" "$CHANNEL_HOME"

echo "Linking vncstartup script to channel home"
ln -s "$NEOPULT_SOURCE/assets/vnc/vncstartup" "$CHANNEL_HOME"

echo "Linking channel banner"
ln -s "$NEOPULT_SOURCE/assets/vnc/channel-banner.pdf" "$CHANNEL_HOME"

echo "Linking zathura config to channel home"
ZATHURA_CONFIG_DIR="$CHANNEL_HOME/.config/zathura"
mkdir -p "$ZATHURA_CONFIG_DIR"
ln -s "$NEOPULT_SOURCE/assets/vnc/zathurarc" "$ZATHURA_CONFIG_DIR"

export HOME="$CHANNEL_HOME"

echo "Setting up default password for the VNC server"
vncpasswd
