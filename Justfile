# KGD Project Justfile

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

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
    cargo check --all-targets

# Run clippy linter
clippy:
    @echo "Running cargo clippy..."
    cargo clippy --all-targets -- -D warnings

# Run tests
test:
    @echo "Running tests..."
    cargo test --all

# Build release binary
build:
    @echo "Building release binary..."
    cargo build --release

# Run the daemon
run:
    @echo "Starting KGD daemon..."
    cargo run --bin kgd

# Run the daemon in release mode
run-release:
    @echo "Starting KGD daemon (release mode)..."
    cargo run --bin kgd --release

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

# Full CI check (fmt check, check, clippy, test)
ci: fmt-check check clippy test
    @echo "✅ CI checks passed!"

# Check formatting without modifying files
fmt-check:
    @echo "Checking code formatting..."
    cargo fmt --all -- --check
