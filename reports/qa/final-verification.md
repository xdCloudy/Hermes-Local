# Final verification

Result: **PASS with explicit functional coverage limitations**.

Security auditing and vulnerability assessment were excluded from this QA engagement.

## Source identity

- Root branch: `codex/full-qa-refactor`
- Root base: `b47e87e710c4f7509da5db54ad25b435ecc679e5`
- Root implementation commits: `5974b3d`, `889bcd1`
- Pinned upstream: `3be565fbdee3115ab5b9338551768b8e5e655c56`
- Nested integration: `0c1dbce2930bdb8b9c09ac768e71f20a3b87f814`
- Expected integration tree:
  `5df883962b78c8b29b98bcbfa1ebc5c939a3f6f4`
- Clean reconstruction from upstream plus patches 0001–0012 produced the
  expected tree.

## Release gates

| Gate | Final result |
|---|---|
| Inventory generation/final disposition | Pass |
| PowerShell parser | 24 files, 0 failures; one security script excluded |
| TypeScript | Pass |
| ESLint | 0 errors, 51 existing warnings |
| Focused Electron regressions | Pass |
| Full Electron Vitest | 74 files passed, one skipped; 835 tests passed, three skipped |
| Full UI Vitest | 299 files passed; 2,555 tests passed, one skipped |
| Windows-critical Python selection | 136 passed, two skipped |
| Recovery fixtures | 7/7 passed |
| Desktop build | Pass |
| Operational diagnostics | Pass |
| Unpacked packaged E2E | Pass |
| Portable executable direct smoke | Pass |
| Installed NSIS E2E in a path containing spaces | Pass |
| Installer/uninstaller and shortcut cleanup | Pass |
| Launch-at-login toggle and restoration | Pass |
| Service start/stop/restart | Pass |
| Model process automatic recovery | Pass |
| Update check | Pass |
| Patch reconstruction/tree equality | Pass |
| Protected state verification | Pass |

The final full orchestrator completed 12/12 required steps in 170,179 ms.
The finalized inventories were generated afterward and the orchestrator now
runs that finalizer as a required thirteenth step on future full runs. The
updated orchestrator then passed its post-documentation `Fast` scope 8/8 in
51,640 ms, including the finalizer.

The full run recorded nested commit `ba953455…`. The final nested commit
`0c1dbce…` adds only login-item restoration coverage; that exact commit passed
the final packaged E2E and the post-documentation Fast gate.

## Final package

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| `Hermes Launcher.exe` | 126,983,589 | `eb6227c867d28de3a062be095ddfbdfcaadcca7fd89b48c6d2d9754769ae3ab5` |
| `Hermes-Launcher-0.18.1-windows-x64-setup.exe` | 127,245,805 | `f8d24f2b1d32b0f819808a5c6aa64d5c8a56702606f8701c483f6becebbf8ba6` |
| setup blockmap | 125,820 | `05c443ba92e2becc9123938c37161e52961ca7b094b332da733e69cb4231911a` |

The package stamp records integration commit `0c1dbce…`, branch
`hermes-local-integration`, and `dirty: false`.

## Protected installation

- Named safety backup:
  `Hermes-Local-20260729T163813Z-before-full-functional-qa-20260729.zip`
  (14,402,189 bytes,
  `747f156e643cc872f7ec70864b62fe3a5c9562ffdffc6450814c6e9fbed428ef`).
- User settings remained byte-identical:
  `7129eff2e9b9df1c965a654da12044e6854b9ab09b023c26f5a4fee4f6a70fd8`.
- The pre-existing nested `package-lock.json` change remained byte-identical:
  `d63bf263d81d632a1c113f156e402f8387c632f270fd045de576f7609f16b82c`.
- Three QA-created CLI sessions were removed through the supported command;
  semantic session/message/model-usage counts returned to baseline.
- Browser-created achievements state was removed after verifying it was absent
  from the safety backup and contained only the three QA-created files.
- The protected model manifest and its user backup remain present, unmodified,
  and uncommitted.
- The managed stack is healthy on the original Daily profile and ports
  8011/9119.

## Explicit limitations

- 48 controls and nine UI states are source-inventoried but were not
  individually actuated.
- 192 executable entries are inventoried but were not run independently; 14
  additional PowerShell entries have parser-only evidence.
- Eight workflows have partial evidence, principally where mutating the active
  model/settings/benchmark/repair state would conflict with data protection.
- No physical reboot, Windows logoff/shutdown, real insufficient-disk event,
  interrupted live update, or full Windows scaling matrix was exercised.
- The user stopped the last direct portable re-launch. The earlier direct
  portable smoke and the final unpacked and installed tests passed; no native
  UI takeover was attempted afterward.
- The NSIS uninstaller left an empty installation directory. It contained no
  user data and was removed after exact-path verification.

These limitations are not hidden by the release-gate result; per-entry details
are in the three machine-readable inventories.
