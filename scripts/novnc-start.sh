#!/bin/sh

if [ -z "$1" ]; then
    echo "Usage: $0 NEOPULT_CHANNEL [NOVNC_SCRIPT]"
    echo "Starts a novnc server for the specified NEOPULT_CHANNEL. NEOPULT_CHANNEL has to be a number from 0 to 99."
    echo "NOVNC_SCRIPT is the path to the novnc launch script and has to be specified, when this script can't auto-detect its location."
    exit 1
fi

if [ -n "$2" ]; then
    novnc="$2"
else
    os_name=$(grep -oP '(?<=^NAME=").*(?=")' /etc/os-release)
    case "$os_name" in
        "Arch Linux")
            novnc="/usr/bin/novnc"
            ;;
        "Ubuntu" | "Debian")
            novnc="/usr/share/novnc/utils/launch.sh"
            ;;
        *)
            echo "Your distribution is not supported directly, please specifiy the NOVNC_SCRIPT parameter"
            exit 1
            ;;
    esac
fi

neopult_channel="$1"
vnc_port=$(expr 5900 + $neopult_channel)
websockify_port=$(expr 6080 + $neopult_channel)
$novnc --listen $websockify_port --vnc localhost:$vnc_port
