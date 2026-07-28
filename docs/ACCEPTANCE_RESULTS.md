# Acceptance results

Assessment date: 2026-07-28  
Release: Hermes Launcher 0.17.0 / local integration 0.17.0-local.1  
Result: **Accepted for local use with two explicit limitations**

The shipped default is loopback-only Daily 64K. No paid provider credential,
Docker or WSL was used.

## Installation and inference

| Gate | Result |
|---|---|
| Windows/hardware validation | Pass: Windows 11 Pro x64, i5-14600K, 64 GiB, RTX 3060 12 GiB |
| Official Hermes source | Pass: pinned upstream with preserved remote and integration branch |
| CUDA llama.cpp | Pass: build 10154; server, CLI and bench executables |
| Laguna model | Pass: exact 20,274,300,032 bytes and SHA-256 |
| Model load/generation | Pass: deterministic `LAGUNA_OK` probe |
| Native tool-call schema | Pass |
| Real Hermes terminal tool | Pass: 6 local model calls and terminal execution |
| Authentication | Pass: unauthenticated model inference denied |
| Binding | Pass: ports 8011 and 9119 listen only on `127.0.0.1` |

The final full operational test passed 9/9 checks at
`2026-07-28T09:43:55Z`. Evidence:
`D:\Hermes-Local\logs\diagnostics\latest-test.json`.

## Performance

| Measurement | Result |
|---|---:|
| Short chat mean | 53.297 tok/s |
| Sustained 1,000-token mean/minimum | 54.57 tok/s |
| 64K prompt / decode | 284.582 / 33.062 tok/s |
| 80K prompt / decode | 269.377 / 33.347 tok/s |
| 64K peak VRAM / RAM | 10,750 MiB / 18.10 GiB |
| 80K peak VRAM / RAM | 10,743 MiB / 18.13 GiB |
| Prompt-cache two-pass ratio | 287.31x |
| Native benchmark failures | 0 across 18 cases / 41 rows |
| Active paging thrash | None observed in selected Daily profile |

The run lasted 51.2 minutes and covered cold/warm, 2K/16K/32K/64K/80K,
long prefill, sustained decode, prompt cache, CPU-MoE, thread, batch, KV and
VRAM reserve sweeps. Speculation was omitted because no compatible verified
draft model was available.

Evidence:
`D:\Hermes-Local\benchmarks\results\latest.json` and
`D:\Hermes-Local\benchmarks\reports\LATEST.md`.

## Source and application tests

| Suite | Result |
|---|---|
| Windows-critical Hermes canonical runner | 463 passed, 0 failed, 2 skipped across 13 files |
| Hermes Local Electron controls | 5 passed |
| Theme/offline behavior | 67 passed across 8 files |
| Ruff | Pass |
| TypeScript | Pass |
| ESLint | 0 errors, 51 pre-existing warnings |
| Packaged CSP/navigation/TUI workstation | Pass |

The 13-file suite covers session export, browser hardening, ACP commands,
write approvals, malformed streaming tool-call repair, extraction, dropped
tool recovery, browser SSRF, cron, interrupt, one-child delegation, skill
commands and compression persistence.

The final upstream full Desktop Vitest command reported 3,332 passed,
21 failed and 3 skipped, with two additional suite import errors:

- 19 failures and both import errors are POSIX-only upstream test harness
  assumptions (POSIX permission bits, `/` sockets/paths, Bash parsing of
  Windows paths, symlink privilege, SSH ControlMaster and package-script
  loading). The corresponding Windows Local runtime/package paths are covered
  by focused tests and packaged E2E.
- 2 failures are upstream BillingSettings copy/poll timing assertions. Billing
  is a Nous/cloud surface and is disabled for the local Laguna deployment.

No test was skipped or weakened to hide these results. Exact output is
`D:\Hermes-Local\reports\acceptance\desktop-vitest-full-final.txt`.

## Packaging

| Gate | Result |
|---|---|
| Unpacked packaged app | Pass: Home, Services, Dashboard, Sessions, Projects, real TUI, narrow viewport, CSP |
| Portable self-extractor | Pass: real Home/Sessions/Projects/TUI and narrow viewport |
| NSIS installer | Pass: per-user install into path containing spaces |
| Start Menu shortcut | Pass |
| Desktop shortcut | Pass and removed by uninstall |
| Launch at login | Pass: current-user entry toggled and restored |
| Clean uninstall | Pass: app/shortcuts removed |
| Preserve user data | Pass: model, Python runtime and user marker hashes unchanged |
| Offline-equivalent restart | Pass behind a dead outbound proxy |
| Interrupted model download resume | Pass: verified 1 MiB prefix resumed to 5 MiB |
| Missing dependency repair | Pass: removed `defusedxml` file restored with exact hash |

Final package hashes:

| Artifact | Size | SHA-256 |
|---|---:|---|
| `Hermes Launcher.exe` / portable | 126,977,718 | `ae1fc84230068a72275fc35d6059dc7142d0839534eb3173415409bbc8b1ac4d` |
| NSIS setup | 127,239,866 | `bea5e16a5190fb019d83778d367f97f16dd52e7640bf1001e7cdf15d2a3db2ef` |
| Setup blockmap | 125,888 | `287bee4d8809fa74fd5cbdd437fdd2e2b278f82fe0a2c4aa7b9ac2ac9ef3e530` |

## Recovery and maintenance

- Supervisor crash recovery passed after forcibly terminating the verified
  llama-server PID; the model and Hermes recovered with restart count 1.
- Backup SHA validation passed.
- Restore recovered a deliberately changed user marker and restarted the
  stack; a pre-restore safety backup was created.
- Launcher update Apply built/staged/smoke-tested a candidate; Rollback
  restored the exact previous launcher hash and preserved model/runtime/user
  hashes.
- Repair stops and restarts the stack, backs up, force-reinstalls dependencies
  and preserves the exact integration tree.
- Patch reconstruction from the official base produced exact integration tree
  `67e4ce9137866dbb7febc3cc8b4072ffda816542`.

## Security

Final status: `pass-with-triaged-residuals`.

- 4 validated security findings fixed with regression tests;
- 0 installed Python vulnerabilities;
- 0 production Gitleaks findings;
- 0 Semgrep secret-rule candidates;
- Defender distribution scan clean;
- Node/Python CycloneDX SBOMs generated;
- Electron CSP, sandbox, context isolation, web security, exact navigation,
  permission and preload controls verified;
- unauthenticated inference denied and both listeners loopback-only.

Three dependency residuals—React Router RSC-only, build-only brace-expansion
and optional/uninstalled PyNaCl—are accepted with re-review triggers.

Evidence:
`D:\Hermes-Local\security\reports\latest-scan.json` and
`D:\Hermes-Local\security\reports\SECURITY_REPORT.md`.

## Limitations

1. A physical Windows reboot was not performed because it would terminate the
   active Codex build/orchestration task. Cold stack restarts,
   offline-equivalent restart, current-user login-item toggle/restore and
   installer cleanup were tested. Confirm one real reboot before relying on
   unattended login startup.
2. The locally built executables are not Authenticode-signed and may show a
   Windows reputation warning.

Maximum Context at 128K is experimental, external providers/integrations
remain disabled, and there is no supported LAN mode.
