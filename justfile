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
