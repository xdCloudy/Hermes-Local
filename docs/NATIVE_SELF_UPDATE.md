# Native Hermes Local self-update

Hermes Local uses the existing Desktop update overlay for application updates. The bottom-right client/version indicator now performs a real check rather than returning the previous `Update not available` dead end.

## Authoritative flow

Both the Desktop surface and recovery CLI use the same root scripts:

1. Desktop creates a durable, globally exclusive `update` task.
2. `Invoke-Hermes-DesktopUpdate.ps1` validates the configured GitHub origin, channel target, fast-forward relationship, tracked working tree and free disk space.
3. A plan and rollback snapshot are staged beneath `build/updates/desktop-staging`.
4. A copy of the helper is launched from the Windows temporary directory, outside `dist`.
5. Electron closes. The helper waits for the launcher PID to exit before replacing files.
6. The trusted root checkout is moved to the exact target commit.
7. Setup synchronises the recorded Hermes Agent integration without downloading or replacing model weights.
8. `Update-Hermes-Local.ps1 -Component Launcher` performs the existing transactional backup, build, validation and rollback stages.
9. The verified launcher is restarted.

The helper never runs `git clean` and refuses to start when tracked or staged changes exist. Models, profiles, chats, memories, configuration, reports and other untracked user data are not removed.

## Failure and rollback

If source synchronisation, build, replacement or validation fails, the helper resets the root checkout to the previous commit, re-synchronises the previous integration, restores the previous `dist` snapshot and relaunches the known-good launcher where possible. Progress and failure evidence remain under `build/updates/desktop-staging` and the normal update-operation reports.

A stale detached-helper lock is recovered only when its recorded owner process no longer exists. Concurrent update operations remain blocked by the Task Centre resource policy and the updater locks.

## Channels

The helper supports `development`, `stable`, `beta` and explicit `pinned` commit targets. Desktop currently defaults to the trusted `main` development channel; channel input is validated before it reaches Git or PowerShell native arguments.

## Recovery

The manual script remains available:

```powershell
& '.\Update-Hermes-Local.ps1' -Mode Check -Component All -NonInteractive
```

For application rollback evidence, inspect:

```text
build\updates\desktop-staging\<operation-id>\
build\updates\operations\
logs\launcher\launcher.log
```
