#!/bin/sh

# Sets up the development environment. Run this script outside of tmux or in a
# fresh tmux session.
#
# Requires:
#  - tmux
#  - janus (systemd service)
#  - nvim
#  - python
#  - novnc

if [ -z $TMUX ]; then
    tmux new-session "$0"
else
    sudo systemctl start janus

    tmux new-window -t 0 2>/dev/null
    tmux send -t 0 'nvim .'

    tmux new-window -t 2 -e 'DISPLAY=:5' -e 'RUST_LOG=debug' -e 'NEOPULT_CHANNEL=5' -e "NEOPULT_HOME=$(pwd)/neopult_home"

    tmux new-window -t 3 -c web
    tmux send -t 3 'nvim .'


    vnc_cmd="./scripts/vnc-start.sh 5"
    if [ -d neopult_home ]; then
        vnc_window_cmd="$vnc_cmd"
        neopult_setup=true
    else
        vnc_window_cmd="./scripts/neopult-setup.sh && $vnc_cmd"
        neopult_setup=false
    fi
    tmux new-window -t 4 -n xvnc
    tmux send -t 4 "$vnc_window_cmd"

    tmux new-window -t 5 -c ../cvh-camera/sender -n sender
    tmux send -t 5 'python -m http.server 3000'

    tmux new-window -t 6 -n novnc
    tmux send -t 6 './scripts/novnc-start.sh 5'

    if $neopult_setup; then
        tmux select-window -t 1
    else
        tmux select-window -t 4
    fi

    # Swaps rust nvim window with the window that runs this script and then
    # kills that window. This has to happen at the end of the script.
    tmux swap-window -s 0 -t 1
    tmux kill-window -t 0
fi
