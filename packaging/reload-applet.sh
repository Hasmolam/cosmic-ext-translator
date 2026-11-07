#!/usr/bin/env bash
# Makes a new install take effect without restarting cosmic-panel.
#
#   reload-applet.sh reload   ask the running panel applet to restart, stop the HUD
#   reload-applet.sh stop     stop the panel applet and the HUD (uninstall)
#
# Restarting the whole panel several times in a row makes cosmic-session delay the next
# start by minutes and can leave the panel without its notifications connection.
#
# On SIGHUP the applet exits with code 75, which makes cosmic-panel start it again from
# the installed binary. An applet stopped by a signal is not restarted by the panel.
# The HUD is started again by the applet on the next capture.
set -euo pipefail

mode="${1:-reload}"
uid="$(id -u)"
# Anchored so that only the processes themselves match, not shells whose command line
# contains these names.
applet='^([^ ]*/)?cosmic-applet-translator --applet$'
hud='^([^ ]*/)?cosmic-translator-overlay --hud$'

pids() {
    pgrep -u "${uid}" -f "$1" || true
}

# Sends a signal to all processes matching a pattern. Returns 1 if none matched.
# pgrep + kill instead of pkill: procps-ng 4.0.4 pkill parses -HUP as -H and rejects
# --signal HUP.
signal_all() {
    local sig="$1" pattern="$2" found
    found="$(pids "${pattern}")"
    [[ -n "${found}" ]] || return 1
    # Word splitting of the PID list is intended.
    # shellcheck disable=SC2086
    kill -s "${sig}" ${found}
}

if signal_all TERM "${hud}"; then
    echo "HUD stopped; the applet starts it again on the next capture."
fi

case "${mode}" in
    stop)
        if signal_all TERM "${applet}"; then
            echo "Panel applet stopped. Remove it from the panel in Settings > Desktop > Panel > Applets."
        fi
        ;;
    reload)
        old="$(pids "${applet}")"
        if [[ -z "${old}" ]]; then
            echo "Panel applet is not running; nothing to reload."
            exit 0
        fi
        signal_all HUP "${applet}" || true
        # The panel restarts an exited applet within a few milliseconds.
        for _ in $(seq 1 20); do
            sleep 0.25
            new="$(pids "${applet}")"
            if [[ -n "${new}" && "${new}" != "${old}" ]]; then
                echo "Panel applet restarted with the installed binary."
                exit 0
            fi
        done
        echo "Panel applet did not restart. Versions before the reload support exit on SIGHUP"
        echo "without being restarted; log out and back in once to load the new version."
        ;;
    *)
        echo "usage: $0 [reload|stop]" >&2
        exit 2
        ;;
esac
