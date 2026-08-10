import {
  IconActivityHeartbeat,
  IconAlertTriangle,
  IconBan,
  IconCheck,
  IconClock,
  IconExternalLink,
  IconFile,
  IconLoader2,
  IconPlayerPause,
  IconPlayerPlay,
  IconRefresh,
  IconServer,
  IconStack2,
  IconX
} from '@tabler/icons-react'
import { useMemo, useState } from 'react'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

import type { LocalActionTask } from './types'

export type TaskFilter = 'active' | 'all' | 'cancelled' | 'completed' | 'failed' | 'queued'

export const TASK_FILTERS: { id: TaskFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'active', label: 'Active' },
  { id: 'queued', label: 'Queued' },
  { id: 'completed', label: 'Completed' },
  { id: 'failed', label: 'Failed' },
  { id: 'cancelled', label: 'Cancelled' }
]

const activeStates = new Set<LocalActionTask['status']>(['cancelling', 'paused', 'queued', 'running'])

const ACTION_COPY: Record<LocalActionTask['action'], { feature: string; label: string; route: string }> = {
  backup: { feature: 'Services', label: 'Backup', route: '/services' },
  benchmark: { feature: 'Benchmarks', label: 'Benchmark', route: '/benchmarks' },
  diagnostics: { feature: 'Services', label: 'Diagnostics', route: '/services' },
  'model-download': { feature: 'Models', label: 'Model download', route: '/models' },
  repair: { feature: 'Services', label: 'Repair', route: '/services' },
  restart: { feature: 'Services', label: 'Restart', route: '/services' },
  restore: { feature: 'Restore', label: 'Restore', route: '/restore' },
  security: { feature: 'Security', label: 'Security scan', route: '/security' },
  start: { feature: 'Services', label: 'Start', route: '/services' },
  stop: { feature: 'Services', label: 'Stop', route: '/services' },
  'switch-model': { feature: 'Models', label: 'Model switch', route: '/models' },
  test: { feature: 'Services', label: 'Tests', route: '/services' },
  update: { feature: 'Services', label: 'Update check', route: '/services' }
}

const STATUS_COPY: Record<LocalActionTask['status'], string> = {
  cancelled: 'Cancelled',
  cancelling: 'Cancelling',
  failed: 'Failed',
  interrupted: 'Interrupted',
  paused: 'Paused',
  queued: 'Queued',
  running: 'Running',
  succeeded: 'Completed'
}

function timestamp(value: null | string): number {
  const parsed = Date.parse(value || '')

  return Number.isFinite(parsed) ? parsed : 0
}

function taskMatchesFilter(task: LocalActionTask, filter: TaskFilter): boolean {
  if (filter === 'active') {return activeStates.has(task.status)}

  if (filter === 'queued') {return task.status === 'queued'}

  if (filter === 'completed') {return task.status === 'succeeded'}

  if (filter === 'failed') {return task.status === 'failed' || task.status === 'interrupted'}

  if (filter === 'cancelled') {return task.status === 'cancelled'}

  return true
}

export function filterTasks(tasks: LocalActionTask[], filter: TaskFilter): LocalActionTask[] {
  return [...tasks]
    .filter(task => taskMatchesFilter(task, filter))
    .sort((left, right) => timestamp(right.createdAt) - timestamp(left.createdAt))
}

export function taskElapsed(task: LocalActionTask, now = Date.now()): string {
  const start = timestamp(task.startedAt || task.queuedAt || task.createdAt)
  const end = timestamp(task.completedAt) || now
  const seconds = Math.max(0, Math.floor((end - start) / 1000))
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const remainder = seconds % 60

  if (hours) {return `${hours}h ${minutes}m`}

  if (minutes) {return `${minutes}m ${remainder}s`}

  return `${remainder}s`
}

