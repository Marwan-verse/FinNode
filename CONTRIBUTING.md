# Contributing to FinNode

Thanks for taking the time to contribute.

This document explains how to run the project locally, how to submit changes,
and the quality bar expected for pull requests.

## Development Setup

1. Fork and clone the repository.
2. Install dependencies:
   - `npm install`
3. Start desktop development mode:
   - `npm run dev:desktop`

Optional:

- Web-only dev mode: `npm run dev`
- Frontend production build check: `npm run build:web`

## Build Commands

- Desktop build: `npm run build:exe`
- Linux-to-Windows build helper: `npm run make:windows`

## Code Areas

- Frontend UI and behavior:
  - `src/App.svelte`
  - `src/style.css`
- Tauri backend and desktop behavior:
  - `src-tauri/src/main.rs`
- Build scripts:
  - `scripts/make-executable.sh`
  - `scripts/make-windows-executable.sh`

## Pull Request Guidelines

Before opening a PR:

1. Keep changes focused and scoped.
2. Run at least:
   - `npm run build:web`
3. Update docs when behavior changes.
4. Include clear test steps in the PR description.

PR quality checklist:

- [ ] Change is limited to intended scope
- [ ] No unrelated refactors included
- [ ] README/how-to docs updated if needed
- [ ] Manual validation steps provided
- [ ] Build checks pass

## Commit Message Suggestions

Use concise, action-oriented messages, for example:

- `feat: add startup macro toggle`
- `fix: preserve desktop board bounds when zoomed out`
- `docs: expand how-to usage guide`

## Reporting Issues

Use GitHub issue templates for:

- Bug reports
- Feature requests

For security-sensitive issues, do not open a public issue. See
`SECURITY.md`.

## Code of Conduct

By contributing, you agree to follow `CODE_OF_CONDUCT.md`.
