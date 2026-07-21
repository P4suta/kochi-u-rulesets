<!--
Thanks for contributing! A few reminders (see CONTRIBUTING.md for the full loop):
- Commits follow Conventional Commits (feat: / fix: / perf: / docs: / …); PRs are squash-merged.
- Site data (web/public/**, web/src/wasm/**) is generated — regenerate with `just build` / `just wasm`, don't hand-edit.
-->

## What & why

Describe the change and the motivation. Link any related issue (`Closes #123`).

## Linear

Closes DEV-___
<!-- Links this PR to its Linear issue; requires the Linear GitHub integration. -->

## Checklist

- [ ] `just lint` passes (fmt-check, clippy `-D warnings`, typos)
- [ ] `just test` passes
- [ ] Generated site data was regenerated with `just build` / `just wasm`, not hand-edited
