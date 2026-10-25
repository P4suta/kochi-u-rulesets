# Project guide

Kochi University Regulations mirrors the university's regulation index, parses its PDF corpus, and prepares structured data for a browser viewer.

The Rust workspace separates the native synchronization and build commands in `crates/cli`, the parsing and search implementation in `crates/core`, and the browser WebAssembly interface in `crates/wasm`.

The Svelte frontend presents the generated regulation catalogue with text search, amendment timelines, and reference graphs.

The official source is the [Kochi University regulations page](https://www.kochi-u.ac.jp/education-support/regulations/).

## Install and build from source

The CLI package defines the `kochi-university-regulations` binary, and the repository pins Rust 1.99.1 in `rust-toolchain.toml`.

The root `Cargo.toml` pins the shared dependency graph through `Cargo.lock`.

Run the generated README's `cargo install --path crates/cli` command from a source checkout to install the CLI binary.

Run `kochi-university-regulations build --strict` from the repository root to parse the checked-in PDFs and regenerate the browser data.

The `build` command defaults to `sources/`, `manifest.json`, and `web/public/`.

It writes one JSON document and one Markdown document under `web/public/docs/` for each mapped PDF.

It also writes `search.idx`, `graph.json`, and `site.json` under `web/public/`.

The `--strict` flag returns a failure when any input fails to parse or match the manifest.

The build command writes generated files, so inspect its output before committing changes.

## Synchronize source documents

Run `just sync` to fetch the university index, download listed PDFs, and update `manifest.json` by each regulation's numeric code.

The synchronization command aborts before modifying files when it extracts fewer than 20 index entries.

The command removes prior files for regulations missing from the university index.
If a download request fails, the command skips the entry without removing its old path.
For a renamed regulation, the command downloads the new file first.
It removes the old path only after the new download succeeds.

Review all PDF and manifest changes before accepting an automated or manual synchronization.

The scheduled synchronization workflow runs weekly and proposes source changes in a signed draft pull request rather than writing to `main`.

Its `dry-run` input fetches source data without creating or updating a pull request.

## Run the browser app

Run `just wasm` after generating the data to build the search WebAssembly module under `web/src/wasm/`.

The contributor guide documents the separate `wasm-pack` setup required by that recipe.

Install the locked frontend dependencies and start Vite with:

```sh
cd web
pnpm install --frozen-lockfile
pnpm run dev
```

The frontend reads the CLI-generated JSON files and search index from `web/public/`.

The WebAssembly worker loads the search index off the main thread and handles full-text queries.

## Validate and deploy

Run `just check` for locked dependency metadata, formatting, Clippy, spelling, and Rust tests.

The required pull-request workflow runs locked metadata validation, formatting, strict Clippy, and native tests on `pull_request` and `merge_group` events.

Run `pnpm run check` in `web/` for the Svelte and TypeScript checks defined by the frontend package.

Read [contribution guidance](../CONTRIBUTING.md) before changing source data or project workflows.

The Pages deployment workflow runs only through manual dispatch and requires the repository's GitHub Pages configuration.
