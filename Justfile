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
    cargo test --all

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

# Start local development environment with Docker Compose
compose-local *args:
    docker compose -f compose.yml -f compose.local.yml up --build {{args}}

# Stop local development environment
compose-local-down *args:
    docker compose -f compose.yml -f compose.local.yml down {{args}}

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
