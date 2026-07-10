set shell := ["bash", "-cu"]

# List recipes.
default:
    @just --list --unsorted

# === Build / run ===

# sources/高知大学学則.pdf → out/高知大学学則.{json,md}
run:
    cargo run --release -q -- --out-dir out sources/高知大学学則.pdf

# Every ruleset PDF in sources/ → out/<name>.{json,md}
run-all:
    cargo run --release -q -- --out-dir out sources

# Dump raw per-page extracted text (debugging / regenerating test fixtures).
dump-raw PDF="sources/高知大学学則.pdf" DIR="raw-dump":
    cargo run -q -- {{PDF}} --dump-raw {{DIR}}

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
