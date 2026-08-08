from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[1])
out = Path(sys.argv[2])


def read(path):
    return (root / path).read_text(encoding="utf-8")


def write(path, text):
    (root / path).write_text(text, encoding="utf-8", newline="")


def one(path, old, new):
    text = read(path)
    n = text.count(old)
    if n != 1:
        raise RuntimeError(f"{path}: expected 1 match, got {n}: {old[:100]!r}")
    write(path, text.replace(old, new, 1))


one(
    "apps/desktop/src/types/hermes.ts",
    "  git_repo_root?: null | string\n  ended_at: null | number",
    "  git_repo_root?: null | string\n  /** Stable first-class Project association; null means projectless. */\n"
    "  project_id?: null | string\n  ended_at: null | number",
)
one(
    "apps/desktop/src/types/hermes.ts",
    "  personality?: string\n  provider?: string",
    "  personality?: string\n  project_id?: null | string\n  provider?: string",
)

one(
    "apps/desktop/src/store/session.ts",
    "export const $currentCwd = atom(getRememberedWorkspaceCwd())",
    "export const $currentCwd = atom(getRememberedWorkspaceCwd())\n"
    "export const $currentProjectId = atom<null | string>(null)",
)
one(
    "apps/desktop/src/store/session.ts",
    "export const setCurrentCwd = (cwd: string) => {",
    "export const setCurrentProjectId = (projectId: null | string) => $currentProjectId.set(projectId)\n\n"
    "export const setCurrentCwd = (cwd: string) => {",
)

one(
    "apps/desktop/src/app/session/hooks/use-session-actions/utils.ts",
    "  setCurrentPersonality,\n  setCurrentProvider,",
    "  setCurrentPersonality,\n  setCurrentProjectId,\n  setCurrentProvider,",
)
one(
    "apps/desktop/src/app/session/hooks/use-session-actions/utils.ts",
    "  if (info.cwd) {\n    sessionState.cwd = info.cwd\n  }",
    "  if (info.cwd !== undefined) {\n    sessionState.cwd = info.cwd || ''\n  }\n\n"
    "  if (foreground && info.project_id !== undefined) {\n"
    "    setCurrentProjectId(info.project_id?.trim() || null)\n"
    "  }",
)
one(
    "apps/desktop/src/app/session/hooks/use-session-actions/utils.ts",
    "export function applyStoredSessionPreviewRuntimeInfo(stored: { model?: null | string } | undefined) {\n  setCurrentModel(stored?.model || '')",
    "export function applyStoredSessionPreviewRuntimeInfo(\n"
    "  stored: { model?: null | string; project_id?: null | string } | undefined\n"
    ") {\n  setCurrentModel(stored?.model || '')\n"
    "  setCurrentProjectId(stored?.project_id?.trim() || null)",
)

one(
    "apps/desktop/src/app/session/hooks/use-session-actions/index.ts",
    "  $currentCwd,\n  $currentFastMode,",
    "  $currentCwd,\n  $currentFastMode,\n  $currentProjectId,",
)
one(
    "apps/desktop/src/app/session/hooks/use-session-actions/index.ts",
    "  setCurrentCwdTransient,\n  setCurrentServiceTier,",
    "  setCurrentCwdTransient,\n  setCurrentProjectId,\n  setCurrentServiceTier,",
)
one(
    "apps/desktop/src/app/session/hooks/use-session-actions/index.ts",
    "    provider: $currentProvider.get().trim()\n  }",
    "    provider: $currentProvider.get().trim(),\n    projectId: $currentProjectId.get()?.trim() || null\n  }",
)
one(
    "apps/desktop/src/app/session/hooks/use-session-actions/index.ts",
    "    ...(cwd && { cwd }),\n    ...(profile ? { profile } : {}),",
    "    ...(cwd && { cwd }),\n    ...(selection.projectId ? { project_id: selection.projectId } : {}),\n    ...(profile ? { profile } : {}),",
)
one(
    "apps/desktop/src/app/session/hooks/use-session-actions/index.ts",
    "      setCurrentServiceTier('')\n      setYoloActive(false)",
    "      setCurrentServiceTier('')\n      setCurrentProjectId(null)\n      setYoloActive(false)",
)

