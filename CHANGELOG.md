# Changelog

All notable changes to this project are documented in this file.

The format is inspired by Keep a Changelog and Semantic Versioning principles.

## [Unreleased]

### Added

- Node groups: cluster nodes into labelled groups with translucent hulls and collapse-to-chip
- Quick grouping: select nodes and click "Group", or right-click a node → "Add to group" — no need to open the editor
- Appearance themes: six accent themes and four board backgrounds (dots, grid, aurora, void), persisted
- Command palette upgrade: fuzzy search across nodes, runnable macros, recents, and quick commands
- Macro variables: `{{clipboard}}`, `{{date}}`, `{{time}}`, `{{datetime}}` tokens plus a `prompt-input` step feeding `{{input}}`
- Drag-and-drop node creation: drop files, folders, or links onto the board to create nodes with the right target
- Workspace import/export to portable `.finnode.json` files (`export_workspace` command)
- Usage insights: per-node launch counts, most-used ranking, and live workspace stats
- Web-only preview bridge so the Svelte UI runs (with seeded demo data) in a plain browser
- Startup macro setting (off by default): run main-node macro on system start
- Keyboard shortcut macro action with recording and multi-switch sequence support
- Middle-mouse board panning for zoomed-out navigation
- Extended how-to user guide and expanded README documentation

### Changed

- `save_layout` now preserves authoritative app settings, so routine layout saves no longer reset theme/start-on-boot
- Launching a node records its `last_launched` and increments `launch_count`, persisted immediately
- Improved open-application macro behavior across Linux, macOS, and Windows
- Improved focus handoff for typing and keyboard shortcut macro actions
- Improved node boundary behavior when zoomed out

### Fixed

- App settings were silently reset to defaults on every frontend layout save
- Desktop board size/position persistence across full app exit and system restart
- Startup geometry overwrite race during early window events

## [0.2.0]

### Added

- Transparent desktop node board and settings panel workflow
- Workspace and node graph persistence
- System tray controls and global shortcuts