function relativeTime(value: string, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor((now - timestamp(value)) / 1000))

  if (seconds < 60) {return `${seconds}s ago`}

  if (seconds < 3600) {return `${Math.floor(seconds / 60)}m ago`}

  if (seconds < 86_400) {return `${Math.floor(seconds / 3600)}h ago`}

  return `${Math.floor(seconds / 86_400)}d ago`
}

function taskStage(task: LocalActionTask): string {
  if (task.status === 'queued') {return 'Waiting for resources'}

  if (task.status === 'cancelling') {
    return task.action === 'restore' ? 'Waiting for a safe restore cancellation boundary' : 'Stopping owned process'
  }

  if (task.status === 'paused') {return task.progress?.message || 'Paused with resumable partial data'}

  if (task.status === 'running') {
    return task.stage || task.progress?.message || `${ACTION_COPY[task.action].label} in progress`
  }

  if (task.status === 'interrupted') {return 'Owner exited without a conclusive result'}

  if (task.status === 'failed') {return task.failure?.message || 'Operation failed'}

  if (task.status === 'cancelled') {return 'Cancelled safely'}

  return 'Operation completed'
}

function taskSummary(task: LocalActionTask): string {
  if (task.failure?.message) {return task.failure.message}

  if (task.result?.path) {return task.result.path}

  const line = task.output
    .split(/\r?\n/)
    .map(value => value.trim())
    .filter(Boolean)
    .at(-1)

  return line || taskStage(task)
}

function statusTone(status: LocalActionTask['status']): string {
  if (status === 'succeeded') {return 'text-emerald-500'}

  if (status === 'failed') {return 'text-red-500'}

  if (status === 'interrupted' || status === 'cancelled' || status === 'paused') {return 'text-amber-500'}

  if (status === 'queued' || status === 'cancelling') {return 'text-amber-500'}

  return 'text-(--ui-accent)'
}

function StatusIcon({ status }: { status: LocalActionTask['status'] }) {
  if (status === 'running' || status === 'cancelling') {
    return <IconLoader2 className="size-4 animate-spin motion-reduce:animate-none" />
  }

  if (status === 'succeeded') {return <IconCheck className="size-4" />}

  if (status === 'failed' || status === 'interrupted') {return <IconAlertTriangle className="size-4" />}

  if (status === 'cancelled') {return <IconBan className="size-4" />}

  if (status === 'paused') {return <IconPlayerPause className="size-4" />}

  return <IconClock className="size-4" />
}

function DetailRow({ children, icon: Icon, label }: { children: React.ReactNode; icon: typeof IconClock; label: string }) {
  return (
    <div className="grid gap-1 border-b border-(--ui-stroke-secondary) px-4 py-3 sm:grid-cols-[9rem_minmax(0,1fr)] sm:items-center">
      <div className="flex items-center gap-2 text-xs font-medium text-(--ui-text-tertiary)">
        <Icon className="size-3.5" stroke={1.7} />
        {label}
      </div>
      <div className="min-w-0 text-xs text-(--ui-text-secondary)">{children}</div>
    </div>
  )
}

interface TaskCentreProps {
  modelName: string
  onCancel: (taskId: string) => Promise<void>
  onError: (message: string) => void
  onNavigate: (path: string) => void
  onOpenResult: (taskId: string) => Promise<void>
  onPause?: (taskId: string) => Promise<void>
  onResume?: (taskId: string) => Promise<void>
  onRetry: (taskId: string) => Promise<void>
  profileName: string
  tasks: LocalActionTask[]
}

