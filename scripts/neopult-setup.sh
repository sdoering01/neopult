#!/bin/sh

PKG_DIR="/usr/local"
BIN_DIR="$PKG_DIR/bin"
DATA_DIR="$PKG_DIR/share/neopult"
DIST_NEOPULT_BINARY="$BIN_DIR/neopult"

SYSTEMD_SERVICE_DIR="/etc/systemd/system"

PLUGIN_DIR="$DATA_DIR/plugins"
SCRIPT_DIR="$DATA_DIR/scripts"
WEB_DIR="$DATA_DIR/web"

NEOPULT_USER="neopult"
NEOPULT_HOME="/home/$NEOPULT_USER"
CHANNEL_DEFAULTS_DIR="$NEOPULT_HOME/channel-defaults"

NEOPULT_CHANNELS=6

WORKING_DIR=$(pwd)

print_usage() {
    echo "Usage: $0 <install | channel-setup | full-install | uninstall | full-uninstall>"
    echo "Installs or uninstalls neopult on the system."
}

assert_repo_root() {
    if ! [ -f "$WORKING_DIR/Cargo.toml" ]; then
        echo "This action has to be run from the root of the neopult repository"
        exit 1
    fi
}

assert_neopult_built() {
    if ! [ -x "$WORKING_DIR/target/release/neopult" ]; then
        echo "Please build neopult first: cargo build --release"
        exit 1
    fi
}

install() {
    assert_repo_root
    assert_neopult_built

    echo "Copying neopult binary"
    cp "$WORKING_DIR/target/release/neopult" "$DIST_NEOPULT_BINARY"

    echo "Creating data directory"
    mkdir -p "$DATA_DIR"

    echo "Copying init-example.lua and neopult.lua"
    cp "$WORKING_DIR/init-example.lua" "$WORKING_DIR/neopult.lua" "$DATA_DIR"

    echo "Creating plugin directory"
    mkdir -p "$PLUGIN_DIR"
    echo "Copying plugins"
    cp -ra "$WORKING_DIR/plugins/." "$PLUGIN_DIR"

    echo "Copying web files"
    cp -ra "$WORKING_DIR/web/." "$WEB_DIR"

    echo "Creating script directory"
    mkdir -p "$SCRIPT_DIR"
    echo "Copying scripts"
    cp "$WORKING_DIR/scripts/neopult-setup.sh" "$WORKING_DIR/scripts/novnc-start.sh" "$WORKING_DIR/scripts/vnc-start.sh" "$SCRIPT_DIR"

    echo "Copying systemd service templates"
    cp -a "$WORKING_DIR/config/systemd/." "$SYSTEMD_SERVICE_DIR"

    echo "Creating user $NEOPULT_USER"
    useradd -m $NEOPULT_USER
}

channel_setup() {
    assert_repo_root

    echo "Creating directory for channel defaults"
    mkdir -p "$CHANNEL_DEFAULTS_DIR"
    mkdir -p "$CHANNEL_DEFAULTS_DIR/.config/zathura"
    mkdir -p "$CHANNEL_DEFAULTS_DIR/plugins"

    echo "Copying channel defaults"
    cp -ra "$WORKING_DIR/plugins/." "$CHANNEL_DEFAULTS_DIR"
    # Do not overwrite existing init script
    cp -n "$WORKING_DIR/init-example.lua" "$CHANNEL_DEFAULTS_DIR/init.lua"
    cp "$WORKING_DIR/assets/vnc/channel-banner.pdf" "$WORKING_DIR/assets/vnc/vncstartup" "$CHANNEL_DEFAULTS_DIR"
    cp "$WORKING_DIR/assets/vnc/zathurarc" "$CHANNEL_DEFAULTS_DIR/.config/zathura"

    echo
    echo "First enter a STRONG and SECRET password, then enter 'y' and enter the public view-only password"
    HOME="$CHANNEL_DEFAULTS_DIR" vncpasswd

    for channel in $(seq $NEOPULT_CHANNELS); do
        echo "Linking channel home of channel $channel to channel defaults"
        ln -s "$CHANNEL_DEFAULTS_DIR" "$NEOPULT_HOME/channel-$channel"
    done

    echo "Giving ownership of files in neopult home to user $NEOPULT_USER"
    find $NEOPULT_HOME -exec chown $NEOPULT_USER:$NEOPULT_USER {} \;

    echo
    echo "NOTE: Please review the default init.lua in $CHANNEL_DEFAULTS_DIR"
}

uninstall() {
    echo "Removing neopult binary"
    rm -f "$DIST_NEOPULT_BINARY"

    echo "Removing neopult data directory"
    rm -rf "$DATA_DIR"

    echo "Removing neopult systemd service templates"
    rm -f "$SYSTEMD_SERVICE_DIR/neopult@.service" "$SYSTEMD_SERVICE_DIR/neopult-novnc@.service" "$SYSTEMD_SERVICE_DIR/neopult-vncserver@.service"
}

full_uninstall() {
    uninstall

    echo "Removing user $NEOPULT_USER and its home directory"
    userdel -r $NEOPULT_USER
}

if [ $(id -u) != 0 ]; then
    echo "This script must be run as root"
    exit 1
fi

if [ "$#" != 1 ]; then
    print_usage
    exit 1
fi

case "$1" in
    install)
        install
        ;;

    channel-setup)
        channel_setup
        ;;

    full-install)
        install
        channel_setup
        ;;

    uninstall)
        uninstall
        ;;

    full-uninstall)
        full_uninstall
        ;;

    *)
        print_usage
        exit 1
esac
