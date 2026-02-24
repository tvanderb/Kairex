# Kairex — common commands

# Build all crates
build:
    cargo build

# Build release
release:
    cargo build --release

# Run all tests (Rust + Python)
test:
    cargo test
    LD_LIBRARY_PATH=/usr/lib .venv/bin/python3 -m pytest scripts/tests/ -v

# Run clippy and fmt check
lint:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings

# Run Python tests only
pytest:
    LD_LIBRARY_PATH=/usr/lib .venv/bin/python3 -m pytest scripts/tests/ -v

# Format code
fmt:
    cargo fmt --all

# Run the binary in dev mode
dev:
    RUST_LOG=debug cargo run -p kairex-bin

# Build Docker image
docker-build:
    docker build -t kairex .

# Start full stack (app + observability)
up:
    docker compose up -d

# Stop full stack
down:
    docker compose down

# Start with dev overrides
dev-up:
    docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d

# Deploy to VPS via Ansible
deploy:
    cd ansible && ansible-playbook playbooks/deploy.yml

# Bring up WireGuard VPN
vpn-up:
    sudo wg-quick up wg0

# Bring down WireGuard VPN
vpn-down:
    sudo wg-quick down wg0

# Show WireGuard VPN status
vpn-status:
    sudo wg show

# Configure git hooks
setup-hooks:
    git config core.hooksPath .githooks
