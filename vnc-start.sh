#!/bin/sh

export HOME=$(pwd)/vnc_home

if ! [ -d $HOME ]; then
    echo "Please run the setup script (vnc-setup.sh) first"
    exit 1
fi


Xvnc :5 -auth $HOME/.Xauthority -rfbport 5905 -geometry 1920x1080 -depth 24 -pn -localhost -rfbauth $HOME/.vnc/passwd
