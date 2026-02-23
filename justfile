# Kairex — common commands

# Build all crates
build:
    cargo build

# Build release
release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run clippy and fmt check
lint:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings

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

# Deploy (placeholder)
deploy:
    @echo "Run: ansible-playbook ansible/playbooks/deploy.yml"
