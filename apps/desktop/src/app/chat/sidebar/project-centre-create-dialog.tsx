import { useEffect, useMemo, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Codicon } from '@/components/ui/codicon'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { notifyError } from '@/store/notifications'
import { cloneProject, createProject, pickProjectFolder, refreshProjectCentre } from '@/store/projects'

type CreateMode = 'attach' | 'clone' | 'empty'

interface ProjectCentreCreateDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

const MODES: Array<{ id: CreateMode; label: string; description: string }> = [
  { id: 'empty', label: 'Empty', description: 'Create a stable project without a folder or Git repository.' },
  { id: 'attach', label: 'Attach folder', description: 'Register an existing local folder as a stable project.' },
  { id: 'clone', label: 'Clone Git', description: 'Clone a repository into a chosen parent folder and register it.' }
]

const folderName = (path: string): string => path.split(/[\\/]/).filter(Boolean).at(-1) ?? ''

const repositoryName = (url: string): string => {
  const value = url.trim().replace(/[\\/]$/, '')
  const tail = value.split('/').at(-1)?.split(':').at(-1) ?? ''
  return tail.replace(/\.git$/i, '')
}

export function ProjectCentreCreateDialog({ open, onOpenChange }: ProjectCentreCreateDialogProps) {
  const [mode, setMode] = useState<CreateMode>('empty')
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [repositoryUrl, setRepositoryUrl] = useState('')
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    if (!open) {
      return
    }

    setMode('empty')
    setName('')
    setPath('')
    setRepositoryUrl('')
    setSubmitting(false)
  }, [open])

  const selectedMode = useMemo(() => MODES.find(item => item.id === mode) ?? MODES[0], [mode])

  const chooseFolder = async () => {
    try {
      const selected = await pickProjectFolder()
      if (!selected) {
        return
      }

      setPath(selected)
      if (!name.trim() && mode === 'attach') {
        setName(folderName(selected))
      }
    } catch (err) {
      notifyError(err, 'Could not choose a folder')
    }
  }

  const submit = async () => {
    const trimmedName = name.trim()
    if (!trimmedName || submitting) {
      return
    }

    setSubmitting(true)
    try {
      if (mode === 'empty') {
        await createProject({ folders: [], name: trimmedName, use: true })
      } else if (mode === 'attach') {
        if (!path) {
          return
        }
        await createProject({ folders: [path], name: trimmedName, primaryPath: path, use: true })
      } else {
        if (!path || !repositoryUrl.trim()) {
          return
        }
        await cloneProject({
          name: trimmedName,
          parentPath: path,
          repositoryUrl: repositoryUrl.trim(),
          use: true
        })
      }

      await refreshProjectCentre()
      onOpenChange(false)
    } catch (err) {
      notifyError(err, 'Could not create project')
    } finally {
      setSubmitting(false)
    }
  }

  const selectMode = (nextMode: CreateMode) => {
    setMode(nextMode)
    setPath('')
    if (nextMode !== 'clone') {
      setRepositoryUrl('')
    }
  }

  const canSubmit =
    Boolean(name.trim()) &&
    !submitting &&
    (mode === 'empty' ||
      (mode === 'attach' && Boolean(path)) ||
      (mode === 'clone' && Boolean(path && repositoryUrl.trim())))

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-w-lg" onInteractOutside={event => event.preventDefault()}>
        <DialogHeader>
          <DialogTitle>Create project</DialogTitle>
          <DialogDescription>{selectedMode.description}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-wrap gap-1">
          {MODES.map(item => (
            <Button
              key={item.id}
              onClick={() => selectMode(item.id)}
              size="sm"
              type="button"
              variant={mode === item.id ? 'default' : 'ghost'}
            >
              {item.label}
            </Button>
          ))}
        </div>

        <Input
          autoFocus
          disabled={submitting}
          onChange={event => setName(event.target.value)}
          onKeyDown={event => {
            if (event.key === 'Enter' && canSubmit) {
              event.preventDefault()
              void submit()
            }
          }}
          placeholder="Project name"
          value={name}
        />

        {mode === 'clone' && (
          <Input
            disabled={submitting}
            onBlur={() => {
              if (!name.trim()) {
                setName(repositoryName(repositoryUrl))
              }
            }}
            onChange={event => setRepositoryUrl(event.target.value)}
            placeholder="Repository URL (HTTPS or SSH)"
            value={repositoryUrl}
          />
        )}

        {mode !== 'empty' && (
          <div className="flex min-w-0 items-center gap-2">
            <Input
              className="min-w-0 flex-1"
              disabled
              placeholder={mode === 'clone' ? 'Clone destination parent folder' : 'Folder to attach'}
              value={path}
            />
            <Button disabled={submitting} onClick={() => void chooseFolder()} type="button" variant="ghost">
              <Codicon name="folder-opened" size="0.8rem" />
              Choose…
            </Button>
          </div>
        )}

        {mode === 'empty' && (
          <p className="text-xs text-(--ui-text-tertiary)">
            This project starts without a filesystem location. You can attach a folder later without changing its
            project identity.
          </p>
        )}

        <DialogFooter>
          <Button disabled={submitting} onClick={() => onOpenChange(false)} type="button" variant="ghost">
            Cancel
          </Button>
          <Button disabled={!canSubmit} onClick={() => void submit()} type="button">
            {mode === 'clone' ? 'Clone & create' : 'Create project'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
