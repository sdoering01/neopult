#/usr/bin/sh

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

    tmux new-window -t 2 -e 'DISPLAY=:5' -e 'RUST_LOG=debug'

    tmux new-window -t 3 -c web
    tmux send -t 3 'nvim .'

    if [ -d vnc_home ]; then
        vnc_cmd="./vnc-start.sh"
        vnc_setup=true
    else
        vnc_cmd="./vnc-setup.sh && ./vnc-start.sh"
        vnc_setup=false
    fi
    tmux new-window -t 4 -n xvnc "$vnc_cmd"

    tmux new-window -t 5 -c ../cvh-camera/sender -n sender python -m http.server 3000

    tmux new-window -t 6 -n novnc novnc --vnc localhost:5905

    if $vnc_setup; then
        tmux select-window -t 1
    else
        tmux select-window -t 4
    fi

    # Swaps rust nvim window with the window that runs this script and then
    # kills that window. This has to happen at the end of the script.
    tmux swap-window -s 0 -t 1
    tmux kill-window -t 0
fi
