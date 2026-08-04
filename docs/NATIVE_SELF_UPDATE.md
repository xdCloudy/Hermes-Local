# Native Hermes Local self-update

Hermes Local uses the existing Desktop update surface for application updates. The update is prepared while the current launcher remains open, then activated only after the user closes the launcher.

## Authoritative flow

Both the Desktop surface and recovery CLI use the same root scripts:

1. Desktop creates a durable, globally exclusive `update` task.
2. `Invoke-Hermes-DesktopUpdate.ps1` validates the configured GitHub origin, channel target, fast-forward relationship and free disk space.
3. A plan, rollback snapshot and pending launcher directory are created beneath `build/updates/desktop-staging`.
4. Tracked, staged and non-ignored untracked source changes are placed in an operation-labelled Git stash. Ignored machine-local configuration remains in place and is never included in the stash.
5. The trusted root checkout is moved to the exact target commit and the pinned Hermes Agent integration is synchronised without downloading or replacing model weights.
6. `Build-Hermes-Launcher.ps1 -DestinationDirectory <pending-dist>` builds and validates the new launcher beside the active `dist` directory. The running launcher is not replaced or closed.
7. The updater reapplies the preserved working tree, removes its temporary stash after a clean restoration, and reports `ready-to-restart`.
8. A detached promotion helper waits for all processes using `dist\Hermes Launcher.exe` to exit.
9. When the user closes Hermes Launcher, the helper atomically promotes the pending launcher into `dist`. It does not relaunch the application.
10. The next user-initiated launch runs the updated version.

The updater never runs `git clean`. Models, profiles, chats, memories, configuration, reports and other ignored user data are not removed. Local source edits do not block an update: they are preserved automatically and reapplied after the staged build. If upstream changes conflict with those edits, the launcher update remains staged and the original changes remain available in the recorded Git stash.

## User experience

During preparation, Hermes Launcher stays usable and Task Centre reports build progress. Completion is shown as **Update ready — restart when convenient** rather than claiming that the launcher is restarting.

Closing the window is the activation boundary. The helper waits for the launcher and its Electron child processes to release the installed files, swaps the staged build into place, and exits. The user starts Hermes Launcher again normally; there is no automatic relaunch.

If the promotion helper is interrupted, `data\runtime\pending-desktop-update.json` records the pending update. A later update check restarts the helper without rebuilding the launcher.

## Failure and rollback

If source synchronisation or the staged build fails, the active `dist` directory is untouched. The updater resets the root checkout to the previous commit, re-synchronises the previous integration and reapplies the preserved working tree. If the working tree cannot be reapplied cleanly, the stash is retained and its commit is recorded in the update result.

If activation fails after the user closes the launcher, the previous `dist` directory is restored where possible and the pending-update evidence is retained for recovery. Progress, stash metadata and failure evidence remain under `build/updates/desktop-staging` and the normal update-operation reports.

A stale promotion-helper lock is recovered only when its recorded owner process no longer exists. Concurrent update operations remain blocked by the Task Centre resource policy and updater locks.

## Channels

The updater supports `development`, `stable`, `beta` and explicit `pinned` commit targets. Desktop currently defaults to the trusted `main` development channel; channel input is validated before it reaches Git or PowerShell native arguments.

## Recovery

Check application-update state from PowerShell:

```powershell
& '.\Invoke-Hermes-DesktopUpdate.ps1' `
    -Mode Check `
    -Channel development `
    -NonInteractive
```

For application update and activation evidence, inspect:

```text
build\updates\desktop-staging\<operation-id>\
data\runtime\pending-desktop-update.json
build\updates\operations\
logs\launcher\launcher.log
```
