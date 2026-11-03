#!/bin/sh
set -e

PREFIX="${PREFIX:-/usr}"

echo "Removing cosmic-ext-translator from ${PREFIX}..."
rm -f "${PREFIX}/bin/cosmic-applet-translator"
rm -f "${PREFIX}/bin/cosmic-translator-overlay"
rm -f "${PREFIX}/share/applications/com.system76.CosmicExtTranslator.desktop"
rm -f "${PREFIX}/share/metainfo/com.system76.CosmicExtTranslator.metainfo.xml"
rm -f "${PREFIX}/share/icons/hicolor/scalable/apps/com.system76.CosmicExtTranslator.svg"

echo "Uninstallation complete."
