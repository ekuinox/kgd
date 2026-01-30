# KGD Project Justfile

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# heic-support feature は Linux 向け（libheif の embedded ビルドが必要）
# Windows ではデフォルト feature を無効化してビルドする
cargo_features := if os() == "windows" { "--no-default-features" } else { "" }

# Default recipe (show available commands)
default:
    @just --list

# Run all validation checks (fmt, check, clippy)
validate: fmt check clippy
    @echo "✅ All validation checks passed!"

# Format code with cargo fmt
fmt:
    @echo "Running cargo fmt..."
    cargo fmt --all

# Check code with cargo check
check:
    @echo "Running cargo check..."
    cargo check --all-targets {{ cargo_features }}

# Run clippy linter
clippy:
    @echo "Running cargo clippy..."
    cargo clippy --all-targets {{ cargo_features }} -- -D warnings

# Run cargo-deny checks (advisories, licenses, bans, sources)
deny:
    @echo "Running cargo deny..."
    cargo deny check

# Check for unused dependencies
machete:
    @echo "Running cargo machete..."
    cargo machete

# Run tests
test:
    @echo "Running tests..."
    cargo test --all {{ cargo_features }}

# Build release binary
build:
    @echo "Building release binary..."
    cargo build --release

# Run the daemon
run *args:
    @echo "Starting KGD daemon..."
    cargo run --bin kgd -- {{args}}

# Run the daemon in release mode
run-release *args:
    @echo "Starting KGD daemon (release mode)..."
    cargo run --bin kgd --release -- {{args}}

# Clean build artifacts
clean:
    @echo "Cleaning build artifacts..."
    cargo clean

# Check for outdated dependencies
outdated:
    @echo "Checking for outdated dependencies..."
    cargo outdated

# Update dependencies
update:
    @echo "Updating dependencies..."
    cargo update

# Full CI check (fmt check, check, clippy, deny, machete, test)
ci: fmt-check check clippy deny machete test
    @echo "✅ CI checks passed!"

# Check formatting without modifying files
fmt-check:
    @echo "Checking code formatting..."
    cargo fmt --all -- --check
