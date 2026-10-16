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

# Build release binaries
build-release:
    cargo build --release --workspace

# Run capture test utility
test-capture:
    cargo run -p translator-core --example test_capture

# Run panel applet
run-applet:
    cargo run -p cosmic-applet-translator
