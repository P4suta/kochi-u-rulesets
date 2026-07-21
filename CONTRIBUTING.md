# Contributing

Contributions are welcome. This project parses Kochi University's 規則集
(regulations) PDFs into structured data and serves them through a browser app:
a Rust workspace (`crates/core` parsing/rendering, `crates/cli` build-time data
generation, `crates/wasm` the in-browser parser + search engine) plus a Svelte 5
+ Vite frontend in `web/`.

## Setup

Tools are pinned via [mise](https://mise.jdx.dev/) (`.mise.toml`); tasks run
through [just](https://github.com/casey/just). The Rust toolchain itself is
owned by `rust-toolchain.toml`; `wasm-pack` is installed separately
(`cargo install wasm-pack`).

```
just install-tools   # mise install — provisions the pinned tools
just install-hooks   # lefthook install — commit-msg + pre-commit/pre-push hooks
```

Declare tools in `.mise.toml` and install via `mise install`; do not add them ad
hoc.

## Dev loop

```
just build    # native CLI: sources/*.pdf → web/public data + search/graph
just wasm     # build the browser wasm module into web/src/wasm
just serve    # run the Vite dev server for web/
just lint     # cargo fmt --check + clippy -D warnings + typos
just test     # cargo test
just check    # lint + test (matches the lefthook pre-push hook)
```

## Commit / PR rules

- [Conventional Commits](https://www.conventionalcommits.org/) (`feat:` / `fix:` /
  `perf:` / `docs:` / `refactor:` / `test:` / `chore:` / `ci:` / `build:`).
  Enforced locally by [committed](https://github.com/crate-ci/committed) in the
  lefthook `commit-msg` hook (see `committed.toml`).
- **Squash-merge only.**
- Releases are cut by [release-please](https://github.com/googleapis/release-please):
  it opens a release PR that bumps the version + CHANGELOG from conventional
  commits, then tags on merge. (The integration is dormant until its GitHub App
  credentials are configured.)

## Before pushing

- `just check` green (the `lefthook` pre-push hook runs lint + test).
- Site data under `web/public/**` and `web/src/wasm/**` is generated — regenerate
  it via `just build` / `just wasm`, do not hand-edit.
- Do not bypass hooks with `--no-verify`. If a hook fails, fix the cause.
