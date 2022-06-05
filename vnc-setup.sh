#!/bin/sh

export HOME=$(pwd)/vnc_home

echo "Creating vnc home"
mkdir -p $HOME

echo "Setting up password for the VNC server"
vncpasswd
