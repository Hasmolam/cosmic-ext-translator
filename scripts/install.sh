#!/bin/sh
set -e

PREFIX="${PREFIX:-/usr}"

echo "Installing cosmic-ext-translator to ${PREFIX}..."
install -Dm0755 cosmic-applet-translator "${PREFIX}/bin/cosmic-applet-translator"
install -Dm0755 cosmic-translator-overlay "${PREFIX}/bin/cosmic-translator-overlay"
install -Dm0644 com.system76.CosmicExtTranslator.desktop "${PREFIX}/share/applications/com.system76.CosmicExtTranslator.desktop"
install -Dm0644 com.system76.CosmicExtTranslator.metainfo.xml "${PREFIX}/share/metainfo/com.system76.CosmicExtTranslator.metainfo.xml"
install -Dm0644 com.system76.CosmicExtTranslator.svg "${PREFIX}/share/icons/hicolor/scalable/apps/com.system76.CosmicExtTranslator.svg"

echo "Installation complete!"
echo "Add the applet to your panel: Settings -> Desktop -> Panel -> Configure panel applets -> Add applet -> Screen Translator"
