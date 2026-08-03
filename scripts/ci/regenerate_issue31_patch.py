from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(os.environ["GITHUB_WORKSPACE"]).resolve()
PATCH_PATH = ROOT / "source/hermes-launcher/patches/0037-refactor-desktop-consolidate-stack-controls-on-home.patch"


def run(*args: str, cwd: Path, capture: bool = False) -> str:
    result = subprocess.run(
        args,
        cwd=cwd,
        check=True,
        text=True,
        capture_output=capture,
    )
    return result.stdout if capture else ""


def replace_once(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{relative}: expected one match, found {count}: {old[:100]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")


def main() -> None:
    first_line = PATCH_PATH.read_text(encoding="utf-8").splitlines()[0]
    if not first_line.startswith("From 0000000000000000000000000000000000000000"):
        print("Issue 31 patch is already generated; no changes required.")
        return

    manifest = json.loads((ROOT / "VERSION.json").read_text(encoding="utf-8"))
    source = Path(tempfile.mkdtemp(prefix="hermes-agent-issue31-"))
    try:
        run("git", "clone", "--no-checkout", manifest["sources"]["hermesAgent"]["repository"], str(source), cwd=ROOT)
        run("git", "checkout", "--detach", manifest["sources"]["hermesAgent"]["commit"], cwd=source)
        run("git", "config", "user.name", "Hermes Local Integration", cwd=source)
        run("git", "config", "user.email", "hermes-local-ci@localhost", cwd=source)

        patches = sorted((ROOT / "source/hermes-launcher/patches").glob("*.patch"))[:36]
        run(
            "git",
            "am",
            "--3way",
            "--committer-date-is-author-date",
            *(str(path) for path in patches),
            cwd=source,
        )

        replace_once(source, "apps/desktop/src/app/chat/sidebar/index.tsx", "  SERVICES_ROUTE,\n", "")
        replace_once(
            source,
            "apps/desktop/src/app/chat/sidebar/index.tsx",
            "  {\n    id: 'services',\n    label: '',\n    icon: props => <Codicon name=\"server-process\" {...props} />,\n    route: SERVICES_ROUTE\n  },\n",
            "",
        )
        replace_once(
            source,
            "apps/desktop/src/app/contrib/surfaces.tsx",
            "import { contributedRoutes, LEGACY_TOOLS_REDIRECT_ROUTE, NEW_CHAT_ROUTE, ROUTES_AREA, sessionRoute } from '../routes'\n",
            "import {\n  contributedRoutes,\n  LEGACY_SERVICES_REDIRECT_ROUTE,\n  LEGACY_TOOLS_REDIRECT_ROUTE,\n  NEW_CHAT_ROUTE,\n  ROUTES_AREA,\n  sessionRoute\n} from '../routes'\n",
        )
        replace_once(
            source,
            "apps/desktop/src/app/contrib/surfaces.tsx",
            '      <Route element={page(<LocalWorkstationView />)} path="services" />\n',
            '      <Route element={<Navigate replace to={LEGACY_SERVICES_REDIRECT_ROUTE} />} path="services" />\n',
        )
        replace_once(
            source,
            "apps/desktop/src/app/routes.ts",
            "export const ABOUT_ROUTE = '/about'\n",
            "export const ABOUT_ROUTE = '/about'\nexport const LEGACY_SERVICES_REDIRECT_ROUTE = HOME_ROUTE\n",
        )
        replace_once(
            source,
            "apps/desktop/src/app/routes.test.ts",
            "  appViewForPath,\n",
            "  appViewForPath,\n  HOME_ROUTE,\n  LEGACY_SERVICES_REDIRECT_ROUTE,\n",
        )
        replace_once(
            source,
            "apps/desktop/src/app/routes.test.ts",
            "  primaryRouteSelectedSessionId,\n",
            "  primaryRouteSelectedSessionId,\n  SERVICES_ROUTE,\n",
        )
        routes_test = source / "apps/desktop/src/app/routes.test.ts"
        routes_test.write_text(
            routes_test.read_text(encoding="utf-8")
            + "\ndescribe('legacy Services route', () => {\n"
            + "  it('remains reserved and redirects to the canonical Home surface', () => {\n"
            + "    expect(appViewForPath(SERVICES_ROUTE)).toBe('workstation')\n"
            + "    expect(LEGACY_SERVICES_REDIRECT_ROUTE).toBe(HOME_ROUTE)\n"
            + "  })\n"
            + "})\n",
            encoding="utf-8",
            newline="\n",
        )
        replace_once(
            source,
            "apps/desktop/e2e/hermes-local-portable.spec.ts",
            "      await expect(workstationNav.getByRole('button', { exact: true, name: 'Tools' })).toHaveCount(0)\n",
            "      await expect(workstationNav.getByRole('button', { exact: true, name: 'Tools' })).toHaveCount(0)\n"
            "      await expect(workstationNav.getByRole('button', { exact: true, name: 'Services' })).toHaveCount(0)\n",
        )
        replace_once(
            source,
            "apps/desktop/e2e/hermes-local-portable.spec.ts",
            "      await expect(page.getByText('Ready for local inference')).toBeVisible()\n",
            "      await expect(page.getByText('Ready for local inference')).toBeVisible()\n"
            "      await expect(page.getByRole('button', { name: 'Repair', exact: true })).toBeVisible()\n"
            "      await expect(page.getByRole('button', { name: 'Run tests', exact: true })).toBeVisible()\n"
            "      await expect(page.getByRole('button', { name: 'Export diagnostics', exact: true })).toBeVisible()\n"
            "      await expect(page.getByRole('button', { name: 'Back up', exact: true })).toBeVisible()\n"
            "      await expect(page.getByRole('button', { name: 'Check for updates', exact: true })).toBeVisible()\n",
        )

        home_operations = r'''function HomeOperations({
  onRun,
  snapshot
}: {
  onRun: (action: LocalAction, input?: Record<string, unknown>) => void
  snapshot: LocalWorkstationSnapshot
}) {
  const latestUpdate = snapshot.updates.latest
  const updateBusy = snapshot.tasks.some(
    task =>
      task.action === 'update' &&
      (task.status === 'queued' || task.status === 'running' || task.status === 'cancelling')
  )
  const compatibilityReady =
    latestUpdate?.mode === 'Compatibility' &&
    latestUpdate.status === 'succeeded' &&
    latestUpdate.target?.updateAvailable === true
  const latestTask =
    snapshot.tasks.findLast(
      candidate => candidate.status === 'queued' || candidate.status === 'running' || candidate.status === 'cancelling'
    ) || snapshot.tasks.at(-1)

  return (
    <div className="space-y-4">
      <Surface title="Stack operations">
        <div className="flex flex-wrap gap-2 p-4">
          <ActionButton action="repair" available={snapshot.actions.repair} onRun={onRun} tasks={snapshot.tasks}>
            <IconRestore className="size-3.5" /> Repair
          </ActionButton>
          <ActionButton action="test" available={snapshot.actions.test} onRun={onRun} tasks={snapshot.tasks}>
            <IconActivityHeartbeat className="size-3.5" /> Run tests
          </ActionButton>
          <ActionButton action="diagnostics" available={snapshot.actions.diagnostics} onRun={onRun} tasks={snapshot.tasks}>
            <IconFileAnalytics className="size-3.5" /> Export diagnostics
          </ActionButton>
          <ActionButton action="backup" available={snapshot.actions.backup} onRun={onRun} tasks={snapshot.tasks}>
            <IconDatabase className="size-3.5" /> Back up
          </ActionButton>
        </div>
        {latestTask && (
          <div className="border-t border-(--ui-stroke-secondary) px-4 py-3">
            <p className="text-xs font-semibold">
              {latestTask.action} · {latestTask.status}
            </p>
            {latestTask.output && (
              <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded-lg bg-[#111315] p-3 font-mono text-[0.6875rem] text-[#d9dde3]">
                {latestTask.output}
              </pre>
            )}
          </div>
        )}
      </Surface>
      <Surface title="Hermes Agent update centre">
        <div className="grid gap-4 p-4 xl:grid-cols-[1.1fr_0.9fr]">
          <div>
            <div className="grid gap-3 sm:grid-cols-2">
              {[
                ['Installed upstream base', shortCommit(snapshot.updates.installed.baseCommit)],
                ['Integrated commit', shortCommit(snapshot.updates.installed.integrationCommit)],
                ['Integrated tree', shortCommit(snapshot.updates.installed.integrationTree)],
                ['Patch series', `${snapshot.updates.installed.patchCount} patches`],
                ['Target', shortCommit(latestUpdate?.target?.candidate)],
                [
                  'Compatibility',
                  latestUpdate?.mode === 'Compatibility' && latestUpdate.status === 'succeeded'
                    ? 'Passed'
                    : latestUpdate?.failure?.stage || 'Not checked'
                ]
              ].map(([label, value]) => (
                <div className="rounded-lg border border-(--ui-stroke-secondary) px-3 py-2.5" key={label}>
                  <p className="text-[0.6875rem] font-medium text-(--ui-text-tertiary)">{label}</p>
                  <p className="mt-1 truncate font-mono text-xs font-semibold">{value}</p>
                </div>
              ))}
            </div>
            {latestUpdate?.recovery.staleLockRecovered && (
              <p className="mt-3 rounded-lg border border-amber-500/25 bg-amber-500/8 px-3 py-2 text-xs text-amber-500">
                A stale update lock was recovered from operation {latestUpdate.recovery.previousOperationId || 'unknown'}.
              </p>
            )}
            {latestUpdate?.failure && (
              <p className="mt-3 rounded-lg border border-red-500/25 bg-red-500/8 px-3 py-2 text-xs text-red-500">
                {latestUpdate.failure.stage || latestUpdate.currentStage || 'update'}: {latestUpdate.failure.message}
                {latestUpdate.failure.rollback?.status && ` · rollback ${latestUpdate.failure.rollback.status}`}
              </p>
            )}
          </div>
          <div className="flex flex-col justify-between gap-4 rounded-lg bg-(--ui-control-background) p-4">
            <div>
              <p className="text-xs font-semibold">Transactional workflow</p>
              <p className="mt-1 text-xs leading-5 text-(--ui-text-tertiary)">
                Check resolves the upstream target. Compatibility reconstructs the patch series and runs dependency,
                schema, test and build gates without touching the active backend. Apply promotes only a verified candidate
                and restores the previous installation if promotion or health checks fail.
              </p>
              {latestUpdate && (
                <p className="mt-3 font-mono text-[0.6875rem] text-(--ui-text-tertiary)">
                  {latestUpdate.mode} · {latestUpdate.currentStage || latestUpdate.status} · {latestUpdate.progress.percent}%
                </p>
              )}
            </div>
            <div className="flex flex-wrap gap-2">
              <ActionButton
                action="update"
                available={snapshot.actions.update}
                input={{ mode: 'Check' satisfies LocalUpdateMode }}
                onRun={onRun}
                tasks={snapshot.tasks}
              >
                <IconRefresh className="size-3.5" /> Check for updates
              </ActionButton>
              <ActionButton
                action="update"
                available={snapshot.actions.update && latestUpdate?.target?.updateAvailable === true}
                input={{
                  mode: 'Compatibility' satisfies LocalUpdateMode,
                  targetCommit: latestUpdate?.target?.candidate || undefined
                }}
                onRun={onRun}
                tasks={snapshot.tasks}
              >
                <IconShieldCheck className="size-3.5" /> Check compatibility
              </ActionButton>
              <ActionButton
                action="update"
                available={snapshot.actions.update && compatibilityReady}
                input={{
                  mode: 'Apply' satisfies LocalUpdateMode,
                  targetCommit: latestUpdate?.target?.candidate || undefined
                }}
                onRun={onRun}
                tasks={snapshot.tasks}
                variant="default"
              >
                <IconRocket className="size-3.5" /> Apply verified update
              </ActionButton>
              <ActionButton
                action="update"
                available={snapshot.actions.update && !updateBusy}
                input={{ mode: 'Rollback' satisfies LocalUpdateMode }}
                onRun={onRun}
                tasks={snapshot.tasks}
                variant="destructive"
              >
                <IconRestore className="size-3.5" /> Recover previous backend
              </ActionButton>
            </div>
          </div>
        </div>
      </Surface>
    </div>
  )
}

'''
        replace_once(
            source,
            "apps/desktop/src/app/local-workstation/index.tsx",
            "function HomeContent({\n",
            home_operations + "function HomeContent({\n",
        )
        replace_once(
            source,
            "apps/desktop/src/app/local-workstation/index.tsx",
            '      <Surface title="Integrity and provenance">\n',
            '      <HomeOperations onRun={onRun} snapshot={snapshot} />\n\n      <Surface title="Integrity and provenance">\n',
        )

        run("git", "add", "apps/desktop", cwd=source)
        run("git", "commit", "-m", "refactor(desktop): consolidate stack controls on Home", cwd=source)
        patch = run("git", "format-patch", "-1", "--stdout", cwd=source, capture=True)
        PATCH_PATH.write_text(patch, encoding="utf-8", newline="\n")

        run("git", "config", "user.name", "Hermes Local Integration", cwd=ROOT)
        run("git", "config", "user.email", "hermes-local-ci@localhost", cwd=ROOT)
        run("git", "add", str(PATCH_PATH.relative_to(ROOT)), cwd=ROOT)
        run("git", "commit", "-m", "fix(ci): regenerate issue 31 integration patch", cwd=ROOT)
        run("git", "push", "origin", "HEAD:fix/issue-31-home-stack-controls", cwd=ROOT)
    finally:
        shutil.rmtree(source, ignore_errors=True)


if __name__ == "__main__":
    main()
