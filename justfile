set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# List the available recipes
default:
    @just --list

# Run the desktop app with hot reload
dev:
    pnpm --filter @frosty/desktop tauri dev

# Build the desktop app and its installer
build:
    pnpm --filter @frosty/desktop tauri build

# Run all Rust and JS tests
test:
    cargo test --workspace
    pnpm test

# Check formatting, lints, and types without changing files
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    pnpm format:check
    pnpm lint
    pnpm typecheck

# Format all Rust and JS code
fmt:
    cargo fmt --all
    pnpm format
