#!/bin/sh

print_usage() {
    echo "Usage: $0 <CHANNEL> [-d]"
    echo "Starts a vnc server for the specified neopult channel. CHANNEL must be between 0 and 99."
    echo " -h, --help     Print this help."
    echo " -d             Go to background after starting vnc server. Useful for forking systemd services."
}

# TODO: regard specific channel home if it exists
export HOME="$(pwd)/neopult_home/channel-default"
export XDG_CONFIG_HOME="$HOME/.config"

if [ $# -lt 1 ]; then
    print_usage
    exit 1
fi

if [ $1 = "-h" ] || [ $1 = "--help" ]; then
    print_usage
    exit 0
fi

if ! [ -d "$HOME" ]; then
    echo "Neopult home does not exist. Please run the setup script (neopult-setup.sh) first"
    exit 1
fi

channel="$1"

if ! echo "$channel" | grep -P "^\d+$"; then
    echo "error: CHANNEL must be a number"
    exit 1
fi

if [ $channel -lt 0 ] || [ $channel -ge 100 ]; then
    echo "error: DISPLAY must be between 0 and 99"
    exit 1
fi


rfbport=$(printf "59%02d" $channel)
export DISPLAY=":$channel"
Xvnc $DISPLAY -auth "$HOME/.Xauthority" -rfbport $rfbport -geometry 1920x1080 -depth 24 -pn -localhost -rfbauth "$HOME/.vnc/passwd" -nocursor &

# Wait for vnc server to start
sleep 1

zathura --mode=presentation --page=$channel assets/vnc/channel-banner.pdf &

# For the sake of simplicity just block by sleeping
if [ "$2" != "-d" ]; then
    sleep 1e100
else
    echo "Going to background"
fi
