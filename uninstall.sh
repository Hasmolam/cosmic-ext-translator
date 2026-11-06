#!/usr/bin/env bash
# ==============================================================================
# COSMIC Screen Translator (cosmic-ext-translator) - Uninstaller
# Author: Hasan Hüseyin Yolcu <hasanhuseyinyolcu25@gmail.com>
# License: GPL-3.0-or-later
# ==============================================================================

set -euo pipefail

BLUE='\033[0;34m'
GREEN='\033[0;32m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${BLUE}${BOLD}=== COSMIC Screen Translator Uninstaller ===${NC}\n"

# Current and 0.1.x application IDs.
APP_IDS=("io.github.hasmolam.CosmicExtTranslator" "com.system76.CosmicExtTranslator")

remove_from() {
    local prefix="$1" id
    rm -f "${prefix}/bin/cosmic-applet-translator" "${prefix}/bin/cosmic-translator-overlay"
    for id in "${APP_IDS[@]}"; do
        rm -f "${prefix}/share/applications/${id}.desktop" \
            "${prefix}/share/metainfo/${id}.metainfo.xml" \
            "${prefix}/share/icons/hicolor/scalable/apps/${id}.svg"
    done
}

remove_from "${HOME}/.local"

# Remove from /usr if run with sudo
if [[ $EUID -eq 0 ]]; then
    remove_from "/usr"
fi

# Settings (~/.config/cosmic/<id>) and OCR models (~/.local/share/cosmic-ext-translator)
# are kept. Delete them manually for a full removal.

if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true
fi

killall cosmic-panel 2>/dev/null || true

echo -e "${GREEN}${BOLD}Removed.${NC} Settings and downloaded OCR models were kept.\n"