export function TaskCentre({
  modelName,
  onCancel,
  onError,
  onNavigate,
  onOpenResult,
  onPause,
  onResume,
  onRetry,
  profileName,
  tasks
}: TaskCentreProps) {
  const [filter, setFilter] = useState<TaskFilter>('all')
  const [pendingControl, setPendingControl] = useState('')
  const [selectedTaskId, setSelectedTaskId] = useState('')
  const visibleTasks = useMemo(() => filterTasks(tasks, filter), [filter, tasks])

  const filterCounts = useMemo(
    () => Object.fromEntries(TASK_FILTERS.map(item => [item.id, tasks.filter(task => taskMatchesFilter(task, item.id)).length])),
    [tasks]
  )

  const selected =
    visibleTasks.find(task => task.id === selectedTaskId) ||
    visibleTasks.find(task => task.status === 'running' || task.status === 'cancelling' || task.status === 'paused') ||
    visibleTasks[0]

  const selectedProgressPercent =
    selected?.progress?.mode === 'determinate' && selected.progress.percent !== null
      ? Math.min(100, Math.max(0, selected.progress.percent))
      : null
  const selectedProgressCounters = selected?.progress?.counters || {}

  const moveFilterFocus = (event: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
    let nextIndex = index

    if (event.key === 'ArrowRight') {
      nextIndex = (index + 1) % TASK_FILTERS.length
    } else if (event.key === 'ArrowLeft') {
      nextIndex = (index - 1 + TASK_FILTERS.length) % TASK_FILTERS.length
    } else if (event.key === 'Home') {
      nextIndex = 0
    } else if (event.key === 'End') {
      nextIndex = TASK_FILTERS.length - 1
    } else {
      return
    }

    event.preventDefault()
    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[nextIndex]?.focus()
    setFilter(TASK_FILTERS[nextIndex].id)
  }

  const invoke = async (key: string, operation: () => Promise<void>) => {
    setPendingControl(key)

    try {
      await operation()
    } catch (error) {
      onError(error instanceof Error ? error.message : String(error))
    } finally {
      setPendingControl('')
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex gap-1 overflow-x-auto border-b border-(--ui-stroke-secondary)" role="tablist">
        {TASK_FILTERS.map((item, index) => {
          const count = filterCounts[item.id]

          return (
            <button
              aria-controls="task-centre-panel"
              aria-label={`${item.label} (${count})`}
              aria-selected={filter === item.id}
              className={cn(
                'relative shrink-0 px-3 py-2.5 text-xs font-medium text-(--ui-text-tertiary) transition-colors hover:text-(--ui-text-primary)',
                filter === item.id && 'text-(--ui-text-primary) after:absolute after:inset-x-2 after:bottom-0 after:h-0.5 after:bg-(--ui-accent)'
              )}
              id={`task-filter-${item.id}`}
              key={item.id}
              onClick={() => setFilter(item.id)}
              onKeyDown={event => moveFilterFocus(event, index)}
              role="tab"
              tabIndex={filter === item.id ? 0 : -1}
              type="button"
            >
              {item.label}
              {count > 0 && <span className="ml-1.5 font-mono text-[0.625rem] text-(--ui-text-tertiary)">{count}</span>}
            </button>
          )
        })}
      </div>

      <div
        aria-labelledby={`task-filter-${filter}`}
        className="grid min-h-[34rem] gap-4 lg:grid-cols-[minmax(18rem,0.78fr)_minmax(0,1.5fr)]"
        id="task-centre-panel"
        role="tabpanel"
      >
        <section className="overflow-hidden rounded-xl border border-(--ui-stroke-secondary) bg-(--ui-panel-surface-background)">
          <header className="border-b border-(--ui-stroke-secondary) px-4 py-3 text-xs text-(--ui-text-tertiary)">
            {visibleTasks.length} task{visibleTasks.length === 1 ? '' : 's'}
          </header>
          <div className="max-h-[32rem] overflow-y-auto lg:max-h-none">
            {visibleTasks.length === 0 ? (
              <div className="grid min-h-52 place-items-center px-6 text-center">
                <div>
                  <IconStack2 className="mx-auto size-6 text-(--ui-text-tertiary)" stroke={1.5} />
                  <p className="mt-3 text-sm font-medium">No {TASK_FILTERS.find(item => item.id === filter)?.label.toLowerCase()} tasks</p>
                  <p className="mt-1 text-xs text-(--ui-text-tertiary)">Task history will appear here automatically.</p>
                </div>
              </div>
            ) : (
              visibleTasks.map(task => {
                const active = selected?.id === task.id

                return (
                  <button
                    aria-current={active ? 'true' : undefined}
                    className={cn(
                      'block w-full border-b border-(--ui-stroke-secondary) px-4 py-3 text-left transition-colors last:border-b-0 hover:bg-(--ui-control-hover-background)',
                      active && 'bg-(--ui-control-active-background) shadow-[inset_2px_0_var(--ui-accent)]'
                    )}
                    key={task.id}
                    onClick={() => setSelectedTaskId(task.id)}
                    type="button"
                  >
                    <div className="flex items-center gap-2">
                      <span className={cn('shrink-0', statusTone(task.status))}>
                        <StatusIcon status={task.status} />
                      </span>
                      <span className="min-w-0 flex-1 truncate text-[0.8125rem] font-semibold">
                        {ACTION_COPY[task.action].label}
                      </span>
                      <span className={cn('text-[0.6875rem] font-medium', statusTone(task.status))}>
                        {STATUS_COPY[task.status]}
                      </span>
                      <span className="shrink-0 font-mono text-[0.625rem] text-(--ui-text-tertiary)">
                        {activeStates.has(task.status) ? taskElapsed(task) : relativeTime(task.completedAt || task.updatedAt)}
                      </span>
                    </div>
                    <p className="mt-1 truncate pl-6 text-[0.6875rem] text-(--ui-text-tertiary)">{taskSummary(task)}</p>
                  </button>
                )
              })
            )}
          </div>
        </section>

        <section className="overflow-hidden rounded-xl border border-(--ui-stroke-secondary) bg-(--ui-panel-surface-background)">
          {!selected ? (
            <div className="grid min-h-[34rem] place-items-center px-6 text-center">
              <div>
                <IconStack2 className="mx-auto size-7 text-(--ui-text-tertiary)" stroke={1.5} />
                <h3 className="mt-3 text-sm font-semibold">
                  {tasks.length ? `No ${filter === 'all' ? 'matching' : filter} tasks` : 'No task history yet'}
                </h3>
                <p className="mt-1 max-w-sm text-xs leading-5 text-(--ui-text-tertiary)">
                  {tasks.length
                    ? 'Choose another filter to inspect task progress, output and recovery evidence.'
                    : 'Workstation actions will publish progress, output and recovery evidence here.'}
                </p>
              </div>
            </div>
          ) : (
            <>
              <header className="flex flex-wrap items-center gap-3 border-b border-(--ui-stroke-secondary) px-4 py-4">
                <span className={cn('grid size-8 place-items-center rounded-lg bg-(--ui-control-background)', statusTone(selected.status))}>
                  <StatusIcon status={selected.status} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-base font-semibold">{ACTION_COPY[selected.action].label}</h3>
                    <span className={cn('text-xs font-medium', statusTone(selected.status))}>{STATUS_COPY[selected.status]}</span>
                  </div>
                  <p className="mt-0.5 truncate text-xs text-(--ui-text-tertiary)">{taskStage(selected)}</p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    className="h-8 gap-1.5 px-3 text-xs"
                    disabled={!selected.capabilities.cancel || Boolean(pendingControl)}
                    onClick={() => void invoke('cancel', () => onCancel(selected.id))}
                    size="sm"
                    title={selected.capabilities.cancel ? 'Cancel this owned process' : 'Cancellation is unavailable for this owner or state'}
                    variant="outline"
                  >
                    {pendingControl === 'cancel' ? <IconLoader2 className="size-3.5 animate-spin" /> : <IconX className="size-3.5" />}
                    Cancel
                  </Button>
                  <Button
                    className="h-8 gap-1.5 px-3 text-xs"
                    disabled={!selected.capabilities.pause || Boolean(pendingControl)}
                    onClick={() => void invoke('pause', () => (onPause ? onPause(selected.id) : Promise.resolve()))}
                    size="sm"
                    title={selected.capabilities.pause ? 'Pause after the current safe write boundary' : 'Pause is unavailable in this stage'}
                    variant="outline"
                  >
                    {pendingControl === 'pause' ? <IconLoader2 className="size-3.5 animate-spin" /> : <IconPlayerPause className="size-3.5" />} Pause
                  </Button>
                  <Button
                    className="h-8 gap-1.5 px-3 text-xs"
                    disabled={!selected.capabilities.resume || Boolean(pendingControl)}
                    onClick={() => void invoke('resume', () => (onResume ? onResume(selected.id) : Promise.resolve()))}
                    size="sm"
                    title={selected.capabilities.resume ? 'Resume this task from its retained partial data' : 'Resume is unavailable'}
                    variant="outline"
                  >
                    {pendingControl === 'resume' ? <IconLoader2 className="size-3.5 animate-spin" /> : <IconPlayerPlay className="size-3.5" />} Resume
                  </Button>
                  {selected.capabilities.retry && (
                    <Button
                      className="h-8 gap-1.5 px-3 text-xs"
                      disabled={Boolean(pendingControl)}
                      onClick={() => void invoke('retry', () => onRetry(selected.id))}
                      size="sm"
                    >
                      {pendingControl === 'retry' ? <IconLoader2 className="size-3.5 animate-spin" /> : <IconRefresh className="size-3.5" />}
                      Retry
                    </Button>
                  )}
                </div>
              </header>

              <div
                aria-label={
                  selectedProgressPercent === null
                    ? `${ACTION_COPY[selected.action].label} progress is indeterminate`
                    : `${ACTION_COPY[selected.action].label} progress ${selectedProgressPercent}%`
                }
                aria-valuemax={100}
                aria-valuemin={0}
                aria-valuenow={selectedProgressPercent ?? undefined}
                className="h-1 bg-(--ui-control-background)"
                role="progressbar"
              >
                <div
                  className={cn(
                    'h-full bg-(--ui-accent) transition-[width]',
                    activeStates.has(selected.status) &&
                      selected.status !== 'paused' &&
                      selectedProgressPercent === null &&
                      'w-1/3 animate-pulse motion-reduce:animate-none',
                    selected.status === 'succeeded' && 'w-full bg-emerald-500',
                    (selected.status === 'failed' || selected.status === 'interrupted') && 'w-full bg-red-500',
                    (selected.status === 'cancelled' || selected.status === 'paused') && 'bg-amber-500'
                  )}
                  style={
                    activeStates.has(selected.status) && selectedProgressPercent !== null
                      ? { width: `${selectedProgressPercent}%` }
                      : undefined
                  }
                />
              </div>

              <DetailRow icon={IconClock} label="Stage">
                {taskStage(selected)}
              </DetailRow>
              <DetailRow icon={IconStack2} label="Task ID">
                <span className="font-mono">{selected.id}</span>
              </DetailRow>
              {selected.progress && (
                <DetailRow icon={IconActivityHeartbeat} label="Progress">
                  <div className="space-y-1">
                    <div className="flex flex-wrap gap-x-3 gap-y-1 font-mono text-[0.6875rem]">
                      {selected.progress.completedUnits !== null && selected.progress.totalUnits !== null && (
                        <span>
                          checks {selected.progress.completedUnits}/{selected.progress.totalUnits}
                        </span>
                      )}
                      {Object.entries(selectedProgressCounters).map(([key, value]) => (
                        <span key={key}>
                          {key} {value}
                        </span>
                      ))}
                      {selected.progress.mode === 'indeterminate' && <span>indeterminate</span>}
                    </div>
                    {selected.progress.message && (
                      <p className="text-(--ui-text-tertiary)">{selected.progress.message}</p>
                    )}
                  </div>
                </DetailRow>
              )}
              <DetailRow icon={IconClock} label="Elapsed">
                <span className="font-mono">{taskElapsed(selected)}</span>
              </DetailRow>
              {selected.action === 'model-download' && selected.progress && (
                <DetailRow icon={IconFile} label="Transfer">
                  <span className="font-mono">
                    {selected.progress.bytesCompleted?.toLocaleString() || 0}
                    {selected.progress.bytesTotal ? ` / ${selected.progress.bytesTotal.toLocaleString()} bytes` : ' bytes'}
                    {selected.progress.rateBytesPerSecond
                      ? ` · ${Math.round(selected.progress.rateBytesPerSecond).toLocaleString()} B/s`
                      : ''}
                    {selected.progress.etaSeconds ? ` · ETA ${Math.ceil(selected.progress.etaSeconds)}s` : ''}
                  </span>
                </DetailRow>
              )}
              <DetailRow icon={IconServer} label="Owner">
                <span className="font-mono">
                  {selected.owner.kind === 'external-process' ? 'External process' : 'Desktop process'}
                  {selected.owner.pid ? ` · PID ${selected.owner.pid}` : ''}
                </span>
              </DetailRow>
              <DetailRow icon={IconStack2} label="Project">
                Hermes Local workstation
              </DetailRow>
              <DetailRow icon={IconStack2} label="Model / profile">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <span>{selected.resources.some(claim => claim.resource === 'model-runtime') ? modelName : 'Not model-bound'}</span>
                  <span className="text-(--ui-text-tertiary)">/</span>
                  <span>{profileName || 'Default profile'}</span>
                </div>
              </DetailRow>
              <DetailRow icon={IconStack2} label="Resources">
                <div className="flex flex-wrap gap-x-3 gap-y-1 font-mono text-[0.6875rem]">
                  {selected.resources.length
                    ? selected.resources.map(claim => (
                        <span key={`${claim.resource}-${claim.mode}`}>{claim.resource} · {claim.mode}</span>
                      ))
                    : 'Observational — no exclusive resources'}
                </div>
              </DetailRow>
              <DetailRow icon={IconExternalLink} label="Owner feature">
                <button
                  className="inline-flex items-center gap-1 font-medium text-(--ui-accent) hover:underline"
                  onClick={() => onNavigate(ACTION_COPY[selected.action].route)}
                  type="button"
                >
                  Open {ACTION_COPY[selected.action].feature} <IconExternalLink className="size-3" />
                </button>
              </DetailRow>

              <div className="p-4">
                <div className="flex items-center gap-2">
                  <h4 className="text-xs font-semibold">Output</h4>
                  {selected.outputTruncated && <span className="text-[0.625rem] text-amber-500">Earlier output discarded</span>}
                </div>
                <pre className="mt-2 max-h-56 min-h-28 overflow-auto whitespace-pre-wrap rounded-lg bg-[#111315] p-3 font-mono text-[0.6875rem] leading-5 text-[#d9dde3]">
                  {selected.output || 'No output has been recorded for this task.'}
                </pre>
              </div>

              <div className="border-t border-(--ui-stroke-secondary) px-4 py-3">
                <div className="flex flex-wrap items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <h4 className="text-xs font-semibold">Result / recovery</h4>
                    <p className="mt-1 truncate text-[0.6875rem] text-(--ui-text-tertiary)">
                      {selected.result?.path || selected.failure?.message || 'No result yet. Recovery options appear when the task terminates.'}
                    </p>
                  </div>
                  {selected.result && (
                    <Button
                      className="h-8 gap-1.5 px-3 text-xs"
                      disabled={Boolean(pendingControl)}
                      onClick={() => void invoke('result', () => onOpenResult(selected.id))}
                      size="sm"
                      variant="outline"
                    >
                      <IconFile className="size-3.5" /> Open result
                    </Button>
                  )}
                </div>
              </div>
            </>
          )}
        </section>
      </div>
    </div>
  )
}
