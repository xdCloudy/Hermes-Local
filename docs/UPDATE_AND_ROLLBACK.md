# Update and rollback

[← Documentation home](README.md) · [Project home](../README.md) ·
[Operations](OPERATIONS.md)

Hermes Local treats the official agent, launcher integration, llama.cpp,
model, Python lock, Node lock, browser binaries and optional tools as separate
components. Updates are never applied without an explicit command.

## Check all components

```powershell
Set-Location 'C:\path\to\Hermes-Local'
$HermesRoot = (Get-Location).Path
& (Join-Path $HermesRoot 'Update-Hermes-Local.ps1') -Mode Check -Component All -NonInteractive
```

The inventory records current, pinned and candidate revisions and links to
source comparisons where available. Valid component names are `All`,
`HermesAgent`, `Launcher`, `LlamaCpp`, `Model`, `PythonLock`, `NodeLock`,
`BrowserBinaries` and `OptionalTools`.

## Update Hermes Local Desktop

The Desktop update channel advances the Hermes Local repository and prepares a
new launcher without building from, stashing, or hard-resetting the installed
working tree:

```powershell
& '.\Invoke-Hermes-DesktopUpdate.ps1' -Mode Check -Channel development
& '.\Invoke-Hermes-DesktopUpdate.ps1' -Mode Apply -Channel development -NonInteractive
```

The updater fetches the immutable target commit, creates a detached Git
worktree under the operation staging directory, and runs setup plus the launcher
build there. Only after that candidate succeeds does Git fast-forward the
installed checkout. Git leaves unrelated staged, unstaged and untracked files
in place. If a target path overlaps a local edit or untracked file, promotion
stops without changing `HEAD` or that file and reports the conflicting path.

The active launcher remains untouched until the validated payload is ready.
Activation waits for the launcher process tree to exit, retains the prior
distribution until the new executable is in place, and restores it if activation
fails. A locally modified `source/hermes-agent` checkout is not moved; its
synchronisation is deferred until those edits are reconciled.

This path intentionally does not use `git stash`, `git clean`, or
`git reset --hard`. Failed candidate work is replaceable staging state; user
data, local source edits, model files, configuration and workspaces remain in
their installed locations.

## Update Hermes Agent

Do not run Hermes Agent's in-chat `/update` command inside a Hermes Local TUI or
Desktop session. The upstream command updates its own checkout in place and does
not understand Hermes Local's ordered integration patches, source pin or
separate launcher build.

Use the external transactional updater instead:

```powershell
Set-Location 'C:\path\to\Hermes-Local'
& '.\Update-Hermes-Agent.ps1' -Mode Check
& '.\Update-Hermes-Agent.ps1' -Mode Apply
```

Windows users may double-click `Update-Hermes-Agent.cmd` for the same apply
workflow. For unattended execution after reviewing the candidate:

```powershell
& '.\Update-Hermes-Agent.ps1' -Mode Apply -NonInteractive -Confirm:$false
```

A specific reviewed upstream commit or branch can be selected:

```powershell
& '.\Update-Hermes-Agent.ps1' -Mode Apply `
  -TargetCommit '<40-character-commit>'

& '.\Update-Hermes-Agent.ps1' -Mode Apply `
  -TargetBranch 'release-candidate'
```

### Transactional workflow

The updater:

1. requires PowerShell 7 and a clean, recorded Hermes Agent harness tree;
2. resolves the selected upstream branch or commit;
3. clones the candidate under `build\updates\staging` without touching the
   active checkout;
4. applies every patch in `source\hermes-launcher\patches` with Git's
   three-way merge support;
5. leaves the active installation untouched if a patch cannot be applied;
6. stops the supervised stack and creates a normal user-data backup;
7. moves the current source checkout and Python environment into a timestamped
   known-good directory;
8. records the candidate base commit, harness commit and harness tree
   in the ignored `config\launcher\source-overrides.json` file;
9. recreates dependencies through `Setup-Hermes-Local.ps1`;
10. rebuilds Hermes Launcher, starts the selected profile and runs the quick
    health suite;
11. restores the prior source, environment, launcher and source pin
    automatically if promotion or validation fails.

Models, sessions, memory, skills, configuration and workspace files are not
replaced. Failed candidate trees are retained under `build\updates\failed` for
inspection.

### Patch conflicts

Patch conflicts are deliberately not guessed or silently discarded. If the
ordered Hermes Local patch series cannot be applied to the new upstream commit,
the updater exits before stopping the active installation. A maintainer must
rebase the affected patch, validate the resulting integration and publish the
updated patch series.

Rerun the applicable source, UI, functional QA, and package gates before
publishing that series. Security work is a separately authorised scope and was
excluded from the 2026-07-29 functional QA engagement.

## Roll back Hermes Agent

The most recent successful update retains its prior source checkout, Python
environment and launcher build:

```powershell
& '.\Update-Hermes-Agent.ps1' -Mode Rollback
```

Rollback creates a fresh user-data backup, quarantines the displaced current
installation, restores the recorded known-good components and reruns the health
suite. The source pin override is restored to its previous content or removed
when the installation originally used only `VERSION.json`.

## Update or roll back the launcher only

The general updater still handles launcher-only rebuild and rollback:

```powershell
& (Join-Path $HermesRoot 'Update-Hermes-Local.ps1') `
  -Mode Apply -Component Launcher -NonInteractive -Confirm:$false

& (Join-Path $HermesRoot 'Update-Hermes-Local.ps1') `
  -Mode Rollback -Component Launcher -NonInteractive -Confirm:$false
```

The launcher workflow builds the candidate, runs the quick test and retains the
prior packaged launcher for rollback. Other source components remain review-only
in the general updater.

## Backups

Create a named backup before manual changes:

```powershell
& (Join-Path $HermesRoot 'Backup-Hermes-Local.ps1') -Name before-upgrade -NonInteractive
```

Backups and `.sha256` sidecars are under `$HermesRoot\backups`. A backup stops
a running stack for a consistent snapshot and restarts the prior profile.

Restore only an archive from the managed backup directory:

```powershell
$backup = Join-Path $HermesRoot 'backups\Hermes-Local-<timestamp>-before-upgrade.zip'
& (Join-Path $HermesRoot 'Restore-Hermes-Local.ps1') `
  -BackupPath $backup `
  -NonInteractive -Confirm:$false
```

Restore verifies the sidecar, rejects absolute or traversal archive entries,
stages extraction, creates a pre-restore safety backup, switches data and
restarts the previous profile.

## Version and update records

- `VERSION.json` records the repository's tested default source, model and
  runtime identities.
- `config\launcher\source-overrides.json` records a machine-local promoted
  Hermes Agent base commit and resulting integration identities.
- `build\updates\history\*-hermes-agent.json` records successful promotions and
  their known-good rollback paths.
- `CHANGELOG-LOCAL.md` records local product changes.
- `dist\package-manifest.json` records final package sizes and hashes.
- `security\reports\latest-scan.json` records the scanned harness commit.

`Get-HermesVersionManifest` merges the ignored source override over the tracked
default manifest. Setup, repair, diagnostics and future update checks therefore
use the promoted source pin without modifying tracked repository files.

For the 2026-07-29 functional QA, Check mode completed successfully and the
complete 0001–0012 patch series reconstructed
`5df883962b78c8b29b98bcbfa1ebc5c939a3f6f4` from pinned upstream
`3be565fbdee3115ab5b9338551768b8e5e655c56`. Failure-path update and rollback
coverage used existing controlled tests and historical fixture evidence; no
interrupted update was induced in the active protected installation.
