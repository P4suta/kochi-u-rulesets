set shell := ["bash", "-cu"]

# List recipes.
default:
    @just --list --unsorted

# === Sync / build ===

# Mirror the live 規則集 index into sources/ and update manifest.json.
sync:
    cargo run -p kochi-university-regulations --release -q -- sync

# Every ruleset PDF in sources/ → web/public/{docs/<code>.json, ...} + search/graph/site.
build:
    cargo run -p kochi-university-regulations --release -q -- build sources

# Full local refresh: pull the latest PDFs, then regenerate site data.
update: sync build

# Dump raw per-page extracted text (debugging / regenerating test fixtures).
dump-raw PDF="sources/高知大学学則.pdf" DIR="raw-dump":
    cargo run -p kochi-university-regulations -q -- build {{PDF}} --dump-raw {{DIR}}

# === Web / WASM ===

# Build the browser wasm module (live parser + search engine) into web/src/wasm.
wasm:
    wasm-pack build crates/wasm --target web --out-dir ../../web/src/wasm

# Serve the built site locally (requires a JS toolchain in web/).
serve:
    cd web && (bun run dev || npm run dev)

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
