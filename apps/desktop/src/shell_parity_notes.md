# Desktop Shell parity controls

This file documents the automated Desktop-owned Shell slice used by the Dioxus migration matrix.

- `Ctrl/Cmd+Alt+L` opens the layout/language panel.
- Pane layout state is persisted at `hermes.desktop.layoutTree.v3`.
- Locale state is persisted at `hermes.desktop.locale` and supports `en`, `zh`, `zh-hant`, `ja`, and `ar`.
- The focus collision guard preserves editor/composer/terminal-owned shortcuts before the global Shell router sees them.
- The single-instance lease prevents two Desktop authorities from starting against the same local data/runtime directory.
- The layout engine owns groups, tabs, horizontal/vertical splits, active focus, floating panes, docking, ordering, close/collapse, bounds and JSON restoration.
- Automated accessibility checks cover missing interactive names, tab selection state and duplicate IDs; shell CSS observes reduced-motion and focus-visible preferences.

Human acceptance remains authoritative for visual parity and final A6 validation.
