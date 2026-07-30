# Hermes Local changelog

## Unreleased

- Managed enabled Hermes messaging gateways with the local stack, added authoritative gateway readiness and ownership state, and made diagnostics fail when a required gateway is offline.
- Removed the redirect-only Tools page from primary navigation, preserved `/tools` as a redirect to Skills, and retained direct Chat and TUI access.
- Replaced the remaining native launcher selects with the shared themed,
  accessible dropdown across model, runtime, network, log, profile and quick
  entry controls.
- Added a transactional Hermes Agent updater that stages upstream source away
  from the active installation and applies the ordered Hermes Local patch series
  with three-way merge support.
- Added automatic source, Python environment, launcher and source-pin rollback
  when dependency rebuilds or runtime health checks fail.
- Added a machine-local source pin override consumed by setup, repair,
  diagnostics and future update checks without modifying tracked defaults.
- Added a double-click Windows updater and documented why the upstream in-chat
  `/update` command must not be used for a patched Hermes Local checkout.

## 0.18.1 — 2026-07-28

- Established the first accepted Hermes Local reference build for Windows 11.
- Added a native launcher, persistent supervisor and local model runtime.
- Added project-managed Python, Node.js and browser dependencies.
- Added bootstrap, diagnostics, repair, backup and restore workflows.
- Added local authentication and loopback-only service defaults.
- Added deterministic source patching and build metadata.