one(
    "apps/desktop/src/app/chat/sidebar/projects/workspace-groups.ts",
    "export function liveSessionProjectId(session: SessionInfo, explicitProjects: ProjectInfo[]): null | string {\n  const cwd = (session.cwd || '').trim()",
    "export function liveSessionProjectId(session: SessionInfo, explicitProjects: ProjectInfo[]): null | string {\n"
    "  const stableProjectId = session.project_id?.trim()\n\n"
    "  if (stableProjectId) {\n"
    "    return explicitProjects.some(project => !project.archived && project.id === stableProjectId)\n"
    "      ? stableProjectId\n"
    "      : null\n"
    "  }\n\n"
    "  const cwd = (session.cwd || '').trim()",
)

project_row = r'''import { useStore } from '@nanostores/react'
import { memo } from 'react'

import { StatusRow } from '@/components/chat/status-row'
import { ActionsMenu, type MenuKit, renderActionItem } from '@/components/ui/actions-menu'
import { Button } from '@/components/ui/button'
import { Codicon } from '@/components/ui/codicon'
import { useI18n } from '@/i18n'
import { $projects, openProjectCreate } from '@/store/projects'
import type { ProjectInfo } from '@/types/hermes'

const normalizePath = (value: string) => value.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()

const isInside = (folder: string, target: string) => {
  const base = normalizePath(folder)
  const candidate = normalizePath(target)
  return Boolean(base && candidate && (candidate === base || candidate.startsWith(`${base}/`)))
}

export function selectedProjectIdForCwd(
  projects: ProjectInfo[],
  cwd: null | string | undefined,
  preferredId: null | string | undefined
): null | string {
  const preferred = preferredId?.trim()
  if (preferred) {
    return projects.some(project => !project.archived && project.id === preferred) ? preferred : null
  }

  const path = cwd?.trim() || ''
  if (!path) {
    return null
  }

  let selected: null | string = null
  let bestLength = -1
  for (const project of projects) {
    if (project.archived) continue
    for (const folder of project.folders) {
      if (isInside(folder.path, path) && folder.path.length > bestLength) {
        bestLength = folder.path.length
        selected = project.id
      }
    }
  }
  return selected
}

interface ProjectStatusRowProps {
  onSelectProject: (projectId: null | string, cwd: string) => Promise<void> | void
  selectedProjectId: null | string
}

export const ProjectStatusRow = memo(function ProjectStatusRow({
  onSelectProject,
  selectedProjectId
}: ProjectStatusRowProps) {
  const { t } = useI18n()
  const projects = useStore($projects).filter(project => !project.archived)
  const selected = projects.find(project => project.id === selectedProjectId) ?? null
  const label = selected?.name || t.rightSidebar.noProjectTitle

  const renderItems = (kit: MenuKit) => (
    <>
      {renderActionItem(kit, {
        key: '__no-project__',
        label: <span className="truncate">{t.rightSidebar.noProjectTitle}</span>,
        onSelect: () => void onSelectProject(null, '')
      })}
      {projects.length > 0 && <kit.Separator />}
      {projects.map(project => {
        const cwd =
          project.primary_path?.trim() ||
          project.folders.find(folder => folder.is_primary)?.path?.trim() ||
          project.folders[0]?.path?.trim() ||
          ''
        return renderActionItem(kit, {
          key: project.id,
          label: <span className="truncate">{project.name}</span>,
          disabled: !cwd,
          onSelect: () => void onSelectProject(project.id, cwd)
        })
      })}
      <kit.Separator />
      {renderActionItem(kit, {
        key: '__create-project__',
        label: <span className="truncate">{t.sidebar.projects.createTitle}</span>,
        onSelect: () => openProjectCreate()
      })}
    </>
  )

  return (
    <StatusRow leading={<Codicon className="text-muted-foreground/75" name="folder" size="0.8rem" />}>
      <ActionsMenu align="start" contentClassName="w-64" items={renderItems} side="top">
        <Button className="h-6 min-w-0 max-w-full justify-start gap-1 px-1.5 text-xs font-normal" variant="ghost">
          <span className="truncate">{label}</span>
          <Codicon className="shrink-0 text-muted-foreground/70" name="chevron-down" size="0.7rem" />
        </Button>
      </ActionsMenu>
    </StatusRow>
  )
})
'''
write("apps/desktop/src/app/chat/composer/status-stack/project-row.tsx", project_row)

