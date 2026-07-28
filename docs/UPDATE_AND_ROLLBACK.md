# Update and rollback

Hermes Local treats the official agent, launcher integration, llama.cpp,
model, Python lock, Node lock, browser binaries and optional tools as separate
components. Updates are never applied automatically.

## Check

```powershell
& 'D:\Hermes-Local\Update-Hermes-Local.ps1' -Mode Check -Component All -NonInteractive
```

The check records current, pinned and candidate revisions and links to source
comparisons where available. A newer upstream commit is information, not
permission to rebase the local integration.

Valid component names are `All`, `HermesAgent`, `Launcher`, `LlamaCpp`,
`Model`, `PythonLock`, `NodeLock`, `BrowserBinaries` and `OptionalTools`.

## Apply

Apply only a reviewed component:

```powershell
& 'D:\Hermes-Local\Update-Hermes-Local.ps1' -Mode Apply -Component Launcher -NonInteractive -Confirm:$false
```

The workflow:

1. validates the selected component and current state;
2. creates a backup/known-good snapshot;
3. fetches only from the recorded official source;
4. stages the candidate away from the active runtime;
5. verifies source/integrity metadata;
6. builds the candidate;
7. runs smoke tests;
8. switches the replaceable component;
9. retains the prior build for rollback;
10. preserves model, sessions, memory, skills and user files.

Hermes Agent patch conflicts are not auto-resolved. Rebase the six ordered
patches in `source\hermes-launcher\patches`, rerun source/UI/package/security
gates, then update `VERSION.json`.

## Rollback

```powershell
& 'D:\Hermes-Local\Update-Hermes-Local.ps1' -Mode Rollback -Component Launcher -NonInteractive -Confirm:$false
```

Rollback restores the last known-good replaceable artifact. It does not roll
back user data. If a schema migration changed data, use a verified backup
instead.

The launcher Apply/Rollback acceptance test built a staged launcher, passed
its quick test, then restored the exact previous launcher hash. The model,
runtime and user marker hashes remained unchanged.

## Backups

Create a named backup before manual changes:

```powershell
& 'D:\Hermes-Local\Backup-Hermes-Local.ps1' -Name before-upgrade -NonInteractive
```

Backups and `.sha256` sidecars are under `D:\Hermes-Local\backups`. A backup
stops a running stack for a consistent snapshot and restarts the prior
profile.

Restore only an archive from the managed backup directory:

```powershell
& 'D:\Hermes-Local\Restore-Hermes-Local.ps1' `
  -BackupPath 'D:\Hermes-Local\backups\Hermes-Local-<timestamp>-before-upgrade.zip' `
  -NonInteractive -Confirm:$false
```

Restore verifies the sidecar, rejects absolute/traversal archive entries,
stages extraction, creates a pre-restore safety backup, switches data and
restarts the previous profile.

The final restore drill deliberately modified a user marker, restored
`Hermes-Local-20260728T085047Z-final-acceptance.zip`, recovered the original
marker and restarted a healthy stack.

## Version records

- `VERSION.json` records pinned source/model/runtime identities.
- `CHANGELOG-LOCAL.md` records local product changes.
- `dist\package-manifest.json` records final package sizes and hashes.
- `security\reports\latest-scan.json` records the scanned integration commit.

After an update, these four records must agree before promotion.
