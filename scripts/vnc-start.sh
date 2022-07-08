#!/bin/sh

print_usage() {
    echo "Usage: $0 <CHANNEL> [-d]"
    echo "Starts a vnc server for the specified neopult channel. CHANNEL must be between 0 and 99."
    echo " -h, --help     Print this help."
    echo " -d             Go to background after starting vnc server. Useful for forking systemd services."
}

neopult_home="$(pwd)/neopult_home"

if [ $# -lt 1 ]; then
    print_usage
    exit 1
fi

if [ $1 = "-h" ] || [ $1 = "--help" ]; then
    print_usage
    exit 0
fi

if ! [ -d "$neopult_home" ]; then
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

channel_home="$neopult_home/channel-$channel"
if ! [ -d "$channel_home" ]; then
    echo "Channel home directory does not exist. Please run the setup script (neopult-setup.sh) first"
    exit 1
fi

rfbport=$(printf "59%02d" $channel)
export DISPLAY=":$channel"
Xvnc $DISPLAY -auth "$channel_home/.Xauthority" -rfbport $rfbport -geometry 1920x1080 -depth 24 -pn -localhost -rfbauth "$channel_home/.vnc/passwd" -nocursor &

# Wait for vnc server to start
sleep 1

export HOME="$channel_home"
export NEOPULT_CHANNEL=$channel
vncstartup="$channel_home/vncstartup"
if [ -x "$vncstartup" ]; then
    "$vncstartup"
fi

# For the sake of simplicity just block by sleeping
if [ "$2" != "-d" ]; then
    sleep 1e100
else
    echo "Going to background"
fi
