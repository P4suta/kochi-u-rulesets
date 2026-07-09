set shell := ["bash", "-cu"]

# List recipes.
default:
    @just --list --unsorted

# === Build / run ===

# 高知大学学則.pdf → data.json
run:
    cargo run --release -q -- 高知大学学則.pdf -o data.json

# Dump raw per-page extracted text (debugging / regenerating test fixtures).
dump-raw DIR="raw-dump":
    cargo run -q -- 高知大学学則.pdf --dump-raw {{DIR}}

# === Test ===

test:
    cargo test

# === Format / lint ===

fmt:
    cargo fmt
    -typos --write-changes 2>/dev/null || echo "(typos: cargo install typos-cli, or just install-tools)"

lint:
    cargo clippy --all-targets -- -D warnings
    cargo fmt --check
    typos

# CI-equivalent checks (matches lefthook pre-push).
check: lint test

# === Setup ===

install-tools:
    @if command -v mise > /dev/null; then \
      mise install; \
    else \
      echo "→ mise not found: install from https://mise.jdx.dev/ and retry"; \
      echo "  (fallback) cargo install typos-cli lefthook"; \
      exit 1; \
    fi

install-hooks:
    lefthook install
