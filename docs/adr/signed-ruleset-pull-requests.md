---
date: 2026-10-20
---

# Signed ruleset pull requests

## Status

Proposed.

## Context

The default branch requires signed commits and pull requests.
Source synchronization must preserve those protections and offer a reviewable data diff.
Unconfigured Pages deployment must not follow a source sync.

## Decision

Use the pinned create-pull-request action with the repository's temporary GitHub token.
Reuse one draft proposal branch and include only the manifest and source PDFs.
Enable the repository setting for workflow pull request creation while keeping default token permissions read-only.
GitHub exposes that setting together with permission to approve reviews; this workflow never approves or merges a pull request.
Verify GitHub's signing flag before a separate job dispatches the existing required workflow.
Keep Actions write permission in that dispatch job and keep site publication manual.
Remove the custom direct-commit CLI adapter.

## Consequences

The workflow needs no personal token, signing secret, or separate GitHub App.
It trusts GitHub's token isolation and signature verification, the pinned action implementation, and hosted workflow execution.
Unchanged sources create no proposal.
Updated source content may require reviewed snapshot changes before the native tests pass.
The action cannot sign an individual file larger than GitHub's 40 MiB blob limit.
Direct writes to main and automatic deployment would bypass the intended review boundary.
Maintaining a second commit API client would duplicate the signing and proposal lifecycle.
