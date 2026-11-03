rootdir := ''
prefix := '/usr'
destdir := rootdir + prefix
bindir := destdir + '/bin'
appid := 'com.system76.CosmicExtTranslator'
desktop_dst := destdir + '/share/applications/' + appid + '.desktop'
metainfo_dst := destdir + '/share/metainfo/' + appid + '.metainfo.xml'
icon_dst := destdir + '/share/icons/hicolor/scalable/apps/' + appid + '.svg'

home := env_var('HOME')
export PKG_CONFIG_PATH := env_var_or_default('PKG_CONFIG_PATH', '') + ':' + home + '/.local/lib/pkgconfig:' + home + '/.local/lib/x86_64-linux-gnu/pkgconfig:/usr/local/lib/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig'

# Default recipe: check the workspace
default: check

# Check all workspace crates
check:
    cargo check --workspace --all-targets

# Run unit tests across workspace
test:
    cargo test --workspace

# Run clippy on workspace
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Build debug binaries
build *args:
    cargo build --workspace {{args}}

# Build release binaries
build-release *args:
    cargo build --release --workspace {{args}}

# Build Debian/Ubuntu package (.deb)
deb: build-release
    rm -rf target/deb-pkg
    mkdir -p target/deb-pkg/DEBIAN
    mkdir -p target/deb-pkg/usr/bin
    mkdir -p target/deb-pkg/usr/share/applications
    mkdir -p target/deb-pkg/usr/share/metainfo
    mkdir -p target/deb-pkg/usr/share/icons/hicolor/scalable/apps
    install -Dm0755 target/release/cosmic-applet-translator target/deb-pkg/usr/bin/cosmic-applet-translator
    install -Dm0755 target/release/cosmic-translator-overlay target/deb-pkg/usr/bin/cosmic-translator-overlay
    strip target/deb-pkg/usr/bin/*
    install -Dm0644 crates/cosmic-applet-translator/target/xdgen/app.desktop target/deb-pkg/usr/share/applications/{{appid}}.desktop
    install -Dm0644 crates/cosmic-applet-translator/target/xdgen/app.metainfo.xml target/deb-pkg/usr/share/metainfo/{{appid}}.metainfo.xml
    install -Dm0644 crates/cosmic-applet-translator/resources/icon.svg target/deb-pkg/usr/share/icons/hicolor/scalable/apps/{{appid}}.svg
    printf "Package: cosmic-ext-translator\nVersion: 0.1.0\nSection: utils\nPriority: optional\nArchitecture: amd64\nMaintainer: Hasmolam <hasanhuseyinyolcu25@gmail.com>\nDepends: libc6, libpipewire-0.3-0, libxkbcommon0\nDescription: Real-time Wayland screen and game subtitle translator for COSMIC Desktop\n Translates on-screen text in games and visual novels on the COSMIC desktop.\n It reads a screen region you choose, runs OCR locally, and shows the\n translation as a subtitle line on top of the game.\n" > target/deb-pkg/DEBIAN/control
    dpkg-deb --build --root-owner-group target/deb-pkg target/cosmic-ext-translator_0.1.0_amd64.deb

# Install both binaries and desktop integration files
install:
    install -Dm0755 target/release/cosmic-applet-translator {{bindir}}/cosmic-applet-translator
    install -Dm0755 target/release/cosmic-translator-overlay {{bindir}}/cosmic-translator-overlay
    install -Dm0644 crates/cosmic-applet-translator/target/xdgen/app.desktop {{desktop_dst}}
    install -Dm0644 crates/cosmic-applet-translator/target/xdgen/app.metainfo.xml {{metainfo_dst}}
    install -Dm0644 crates/cosmic-applet-translator/resources/icon.svg {{icon_dst}}

# Uninstall binaries and desktop integration files
uninstall:
    rm -f {{bindir}}/cosmic-applet-translator
    rm -f {{bindir}}/cosmic-translator-overlay
    rm -f {{desktop_dst}}
    rm -f {{metainfo_dst}}
    rm -f {{icon_dst}}

# Install for current user only to ~/.local (no root required)
install-user:
    install -Dm0755 target/release/cosmic-applet-translator {{home}}/.local/bin/cosmic-applet-translator
    install -Dm0755 target/release/cosmic-translator-overlay {{home}}/.local/bin/cosmic-translator-overlay
    install -Dm0644 crates/cosmic-applet-translator/target/xdgen/app.desktop {{home}}/.local/share/applications/{{appid}}.desktop
    install -Dm0644 crates/cosmic-applet-translator/target/xdgen/app.metainfo.xml {{home}}/.local/share/metainfo/{{appid}}.metainfo.xml
    install -Dm0644 crates/cosmic-applet-translator/resources/icon.svg {{home}}/.local/share/icons/hicolor/scalable/apps/{{appid}}.svg
    update-desktop-database {{home}}/.local/share/applications 2>/dev/null || true
    killall cosmic-panel 2>/dev/null || true

# Uninstall user-local installation
uninstall-user:
    rm -f {{home}}/.local/bin/cosmic-applet-translator
    rm -f {{home}}/.local/bin/cosmic-translator-overlay
    rm -f {{home}}/.local/share/applications/{{appid}}.desktop
    rm -f {{home}}/.local/share/metainfo/{{appid}}.metainfo.xml
    rm -f {{home}}/.local/share/icons/hicolor/scalable/apps/{{appid}}.svg
    update-desktop-database {{home}}/.local/share/applications 2>/dev/null || true
    killall cosmic-panel 2>/dev/null || true

# Run capture test utility
test-capture:
    cargo run -p translator-core --example test_capture

# Run end-to-end automated integration tests across all components
test-e2e:
    cargo test --test e2e_debouncer_sim --test e2e_capture_pipeline --test e2e_ocr_accuracy -p translator-core
    cargo test --test e2e_clickthrough -p cosmic-translator-overlay
    cargo test --test e2e_ui_workflow -p cosmic-applet-translator

# Run panel applet
run-applet:
    cargo run -p cosmic-applet-translator
