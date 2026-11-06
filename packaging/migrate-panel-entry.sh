#!/usr/bin/env bash
# Replaces an applet ID in the COSMIC panel and dock applet lists of the current user,
# so an applet added under an old ID stays in the panel after an ID change.
#
# Usage: migrate-panel-entry.sh <old-id> <new-id>
set -euo pipefail

old_id="${1:?old applet id}"
new_id="${2:?new applet id}"
config_root="${XDG_CONFIG_HOME:-${HOME}/.config}/cosmic"
# Backups go outside the cosmic-config tree so they are not read as config keys.
backup_root="${XDG_STATE_HOME:-${HOME}/.local/state}/cosmic-ext-translator/panel-backup"

[[ -d "${config_root}" ]] || exit 0

shopt -s nullglob
for list in "${config_root}"/com.system76.CosmicPanel.*/v*/plugins_*; do
    if grep -qF "\"${old_id}\"" "${list}"; then
        rel="${list#"${config_root}"/}"
        mkdir -p "${backup_root}/$(dirname "${rel}")"
        cp -p "${list}" "${backup_root}/${rel}"
        sed -i "s/\"${old_id//./\\.}\"/\"${new_id}\"/g" "${list}"
        echo "Updated panel applet entry in ${list} (backup in ${backup_root})"
    fi
done
