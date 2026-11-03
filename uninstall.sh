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

# Remove from ~/.local
rm -f "${HOME}/.local/bin/cosmic-applet-translator"
rm -f "${HOME}/.local/bin/cosmic-translator-overlay"
rm -f "${HOME}/.local/share/applications/com.system76.CosmicExtTranslator.desktop"
rm -f "${HOME}/.local/share/metainfo/com.system76.CosmicExtTranslator.metainfo.xml"
rm -f "${HOME}/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicExtTranslator.svg"

# Remove from /usr if run with sudo
if [[ $EUID -eq 0 ]]; then
    rm -f "/usr/bin/cosmic-applet-translator"
    rm -f "/usr/bin/cosmic-translator-overlay"
    rm -f "/usr/share/applications/com.system76.CosmicExtTranslator.desktop"
    rm -f "/usr/share/metainfo/com.system76.CosmicExtTranslator.metainfo.xml"
    rm -f "/usr/share/icons/hicolor/scalable/apps/com.system76.CosmicExtTranslator.svg"
fi

if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true
fi

killall cosmic-panel 2>/dev/null || true

echo -e "${GREEN}${BOLD}✓ Success!${NC} COSMIC Screen Translator has been removed.\n"
