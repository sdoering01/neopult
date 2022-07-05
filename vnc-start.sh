#!/bin/sh

export HOME="$(pwd)/vnc_home"
export XDG_CONFIG_HOME="$HOME/.config"

if ! [ -d "$HOME" ]; then
    echo "Please run the setup script (vnc-setup.sh) first"
    exit 1
fi

Xvnc :5 -auth "$HOME/.Xauthority" -rfbport 5905 -geometry 1920x1080 -depth 24 -pn -localhost -rfbauth "$HOME/.vnc/passwd" -nocursor &

# Wait for vnc server to start
sleep 1

export DISPLAY=:5
zathura --mode=presentation --page=1 assets/vnc/channel-banner.pdf
