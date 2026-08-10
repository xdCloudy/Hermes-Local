import { useStore } from '@nanostores/react'
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
  if (preferredId !== undefined) {
    const preferred = preferredId?.trim() || null
    return preferred && projects.some(project => !project.archived && project.id === preferred) ? preferred : null
  }
  const path = cwd?.trim() || ''
  if (!path) {
    return null
  }

  let selected: null | string = null
  let bestLength = -1
  for (const project of projects) {
    if (project.archived) {
      continue
    }
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