one("apps/desktop/src/app/chat/composer/index.tsx", "import { selectDesktopPaths } from '@/lib/desktop-fs'\n", "")
one(
    "apps/desktop/src/app/chat/composer/index.tsx",
    "import { toggleReview } from '@/store/review'\nimport { $gatewayState } from '@/store/session'",
    "import { $projects } from '@/store/projects'\nimport { toggleReview } from '@/store/review'\n"
    "import { $currentProjectId, $gatewayState, setCurrentCwdTransient, setCurrentProjectId } from '@/store/session'",
)
one(
    "apps/desktop/src/app/chat/composer/index.tsx",
    "import { CodingStatusRow } from './status-stack/coding-row'",
    "import { CodingStatusRow } from './status-stack/coding-row'\n"
    "import { ProjectStatusRow, selectedProjectIdForCwd } from './status-stack/project-row'",
)
one(
    "apps/desktop/src/app/chat/composer/index.tsx",
    '''  const changeProject = useCallback(async () => {\n    if (!onChangeSessionCwd) {\n      return\n    }\n\n    const [selected] = await selectDesktopPaths({\n      defaultPath: cwd || undefined,\n      directories: true,\n      multiple: false\n    })\n\n    if (selected) {\n      await onChangeSessionCwd(selected)\n    }\n  }, [cwd, onChangeSessionCwd])\n''',
    '''  const projects = useStore($projects)\n  const currentProjectId = useStore($currentProjectId)\n  const selectedProjectId = selectedProjectIdForCwd(\n    projects,\n    cwd,\n    scope.target === 'main' ? currentProjectId : null\n  )\n\n  const changeProject = useCallback(\n    async (projectId: null | string, projectCwd: string) => {\n      if (onChangeSessionCwd) {\n        await onChangeSessionCwd(projectCwd)\n      } else {\n        setCurrentCwdTransient(projectCwd)\n      }\n      if (scope.target === 'main') {\n        setCurrentProjectId(projectId)\n      }\n    },\n    [onChangeSessionCwd, scope.target]\n  )\n''',
)
one(
    "apps/desktop/src/app/chat/composer/index.tsx",
    '''                <CodingStatusRow\n                  onBranchOff={handleBranchOff}\n                  onChangeProject={onChangeSessionCwd ? changeProject : undefined}\n                  onConvertBranch={handleConvertBranch}\n                  onListBranches={handleListBranches}\n                  // A tile's rail reviews ITS worktree: pin the pane's scope to\n                  // that tile's cwd. The main composer passes null for the legacy\n                  // active-session scope (null).\n                  onOpen={() => toggleReview(scope.target === 'main' ? null : (cwd ?? null))}\n                  onOpenWorktree={openInWorktree}\n                  onRemoveProject={onChangeSessionCwd && cwd ? () => onChangeSessionCwd('') : undefined}\n                  onSwitchBranch={handleSwitchBranch}\n                  repoPath={cwd}\n                />''',
    '''                <ProjectStatusRow onSelectProject={changeProject} selectedProjectId={selectedProjectId} />\n                {selectedProjectId && (\n                  <CodingStatusRow\n                    onBranchOff={handleBranchOff}\n                    onConvertBranch={handleConvertBranch}\n                    onListBranches={handleListBranches}\n                    // A tile's rail reviews ITS worktree: pin the pane's scope to\n                    // that tile's cwd. The main composer passes null for the legacy\n                    // active-session scope (null).\n                    onOpen={() => toggleReview(scope.target === 'main' ? null : (cwd ?? null))}\n                    onOpenWorktree={openInWorktree}\n                    onSwitchBranch={handleSwitchBranch}\n                    repoPath={cwd}\n                  />\n                )}''',
)

one(
    "apps/desktop/src/store/updates.ts",
    "// v7: requires session.cwd.set empty-cwd detach semantics.\nconst REQUIRED_BACKEND_CONTRACT = 7",
    "// v7: requires session.cwd.set empty-cwd detach semantics.\n"
    "// v8: requires stable per-session Project identity in session.info.\n"
    "const REQUIRED_BACKEND_CONTRACT = 8",
)
updates = read("apps/desktop/src/store/updates.test.ts")
updates = updates.replace("desktop_contract: 7", "desktop_contract: 8")
updates = updates.replace("desktop_contract: 6", "desktop_contract: 7")
write("apps/desktop/src/store/updates.test.ts", updates)

subprocess.run(["git", "config", "user.name", "xdCloudy"], cwd=root, check=True)
subprocess.run(["git", "config", "user.email", "connerabery@gmail.com"], cwd=root, check=True)
subprocess.run(["git", "add", "-A"], cwd=root, check=True)
subprocess.run(["git", "commit", "-m", "feat(projects): associate chats with stable project ids"], cwd=root, check=True)
patch = subprocess.run(["git", "format-patch", "-1", "--stdout"], cwd=root, check=True, text=True, capture_output=True).stdout
out.write_text(patch, encoding="utf-8", newline="")
print(out)
