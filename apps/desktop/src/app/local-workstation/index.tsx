import {
  IconActivityHeartbeat,
  IconAlertTriangle,
  IconBolt,
  IconBrain,
  IconBrandWindows,
  IconCheck,
  IconChevronRight,
  IconCpu,
  IconDatabase,
  IconExternalLink,
  IconFileAnalytics,
  IconFolder,
  IconGauge,
  IconKey,
  IconLoader2,
  IconPlayerPlay,
  IconPlus,
  IconRefresh,
  IconRestore,
  IconRocket,
  IconServer2,
  IconShieldCheck,
  IconSquare,
  IconTerminal2,
  IconTrash,
  IconX
} from '@tabler/icons-react'
import { type ComponentType, type ReactNode, useCallback, useEffect, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router'

import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { cn } from '@/lib/utils'

import { AboutHub } from './about-hub'
import { ModelDownloadCard } from './model-download-card'
import { latestSecurityTask, securityTaskState } from './security-task'
import { TaskCentre } from './task-centre'
import { TrustCentre } from './trust-centre'
import type {
  HermesLocalDashboardBounds,
  HermesLocalDashboardState,
  LocalAction,
  LocalActionTask,
  LocalBackup,
  LocalInferenceProfile,
  LocalLog,
  LocalModel,
  LocalUpdateMode,
  LocalWorkstationSettings,
  LocalWorkstationSnapshot
} from './types'
import { deriveLocalWorkstationStatus } from './workstation-status'

const SECTION_COPY = {
  about: ['About', 'Pinned sources, build identity and workstation paths.'],
  benchmarks: ['Benchmarks', 'Measured performance, stability and profile selection evidence.'],
  dashboard: ['Dashboard', 'Open the full local Hermes web management surface.'],
  home: ['Local AI workstation', 'One control centre for the model, Hermes and local operations.'],
  logs: ['Logs', 'Live, redacted output from each local service.'],
  memory: ['Memory', 'Local state, session index and explicit memory controls.'],
  models: ['Models', 'Verified weights, runtime support and context configuration.'],
  'local-profiles': ['Profiles', 'Editable inference profiles kept as versioned structured data.'],
  projects: ['Projects', 'Manage local workspaces with the official Hermes project surface.'],
  restore: ['Restore', 'Validate a managed backup and restore user data with automatic rollback.'],
  security: ['Security', 'Loopback trust boundaries, audits and remediation evidence.'],
  trust: ['Trust Centre', 'Source-bound skills, MCP permissions, scoped grants and invocation audit.'],
  sessions: ['Sessions', 'Resume and manage persistent local Hermes conversations.'],
  services: ['Services', 'Structured health, process ownership and lifecycle controls.'],
  tasks: ['Tasks', 'Background operations, progress and recovery history.']
} as const

type Section = keyof typeof SECTION_COPY

const LOG_NAMES: LocalLog[] = ['supervisor', 'model', 'hermes', 'dashboard', 'security', 'launcher']

function bytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) {
    return '0 B'
  }

  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  const order = Math.min(units.length - 1, Math.floor(Math.log(value) / Math.log(1024)))

  return `${(value / 1024 ** order).toFixed(order >= 3 ? 1 : 0)} ${units[order]}`
}

function shortCommit(value: null | string | undefined): string {
  return value ? value.slice(0, 8) : 'Unavailable'
}

function StatusPill({ label, ok }: { label: string; ok: boolean }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[0.6875rem] font-semibold',
        ok
          ? 'border-emerald-500/25 bg-emerald-500/8 text-emerald-500'
          : 'border-amber-500/25 bg-amber-500/8 text-amber-500'
      )}
    >
      <span className={cn('size-1.5 rounded-full', ok ? 'bg-emerald-500' : 'bg-amber-500')} />
      {label}
    </span>
  )
}

function Surface({ children, className, title }: { children: ReactNode; className?: string; title?: string }) {
  return (
    <section
      className={cn(
        'rounded-xl border border-(--ui-stroke-secondary) bg-(--ui-panel-surface-background) shadow-[0_1px_0_color-mix(in_srgb,var(--ui-text-primary)_4%,transparent)]',
        className
      )}
    >
      {title && (
        <header className="border-b border-(--ui-stroke-secondary) px-4 py-3 text-xs font-semibold uppercase tracking-[0.08em] text-(--ui-text-tertiary)">
          {title}
        </header>
      )}
      {children}
    </section>
  )
}

function ResourceBar({
  icon: Icon,
  label,
  note,
  percent,
  value
}: {
  icon: ComponentType<{ className?: string; stroke?: number }>
  label: string
  note: string
  percent: number
  value: string
}) {
  const bounded = Math.max(0, Math.min(100, percent))

  return (
    <div className="px-4 py-3">
      <div className="mb-2 flex items-center gap-2">
        <Icon className="size-4 text-(--ui-accent)" stroke={1.7} />
        <span className="text-xs font-medium">{label}</span>
        <span className="ml-auto font-mono text-xs font-semibold">{value}</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-(--ui-control-background)">
        <div
          className="h-full rounded-full bg-(--ui-accent) transition-[width] duration-500"
          style={{ width: `${bounded}%` }}
        />
      </div>
      <p className="mt-1.5 text-[0.6875rem] text-(--ui-text-tertiary)">{note}</p>
    </div>
  )
}

function ActionButton({
  action,
  available,
  children,
  input,
  onRun,
  tasks,
  variant = 'outline'
}: {
  action: LocalAction
  available: boolean
  children: ReactNode
  input?: Record<string, unknown>
  onRun: (action: LocalAction, input?: Record<string, unknown>) => void
  tasks: LocalActionTask[]
  variant?: 'default' | 'destructive' | 'outline'
}) {
  const busy = tasks.some(
    task => task.action === action && (task.status === 'queued' || task.status === 'running' || task.status === 'cancelling')
  )

  return (
    <Button
      className="h-8 gap-1.5 px-3 text-xs"
      disabled={!available || busy}
      onClick={() => onRun(action, input)}
      size="sm"
      variant={variant}
    >
      {busy && <IconLoader2 className="size-3.5 animate-spin motion-reduce:animate-none" />}
      {children}
    </Button>
  )
}

function ServiceRow({
  detail,
  healthy,
  name,
  pid
}: {
  detail: string
  healthy: boolean
  name: string
  pid?: null | number
}) {
  return (
    <div className="flex items-center gap-3 border-b border-(--ui-stroke-secondary) px-4 py-3 last:border-b-0">
      <div
        className={cn(
          'grid size-8 place-items-center rounded-lg',
          healthy ? 'bg-emerald-500/10 text-emerald-500' : 'bg-(--ui-control-background) text-(--ui-text-tertiary)'
        )}
      >
        <IconServer2 className="size-4" stroke={1.8} />
      </div>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[0.8125rem] font-semibold">{name}</p>
        <p className="truncate text-[0.6875rem] text-(--ui-text-tertiary)">{detail}</p>
      </div>
      {pid && <span className="font-mono text-[0.6875rem] text-(--ui-text-tertiary)">PID {pid}</span>}
      <StatusPill label={healthy ? 'Healthy' : 'Offline'} ok={healthy} />
    </div>
  )
}

function ProfileEditor({
  onCreate,
  onDelete,
  onSave,
  onSelect,
  selected,
  snapshot
}: {
  onCreate: (profile: LocalInferenceProfile) => Promise<void>
  onDelete: (name: string) => Promise<void>
  onSave: (profile: LocalInferenceProfile, originalName: string) => Promise<void>
  onSelect: (name: string) => Promise<void>
  selected: string
  snapshot: LocalWorkstationSnapshot
}) {
  const profile = snapshot.profiles?.profiles.find(item => item.name === selected) ?? snapshot.profiles?.profiles[0]
  const [draft, setDraft] = useState<LocalInferenceProfile | null>(profile ? structuredClone(profile) : null)
  const [saving, setSaving] = useState(false)
  const [sourceProfileName, setSourceProfileName] = useState(profile?.name || '')

  useEffect(() => {
    if ((profile?.name || '') !== sourceProfileName) {
      setSourceProfileName(profile?.name || '')
      setDraft(profile ? structuredClone(profile) : null)
    }
  }, [profile, sourceProfileName])

  if (!draft || !snapshot.profiles) {
    return <p className="p-5 text-sm text-(--ui-text-tertiary)">Profiles are unavailable.</p>
  }

  const serverArguments = snapshot.model.server.extraArguments
  const specTypeIndex = serverArguments.indexOf('--spec-type')
  const specType = specTypeIndex >= 0 ? serverArguments[specTypeIndex + 1]?.toLowerCase() : undefined

  const modelSpeculativeDecoding =
    (specType !== undefined && specType !== 'none') ||
    serverArguments.some(argument =>
      ['-md', '--model-draft', '--model-draft-url'].includes(argument.toLowerCase())
    )

  const numeric = (
    label: string,
    value: number,
    update: (next: number) => void,
    min: number,
    max: number,
    step = 1
  ) => (
    <label className="space-y-1.5 text-xs">
      <span className="font-medium text-(--ui-text-secondary)">{label}</span>
      <input
        className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 font-mono outline-none focus:border-(--ui-accent)"
        max={max}
        min={min}
        onChange={event => update(Number(event.currentTarget.value))}
        step={step}
        type="number"
        value={value}
      />
    </label>
  )

  const automaticNumber = (
    label: string,
    value: 'auto' | number,
    update: (next: 'auto' | number) => void,
    maximum: number
  ) => (
    <label className="space-y-1.5 text-xs">
      <span className="font-medium text-(--ui-text-secondary)">{label}</span>
      <input
        className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 font-mono outline-none focus:border-(--ui-accent)"
        onChange={event => {
          const next = event.currentTarget.value.trim().toLocaleLowerCase()
          const parsed = Number(next)

          update(
            next === '' || next === 'auto' || !Number.isSafeInteger(parsed)
              ? 'auto'
              : Math.min(maximum, Math.max(0, parsed))
          )
        }}
        placeholder="auto"
        type="text"
        value={value}
      />
    </label>
  )

  return (
    <div className="grid gap-0 lg:grid-cols-[14rem_1fr]">
      <div className="border-b border-(--ui-stroke-secondary) p-3 lg:border-r lg:border-b-0">
        <Button
          className="mb-2 h-8 w-full gap-1.5 text-xs"
          onClick={() => {
            const baseName = 'Custom profile'
            let name = baseName
            let suffix = 2

            while (snapshot.profiles?.profiles.some(item => item.name === name)) {
              name = `${baseName} ${suffix}`
              suffix += 1
            }

            void onCreate({
              ...structuredClone(draft),
              description: 'Custom inference profile.',
              name
            })
          }}
          size="sm"
          variant="outline"
        >
          <IconPlus className="size-3.5" />
          New profile
        </Button>
        <div className="space-y-1">
          {snapshot.profiles.profiles.map(item => (
            <button
              className={cn(
                'flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-xs transition-colors hover:bg-(--ui-control-hover-background)',
                item.name === selected && 'bg-(--ui-control-active-background) font-semibold text-(--ui-accent)'
              )}
              key={item.name}
              onClick={() => void onSelect(item.name)}
              type="button"
            >
              {item.experimental ? <IconAlertTriangle className="size-3.5" /> : <IconGauge className="size-3.5" />}
              <span className="min-w-0 flex-1 truncate">{item.name}</span>
              {item.name === snapshot.profiles?.selected && <IconCheck className="size-3.5" />}
            </button>
          ))}
        </div>
      </div>
      <div className="p-4">
        <div className="mb-4">
          <label className="space-y-1.5 text-xs">
            <span className="font-medium text-(--ui-text-secondary)">Profile name</span>
            <input
              className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 outline-none focus:border-(--ui-accent)"
              onChange={event => setDraft({ ...draft, name: event.currentTarget.value })}
              value={draft.name}
            />
          </label>
          <label className="mt-3 block space-y-1.5 text-xs">
            <span className="font-medium text-(--ui-text-secondary)">Description</span>
            <input
              className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 outline-none focus:border-(--ui-accent)"
              onChange={event => setDraft({ ...draft, description: event.currentTarget.value })}
              value={draft.description}
            />
          </label>
        </div>
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {numeric(
            'Context tokens',
            draft.contextTokens,
            value => setDraft({ ...draft, contextTokens: value }),
            2048,
            4_194_304,
            1024
          )}
          {numeric(
            'Generation threads',
            draft.threads.generation,
            value => setDraft({ ...draft, threads: { ...draft.threads, generation: value } }),
            1,
            512
          )}
          {numeric(
            'Batch threads',
            draft.threads.batch,
            value => setDraft({ ...draft, threads: { ...draft.threads, batch: value } }),
            1,
            512
          )}
          {numeric(
            'Logical batch',
            draft.batch.logical,
            value => setDraft({ ...draft, batch: { ...draft.batch, logical: value } }),
            32,
            65_536,
            32
          )}
          {numeric(
            'Micro-batch',
            draft.batch.physical,
            value => setDraft({ ...draft, batch: { ...draft.batch, physical: value } }),
            16,
            16_384,
            16
          )}
          {numeric(
            'VRAM reserve MiB',
            draft.gpu.vramReserveMiB,
            value => setDraft({ ...draft, gpu: { ...draft.gpu, vramReserveMiB: value } }),
            0,
            131_072,
            128
          )}
          {automaticNumber(
            'GPU layers',
            draft.gpu.layers,
            value => setDraft({ ...draft, gpu: { ...draft.gpu, layers: value } }),
            9_999
          )}
          {(
            [
              ['KV cache keys', 'keyType'],
              ['KV cache values', 'valueType']
            ] as const
          ).map(([label, key]) => (
            <label className="space-y-1.5 text-xs" htmlFor={`profile-kv-cache-${key}`} key={key}>
              <span className="font-medium text-(--ui-text-secondary)">{label}</span>
              <Select
                onValueChange={value =>
                  setDraft({
                    ...draft,
                    kvCache: {
                      ...draft.kvCache,
                      [key]: value as 'f16' | 'q4_0' | 'q8_0'
                    }
                  })
                }
                value={draft.kvCache[key]}
              >
                <SelectTrigger className="font-mono" id={`profile-kv-cache-${key}`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem className="font-mono" value="q8_0">
                    q8_0
                  </SelectItem>
                  <SelectItem className="font-mono" value="q4_0">
                    q4_0
                  </SelectItem>
                  <SelectItem className="font-mono" value="f16">
                    f16
                  </SelectItem>
                </SelectContent>
              </Select>
            </label>
          ))}
        </div>
        <div className="mt-4 flex flex-wrap gap-2">
          {(
            [
              ['Flash Attention', 'flashAttention'],
              ['Prompt cache', 'promptCache'],
              ['Speculative decoding', 'speculativeDecoding']
            ] as const
          ).map(([label, key]) => {
            const modelManaged = key === 'speculativeDecoding' && modelSpeculativeDecoding
            const active = draft[key] || modelManaged

            return (
              <button
                aria-pressed={active}
                className={cn(
                  'rounded-full border px-2.5 py-1 text-[0.6875rem] font-medium',
                  active
                    ? 'border-(--ui-accent)/40 bg-(--ui-accent)/10 text-(--ui-accent)'
                    : 'border-(--ui-stroke-secondary) text-(--ui-text-tertiary)',
                  modelManaged && 'cursor-not-allowed'
                )}
                disabled={modelManaged}
                key={key}
                onClick={() => setDraft({ ...draft, [key]: !draft[key] })}
                title={modelManaged ? 'Enabled by the selected model manifest' : undefined}
                type="button"
              >
                {modelManaged ? `${label} · model` : label}
              </button>
            )
          })}
        </div>
        <div className="mt-5 flex items-center justify-end gap-2 border-t border-(--ui-stroke-secondary) pt-4">
          <Button
            className="mr-auto h-8 gap-1.5 text-xs"
            disabled={saving || snapshot.profiles.profiles.length <= 1}
            onClick={() => void onDelete(selected)}
            size="sm"
            variant="destructive"
          >
            <IconTrash className="size-3.5" />
            Delete
          </Button>
          <Button
            className="h-8 gap-1.5 text-xs"
            disabled={saving}
            onClick={() => {
              setSaving(true)
              void onSave(draft, sourceProfileName)
                .catch(() => undefined)
                .finally(() => setSaving(false))
            }}
            size="sm"
          >
            {saving && <IconLoader2 className="size-3.5 animate-spin" />}
            Save profile
          </Button>
        </div>
      </div>
    </div>
  )
}

function ModelManager({
  onRegister,
  onRemove,
  onSelect,
  snapshot
}: {
  onRegister: (model: Partial<LocalModel> & { localPath: string }) => Promise<void>
  onRemove: (id: string) => Promise<void>
  onSelect: (id: string) => Promise<void>
  snapshot: LocalWorkstationSnapshot
}) {
  const importModel = async () => {
    const [localPath] = await window.hermesDesktop.selectPaths({
      filters: [{ extensions: ['gguf'], name: 'GGUF models' }],
      multiple: false,
      title: 'Register a local GGUF model'
    })

    if (localPath) {
      await onRegister({ localPath })
    }
  }

  const switching = snapshot.lifecycle.switchingModel
  const switchingActive = Boolean(switching)

  return (
    <Surface className="overflow-hidden" title="Registered models">
      <div className="divide-y divide-(--ui-stroke-secondary)">
        {switching && (
          <div className="bg-(--ui-accent)/5 px-4 py-3 text-xs text-(--ui-text-secondary)">
            Switching {switching.previousModelId || "current model"} → {switching.targetAlias || switching.targetModelId}.
            <span className="ml-1 font-medium text-(--ui-text-primary)">{switching.stage || "queued"}</span>
          </div>
        )}
        {snapshot.models.map(model => (
          <div className="flex flex-wrap items-center gap-3 px-4 py-3" key={model.id}>
            <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-(--ui-accent)/10 text-(--ui-accent)">
              <IconBrain className="size-4.5" stroke={1.7} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-semibold">{model.displayName}</p>
              <p className="truncate font-mono text-[0.6875rem] text-(--ui-text-tertiary)">
                {model.alias} ·{' '}
                {model.installed ? bytes(model.actualSizeBytes || model.sizeBytes || 0) : 'file missing'}
              </p>
            </div>
            {model.id === snapshot.settings.selectedModelId ? (
              <StatusPill
                label={
                  switching?.targetModelId === model.id
                    ? `Switching · ${switching.stage || 'starting'}`
                    : snapshot.health.model &&
                        snapshot.lifecycle.identityMatches &&
                        snapshot.runtime.selectedModelId === model.id
                      ? 'Selected · loaded'
                      : 'Selected'
                }
                ok={model.installed}
              />
            ) : (
              <Button
                className="h-8 text-xs"
                disabled={!model.installed || switchingActive}
                onClick={() => void onSelect(model.id)}
                size="sm"
                title={switchingActive ? 'Another model switch owns the workstation lifecycle' : undefined}
                variant="outline"
              >
                Select
              </Button>
            )}
            {model.userManaged && (
              <Button className="size-8 p-0" onClick={() => void onRemove(model.id)} size="sm" variant="ghost">
                <IconTrash className="size-3.5" />
                <span className="sr-only">Remove {model.displayName}</span>
              </Button>
            )}
          </div>
        ))}
      </div>
      <div className="border-t border-(--ui-stroke-secondary) p-3">
        <Button className="h-8 gap-1.5 text-xs" onClick={() => void importModel()} size="sm" variant="outline">
          <IconPlus className="size-3.5" />
          Register GGUF
        </Button>
        <p className="mt-2 text-[0.6875rem] text-(--ui-text-tertiary)">
          Registration never copies or deletes weights. The selected file can live anywhere you control.
        </p>
      </div>
    </Surface>
  )
}

function RuntimeSettings({
  onSave,
  snapshot
}: {
  onSave: (settings: Partial<LocalWorkstationSettings>) => Promise<void>
  snapshot: LocalWorkstationSnapshot
}) {
  const [draft, setDraft] = useState(() => structuredClone(snapshot.settings))
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!dirty) {
      setDraft(structuredClone(snapshot.settings))
    }
  }, [dirty, snapshot.settings])

  const updateDraft = (next: LocalWorkstationSettings) => {
    setDraft(next)
    setDirty(true)
  }

  return (
    <Surface title="Runtime and network">
      <div className="grid gap-3 p-4 sm:grid-cols-2">
        <label className="space-y-1.5 text-xs" htmlFor="runtime-acceleration">
          <span className="font-medium text-(--ui-text-secondary)">Acceleration</span>
          <Select
            onValueChange={value =>
              updateDraft({
                ...draft,
                runtime: { ...draft.runtime, acceleration: value as 'auto' | 'cpu' | 'cuda' }
              })
            }
            value={draft.runtime.acceleration}
          >
            <SelectTrigger id="runtime-acceleration">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="auto">Auto detect</SelectItem>
              <SelectItem value="cuda">NVIDIA CUDA</SelectItem>
              <SelectItem value="cpu">CPU only</SelectItem>
            </SelectContent>
          </Select>
        </label>
        <label className="space-y-1.5 text-xs" htmlFor="runtime-listen-address">
          <span className="font-medium text-(--ui-text-secondary)">Listen address</span>
          <Select
            onValueChange={value => updateDraft({ ...draft, network: { ...draft.network, host: value } })}
            value={draft.network.host}
          >
            <SelectTrigger className="font-mono" id="runtime-listen-address">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem className="font-mono" value="127.0.0.1">
                127.0.0.1 (IPv4 loopback)
              </SelectItem>
              <SelectItem className="font-mono" value="::1">
                ::1 (IPv6 loopback)
              </SelectItem>
            </SelectContent>
          </Select>
        </label>
        {(
          [
            ['Model API port', 'modelPort'],
            ['Hermes/dashboard port', 'hermesPort']
          ] as const
        ).map(([label, key]) => (
          <label className="space-y-1.5 text-xs" key={key}>
            <span className="font-medium text-(--ui-text-secondary)">{label}</span>
            <input
              className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 font-mono outline-none focus:border-(--ui-accent)"
              max={65535}
              min={1024}
              onChange={event =>
                updateDraft({ ...draft, network: { ...draft.network, [key]: Number(event.currentTarget.value) } })
              }
              type="number"
              value={draft.network[key]}
            />
          </label>
        ))}
        <label className="space-y-1.5 text-xs">
          <span className="font-medium text-(--ui-text-secondary)">Build workers</span>
          <input
            className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 font-mono outline-none focus:border-(--ui-accent)"
            onChange={event => {
              const value = event.currentTarget.value.trim().toLocaleLowerCase()

              updateDraft({
                ...draft,
                runtime: {
                  ...draft.runtime,
                  buildParallelism: value === '' || value === 'auto' ? 'auto' : Number(value)
                }
              })
            }}
            placeholder="auto"
            value={draft.runtime.buildParallelism}
          />
        </label>
        <label className="space-y-1.5 text-xs">
          <span className="font-medium text-(--ui-text-secondary)">CUDA architecture</span>
          <input
            className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 font-mono outline-none focus:border-(--ui-accent)"
            onChange={event =>
              updateDraft({
                ...draft,
                runtime: { ...draft.runtime, cudaArchitecture: event.currentTarget.value.trim() || 'auto' }
              })
            }
            placeholder="auto"
            value={draft.runtime.cudaArchitecture}
          />
        </label>
        <label className="space-y-1.5 text-xs">
          <span className="font-medium text-(--ui-text-secondary)">Python version</span>
          <input
            className="h-8 w-full rounded-md border border-(--ui-stroke-secondary) bg-(--ui-control-background) px-2 font-mono outline-none focus:border-(--ui-accent)"
            onChange={event =>
              updateDraft({ ...draft, runtime: { ...draft.runtime, pythonVersion: event.currentTarget.value } })
            }
            placeholder="3.13"
            value={draft.runtime.pythonVersion}
          />
        </label>
      </div>
      <div className="flex flex-wrap items-center gap-3 border-t border-(--ui-stroke-secondary) px-4 py-3">
        <button
          aria-pressed={draft.runtime.verifyModelOnStart}
          className={cn(
            'rounded-full border px-2.5 py-1 text-[0.6875rem] font-medium',
            draft.runtime.verifyModelOnStart
              ? 'border-(--ui-accent)/40 bg-(--ui-accent)/10 text-(--ui-accent)'
              : 'border-(--ui-stroke-secondary) text-(--ui-text-tertiary)'
          )}
          onClick={() =>
            updateDraft({
              ...draft,
              runtime: { ...draft.runtime, verifyModelOnStart: !draft.runtime.verifyModelOnStart }
            })
          }
          type="button"
        >
          Verify model on start
        </button>
        <p className="min-w-0 flex-1 text-[0.6875rem] text-(--ui-text-tertiary)">
          Auto detected {snapshot.settings.autoTuning.logicalProcessors} logical CPUs
          {snapshot.settings.autoTuning.vramMiB
            ? ` and ${snapshot.settings.autoTuning.vramMiB.toLocaleString()} MiB VRAM`
            : ''}
          . Restart the stack after changing these settings.
        </p>
        <Button
          className="h-8 gap-1.5 text-xs"
          disabled={saving}
          onClick={() => {
            setSaving(true)
            void onSave(draft)
              .then(() => setDirty(false))
              .catch(() => undefined)
              .finally(() => setSaving(false))
          }}
          size="sm"
        >
          {saving && <IconLoader2 className="size-3.5 animate-spin" />}
          Save settings
        </Button>
      </div>
    </Surface>
  )
}

function LogViewer() {
  const [name, setName] = useState<LocalLog>('supervisor')
  const [content, setContent] = useState('')
  const [error, setError] = useState('')
  const [filePath, setFilePath] = useState('')
  const [refreshVersion, setRefreshVersion] = useState(0)

  useEffect(() => {
    let active = true
    let latestRequest = 0

    const load = async () => {
      const request = ++latestRequest

      try {
        const result = await window.hermesDesktop.localWorkstation.logs(name, 500)

        if (!active || request !== latestRequest) {
          return
        }

        setContent(result.content)
        setFilePath(result.path)
        setError('')
      } catch (nextError) {
        if (active && request === latestRequest) {
          setError(nextError instanceof Error ? nextError.message : String(nextError))
        }
      }
    }

    void load()
    const timer = window.setInterval(() => void load(), 3000)

    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [name, refreshVersion])

  return (
    <Surface className="overflow-hidden">
      <header className="flex items-center gap-2 border-b border-(--ui-stroke-secondary) px-3 py-2">
        <Select onValueChange={value => setName(value as LocalLog)} value={name}>
          <SelectTrigger aria-label="Log source" className="w-auto min-w-28" size="sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent align="start">
            {LOG_NAMES.map(item => (
              <SelectItem key={item} value={item}>
                {item[0].toUpperCase()}
                {item.slice(1)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span
          className={cn(
            'min-w-0 flex-1 truncate font-mono text-[0.6875rem]',
            error ? 'text-red-500' : 'text-(--ui-text-tertiary)'
          )}
          role={error ? 'alert' : undefined}
        >
          {error || filePath}
        </span>
        <Button
          className="size-8 p-0"
          onClick={() => setRefreshVersion(current => current + 1)}
          size="sm"
          variant="ghost"
        >
          <IconRefresh className="size-3.5" />
          <span className="sr-only">Refresh logs</span>
        </Button>
      </header>
      <pre className="h-[min(64vh,44rem)] overflow-auto bg-[#111315] p-4 font-mono text-[0.71875rem] leading-5 text-[#d9dde3]">
        {content || 'No log entries yet.'}
      </pre>
    </Surface>
  )
}

function HomeContent({
  onNavigate,
  onRun,
  snapshot
}: {
  onNavigate: (path: string) => void
  onRun: (action: LocalAction, input?: Record<string, unknown>) => void
  snapshot: LocalWorkstationSnapshot
}) {
  const status = deriveLocalWorkstationStatus(snapshot)
  const stackRunning = status.stackRunning
  const memoryUsed = snapshot.hardware.memoryTotalBytes - snapshot.hardware.memoryFreeBytes
  const ramPercent = (memoryUsed / snapshot.hardware.memoryTotalBytes) * 100
  const gpuPercent = snapshot.gpu ? (snapshot.gpu.memoryUsedMiB / snapshot.gpu.memoryTotalMiB) * 100 : 0

  return (
    <div className="space-y-4">
      <Surface className="overflow-hidden">
        <div className="relative grid gap-6 p-5 md:grid-cols-[1fr_auto] md:items-center">
          <div className="absolute inset-y-0 left-0 w-1 bg-(--ui-accent)" />
          <div className="min-w-0">
            <div className="mb-2 flex flex-wrap items-center gap-2">
              <StatusPill label={status.label} ok={status.ready} />
              <span className="text-[0.6875rem] text-(--ui-text-tertiary)">
                {snapshot.runtime.profile || snapshot.profiles?.selected || 'No profile'}
              </span>
            </div>
            <h2 className="text-xl font-semibold tracking-[-0.02em]">
              {status.title}
            </h2>
            <p className="mt-1 max-w-2xl text-sm leading-6 text-(--ui-text-secondary)">
              {status.description}

            </p>
          </div>
          <div className="flex flex-wrap gap-2 md:justify-end">
            {stackRunning ? (
              <>
                <ActionButton action="restart" available={snapshot.actions.restart} onRun={onRun} tasks={snapshot.tasks}>
                  <IconRefresh className="size-3.5" />
                  Restart
                </ActionButton>
                <ActionButton
                  action="stop"
                  available={snapshot.actions.stop}
                  onRun={onRun}
                  tasks={snapshot.tasks}
                  variant="destructive"
                >
                  <IconSquare className="size-3.5" />
                  Stop stack
                </ActionButton>
              </>
            ) : (
              <ActionButton
                action="start"
                available={snapshot.actions.start}
                onRun={onRun}
                tasks={snapshot.tasks}
                variant="default"
              >
                <IconPlayerPlay className="size-3.5" />
                Start stack
              </ActionButton>
            )}
          </div>
        </div>
      </Surface>

      <div className="grid gap-4 xl:grid-cols-[1.35fr_1fr]">
        <Surface title="Services">
          <ServiceRow
            detail={`OpenAI-compatible · ${snapshot.settings.network.host}:${snapshot.settings.network.modelPort}`}
            healthy={snapshot.health.model}
            name={`${snapshot.model.displayName} server`}
            pid={snapshot.runtime.model?.pid}
          />
          <ServiceRow
            detail={`JSON-RPC / WebSocket · ${snapshot.settings.network.host}:${snapshot.settings.network.hermesPort}`}
            healthy={snapshot.health.hermes}
            name="Hermes serve"
            pid={snapshot.runtime.hermes?.pid}
          />
          <ServiceRow
            detail={`Unified Hermes web management surface · ${snapshot.settings.network.host}:${snapshot.settings.network.hermesPort}`}
            healthy={snapshot.health.dashboard}
            name="Web Dashboard"
          />
        </Surface>
        <Surface title="Resources">
          <ResourceBar
            icon={IconCpu}
            label="System memory"
            note={`${bytes(snapshot.hardware.memoryFreeBytes)} free of ${bytes(snapshot.hardware.memoryTotalBytes)}`}
            percent={ramPercent}
            value={`${Math.round(ramPercent)}%`}
          />
          <div className="border-t border-(--ui-stroke-secondary)">
            <ResourceBar
              icon={IconGauge}
              label="GPU memory"
              note={
                snapshot.gpu
                  ? `${snapshot.gpu.memoryFreeMiB.toLocaleString()} MiB free · ${snapshot.gpu.temperatureCelsius}°C`
                  : 'Accelerator telemetry unavailable'
              }
              percent={gpuPercent}
              value={snapshot.gpu ? `${snapshot.gpu.memoryUsedMiB.toLocaleString()} MiB` : '—'}
            />
          </div>
        </Surface>
      </div>

      <div className="grid gap-4 md:grid-cols-3">
        {[
          {
            detail: `Chat with ${snapshot.model.displayName} through Hermes.`,
            icon: IconRocket,
            label: 'Open Chat',
            path: '/'
          },
          {
            detail: 'Run the real keyboard-driven Hermes terminal UI.',
            icon: IconTerminal2,
            label: 'Open TUI',
            path: '/tui'
          },
          {
            detail: 'Inspect service logs without exposing local secrets.',
            icon: IconFileAnalytics,
            label: 'View Logs',
            path: '/logs'
          }
        ].map(item => (
          <button
            className="group flex min-h-28 items-start gap-3 rounded-xl border border-(--ui-stroke-secondary) bg-(--ui-panel-surface-background) p-4 text-left transition-colors hover:border-(--ui-accent)/45 hover:bg-(--ui-control-hover-background)"
            key={item.label}
            onClick={() => onNavigate(item.path)}
            type="button"
          >
            <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-(--ui-accent)/10 text-(--ui-accent)">
              <item.icon className="size-4.5" stroke={1.7} />
            </div>
            <div>
              <p className="text-sm font-semibold">{item.label}</p>
              <p className="mt-1 text-xs leading-5 text-(--ui-text-tertiary)">{item.detail}</p>
            </div>
            <IconChevronRight className="ml-auto mt-1 size-4 text-(--ui-text-tertiary) transition-transform group-hover:translate-x-0.5" />
          </button>
        ))}
      </div>

      <Surface title="Integrity and provenance">
        <div className="grid divide-y divide-(--ui-stroke-secondary) sm:grid-cols-2 sm:divide-x sm:divide-y-0 xl:grid-cols-4">
          {[
            ['Model SHA-256', snapshot.model?.sha256 ? `${snapshot.model.sha256.slice(0, 16)}…` : 'Unavailable'],
            ['llama.cpp', shortCommit(snapshot.version?.sources.llamaCpp.commit)],
            ['Hermes Agent', shortCommit(snapshot.version?.sources.hermesAgent.commit)],
            ['Local authentication', 'DPAPI · per-user']
          ].map(([label, value]) => (
            <div className="px-4 py-3" key={label}>
              <p className="text-[0.6875rem] font-medium text-(--ui-text-tertiary)">{label}</p>
              <p className="mt-1 truncate font-mono text-xs font-semibold">{value}</p>
            </div>
          ))}
        </div>
      </Surface>
    </div>
  )
}

function DashboardContent({ snapshot }: { snapshot: LocalWorkstationSnapshot }) {
  const dashboard = window.hermesDesktop.localWorkstation.dashboard
  const hostRef = useRef<HTMLDivElement>(null)

  const [state, setState] = useState<HermesLocalDashboardState>({
    canRetry: true,
    message: dashboard
      ? 'Preparing the protected loopback dashboard.'
      : 'Embedded dashboard controls are unavailable in this Desktop build.',
    origin: '',
    phase: dashboard ? 'loading' : 'offline',
    retryCount: 0,
    visible: false
  })

  const readBounds = useCallback((): HermesLocalDashboardBounds | null => {
    const element = hostRef.current

    if (!element) {
      return null
    }

    const rect = element.getBoundingClientRect()

    return {
      height: Math.max(1, Math.round(rect.height)),
      width: Math.max(1, Math.round(rect.width)),
      x: Math.max(0, Math.round(rect.left)),
      y: Math.max(0, Math.round(rect.top))
    }
  }, [])

  useEffect(() => {
    if (!dashboard) {
      return
    }

    const switchingModel = snapshot.lifecycle.switchingModel

    if (switchingModel) {
      void dashboard.hide().catch(() => undefined)
      setState(current => ({
        ...current,
        canRetry: false,
        message: `Model switch: ${switchingModel.stage || 'starting'}`,
        phase: 'restarting',
        visible: false
      }))

      return
    }

    let active = true
    let frame = 0

    const unsubscribe = dashboard.onState(next => {
      if (active) {
        setState(next)
      }
    })

    const resize = () => {
      window.cancelAnimationFrame(frame)
      frame = window.requestAnimationFrame(() => {
        const bounds = readBounds()

        if (bounds) {
          void dashboard.resize(bounds).catch(() => undefined)
        }
      })
    }

    const observer = new ResizeObserver(resize)

    if (hostRef.current) {
      observer.observe(hostRef.current)
    }

    window.addEventListener('resize', resize)
    window.addEventListener('scroll', resize, true)

    const bounds = readBounds()

    if (bounds) {
      void dashboard.show(bounds).then(next => active && setState(next)).catch(error => {
        if (active) {
          setState(current => ({
            ...current,
            canRetry: true,
            message: error instanceof Error ? error.message : String(error),
            phase: 'offline',
            visible: false
          }))
        }
      })
    }

    return () => {
      active = false
      unsubscribe()
      observer.disconnect()
      window.cancelAnimationFrame(frame)
      window.removeEventListener('resize', resize)
      window.removeEventListener('scroll', resize, true)
      void dashboard.hide().catch(() => undefined)
    }
  }, [
    dashboard,
    readBounds,
    snapshot.lifecycle.switchingModel,
    snapshot.settings.network.hermesPort,
    snapshot.settings.network.host
  ])

  const phaseCopy = {
    authentication: ['Authentication required', IconKey],
    loading: ['Loading dashboard', IconLoader2],
    offline: ['Dashboard offline', IconAlertTriangle],
    ready: ['Dashboard connected', IconCheck],
    restarting: ['Dashboard restarting', IconRefresh]
  } as const

  const [phaseLabel, PhaseIcon] = phaseCopy[state.phase]
  const loading = state.phase === 'loading' || state.phase === 'restarting'

  return (
    <div className="space-y-4">
      <Surface>
        <div className="flex flex-col gap-4 p-4 lg:flex-row lg:items-center">
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <StatusPill label={phaseLabel} ok={state.phase === 'ready'} />
              {state.origin && (
                <span className="truncate font-mono text-[0.6875rem] text-(--ui-text-tertiary)">
                  {state.origin}
                </span>
              )}
            </div>
            <h3 className="mt-3 text-base font-semibold">Hermes Web Dashboard</h3>
            <p className="mt-1 max-w-3xl text-sm leading-6 text-(--ui-text-secondary)">
              Embedded from the active loopback configuration with an isolated sandboxed renderer. Authentication is
              attached in Electron and is never exposed to this page or the Desktop renderer.
            </p>
          </div>
          <div className="flex shrink-0 flex-wrap gap-2">
            <Button
              className="gap-2"
              disabled={!dashboard || !state.canRetry}
              onClick={() => void dashboard?.reload().then(setState).catch(() => undefined)}
              variant="outline"
            >
              <IconRefresh className={cn('size-4', loading && 'animate-spin motion-reduce:animate-none')} />
              Retry
            </Button>
            <Button
              className="gap-2"
              disabled={!snapshot.health.dashboard}
              onClick={() => void window.hermesDesktop.localWorkstation.openDashboard()}
              variant="outline"
            >
              <IconExternalLink className="size-4" />
              Open externally
            </Button>
          </div>
        </div>
      </Surface>

      <Surface className="overflow-hidden">
        <div
          className="relative h-[min(68vh,52rem)] min-h-[28rem] bg-[#111315]"
          data-testid="dashboard-embed-host"
          ref={hostRef}
        >
          {state.phase !== 'ready' && (
            <div className="absolute inset-0 grid place-items-center p-6 text-center">
              <div className="max-w-md">
                <PhaseIcon
                  className={cn(
                    'mx-auto size-7 text-(--ui-accent)',
                    loading && 'animate-spin motion-reduce:animate-none'
                  )}
                  stroke={1.7}
                />
                <h4 className="mt-4 text-sm font-semibold">{phaseLabel}</h4>
                <p className="mt-2 text-xs leading-5 text-(--ui-text-secondary)">{state.message}</p>
                {state.retryCount > 0 && (
                  <p className="mt-2 text-[0.6875rem] text-(--ui-text-tertiary)">
                    Automatic reconnect attempt {state.retryCount}
                  </p>
                )}
              </div>
            </div>
          )}
        </div>
      </Surface>
    </div>
  )
}

function RestoreContent({
  onCancelTask,
  onNavigate,
  onRun,
  snapshot
}: {
  onCancelTask: (taskId: string) => Promise<void>
  onNavigate: (path: string) => void
  onRun: (action: LocalAction, input?: Record<string, unknown>) => void
  snapshot: LocalWorkstationSnapshot
}) {
  const [selectedBackupId, setSelectedBackupId] = useState(snapshot.backups[0]?.id || '')

  useEffect(() => {
    if (!snapshot.backups.some(backup => backup.id === selectedBackupId)) {
      setSelectedBackupId(snapshot.backups[0]?.id || '')
    }
  }, [selectedBackupId, snapshot.backups])

  const selectedBackup = snapshot.backups.find(backup => backup.id === selectedBackupId) || snapshot.backups[0]
  const restoreTasks = snapshot.tasks.filter(task => task.action === 'restore')
  const activeTask = restoreTasks.find(task => ['queued', 'running', 'cancelling'].includes(task.status))
  const latestTask = activeTask || restoreTasks.at(-1)
  const progress = latestTask?.progress
  const progressText =
    progress?.mode === 'determinate' && progress.completedUnits !== null && progress.totalUnits !== null
      ? `${progress.completedUnits}/${progress.totalUnits} · ${progress.percent ?? 0}%`
      : progress?.message || latestTask?.stage || latestTask?.status || 'Ready'

  return (
    <div className="grid gap-4 xl:grid-cols-[1.1fr_0.9fr]">
      <Surface title="Managed backups">
        <div className="space-y-4 p-4">
          <div>
            <p className="text-sm font-semibold">Choose a verified Hermes Local backup</p>
            <p className="mt-1 text-xs leading-5 text-(--ui-text-secondary)">
              Restore validates the archive, creates a safety snapshot, stops owned services and rolls back automatically if validation fails.
            </p>
          </div>
          {snapshot.backups.length ? (
            <Select onValueChange={setSelectedBackupId} value={selectedBackup?.id || ''}>
              <SelectTrigger>
                <SelectValue placeholder="Select a backup" />
              </SelectTrigger>
              <SelectContent>
                {snapshot.backups.map((backup: LocalBackup) => (
                  <SelectItem key={backup.id} value={backup.id}>
                    {backup.name} · {bytes(backup.sizeBytes)}{backup.verified ? '' : ' · unverified'}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <div className="rounded-lg border border-dashed border-(--ui-stroke-secondary) p-4 text-xs text-(--ui-text-secondary)">
              No managed backups are available. Create one from Services first.
            </div>
          )}
          {selectedBackup && (
            <div className="grid gap-2 rounded-lg border border-(--ui-stroke-secondary) p-3 text-xs sm:grid-cols-2">
              <div><span className="text-(--ui-text-tertiary)">Identity</span><p className="mt-1 font-mono">{selectedBackup.id}</p></div>
              <div><span className="text-(--ui-text-tertiary)">Created</span><p className="mt-1">{new Date(selectedBackup.modifiedAt).toLocaleString()}</p></div>
              <div className="sm:col-span-2"><span className="text-(--ui-text-tertiary)">Path</span><p className="mt-1 truncate font-mono">{selectedBackup.path}</p></div>
            </div>
          )}
          <Button
            className="gap-2"
            disabled={!selectedBackup || !selectedBackup.verified || Boolean(activeTask) || !snapshot.actions.restore}
            onClick={() => selectedBackup && onRun('restore', {
              backupId: selectedBackup.id,
              backupPath: selectedBackup.path,
              verifyIntegrity: true
            })}
            variant="destructive"
          >
            <IconRestore className="size-4" />
            Restore selected backup
          </Button>
        </div>
      </Surface>
      <Surface title="Durable restore task">
        {latestTask ? (
          <div className="space-y-4 p-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-sm font-semibold">{latestTask.stage || latestTask.status}</p>
                <p className="mt-1 font-mono text-[0.6875rem] text-(--ui-text-tertiary)">{latestTask.id}</p>
              </div>
              <StatusPill label={latestTask.status} ok={latestTask.status === 'succeeded'} />
            </div>
            <div>
              <div className="flex justify-between text-xs text-(--ui-text-secondary)">
                <span>{progress?.message || latestTask.stage || 'Restore task state'}</span>
                <span>{progressText}</span>
              </div>
              {progress?.mode === 'determinate' && progress.percent !== null && (
                <div className="mt-2 h-2 overflow-hidden rounded-full bg-(--ui-stroke-secondary)">
                  <div className="h-full bg-(--ui-accent)" style={{ width: `${Math.min(100, Math.max(0, progress.percent))}%` }} />
                </div>
              )}
            </div>
            <div className="rounded-lg border border-(--ui-stroke-secondary) p-3 text-xs">
              <p><span className="text-(--ui-text-tertiary)">Backup</span> <span className="font-mono">{latestTask.context?.backupPath || 'Unknown'}</span></p>
              <p className="mt-2"><span className="text-(--ui-text-tertiary)">Result</span> <span className="font-mono">{latestTask.result?.path || 'Pending'}</span></p>
            </div>
            <div className="flex flex-wrap gap-2">
              {latestTask.capabilities.cancel && (
                <Button onClick={() => void onCancelTask(latestTask.id)} variant="outline">
                  Request safe cancellation
                </Button>
              )}
              <Button onClick={() => onNavigate('/tasks')} variant="outline">
                Open in Task Centre
              </Button>
            </div>
          </div>
        ) : (
          <div className="p-5 text-sm text-(--ui-text-secondary)">
            No restore has been run from this workstation yet.
          </div>
        )}
      </Surface>
    </div>
  )
}

function SectionContent({
  onCancelTask,
  onCreateProfile,
  onDeleteProfile,
  onNavigate,
  onRefresh,
  onRegisterModel,
  onRemoveModel,
  onRun,
  onSaveProfile,
  onSaveSettings,
  onSetLaunchAtLogin,
  onTaskError,
  onSelectModel,
  onSelectProfile,
  section,
  selectedProfile,
  snapshot
}: {
  onCancelTask: (taskId: string) => Promise<void>
  onCreateProfile: (profile: LocalInferenceProfile) => Promise<void>
  onDeleteProfile: (name: string) => Promise<void>
  onNavigate: (path: string) => void
  onRefresh: () => void
  onRegisterModel: (model: Partial<LocalModel> & { localPath: string }) => Promise<void>
  onRemoveModel: (id: string) => Promise<void>
  onRun: (action: LocalAction, input?: Record<string, unknown>) => void
  onSaveProfile: (profile: LocalInferenceProfile, originalName: string) => Promise<void>
  onSaveSettings: (settings: Partial<LocalWorkstationSettings>) => Promise<void>
  onSetLaunchAtLogin: (enabled: boolean) => Promise<void>
  onTaskError: (message: string) => void
  onSelectModel: (id: string) => Promise<void>
  onSelectProfile: (name: string) => Promise<void>
  section: Section
  selectedProfile: string
  snapshot: LocalWorkstationSnapshot
}) {
  if (section === 'home') {
    return <HomeContent onNavigate={onNavigate} onRun={onRun} snapshot={snapshot} />
  }

  if (section === 'restore') {
    return <RestoreContent onCancelTask={onCancelTask} onNavigate={onNavigate} onRun={onRun} snapshot={snapshot} />
  }

  if (section === 'sessions' || section === 'projects') {
    const isSessions = section === 'sessions'

    return (
      <div className="grid gap-4 md:grid-cols-[1.2fr_0.8fr]">
        <Surface>
          <div className="p-5">
            <div className="grid size-10 place-items-center rounded-xl bg-(--ui-accent)/10 text-(--ui-accent)">
              {isSessions ? (
                <IconDatabase className="size-5" stroke={1.7} />
              ) : (
                <IconFolder className="size-5" stroke={1.7} />
              )}
            </div>
            <h3 className="mt-4 text-base font-semibold">
              {isSessions ? 'Persistent local sessions' : 'Local project workspaces'}
            </h3>
            <p className="mt-1 max-w-2xl text-sm leading-6 text-(--ui-text-secondary)">
              {isSessions
                ? 'Open the official Hermes chat workspace to search, resume, pin and organise conversations stored on this workstation.'
                : 'Open the official Hermes workspace sidebar to create projects, group repositories and manage isolated worktrees.'}
            </p>
            <Button className="mt-5 gap-2" onClick={() => onNavigate('/')}>
              {isSessions ? <IconRocket className="size-4" /> : <IconFolder className="size-4" />}
              {isSessions ? 'Open session workspace' : 'Open project workspace'}
            </Button>
          </div>
        </Surface>
        <Surface title="Local storage">
          <div className="divide-y divide-(--ui-stroke-secondary)">
            <div className="px-4 py-3">
              <p className="text-[0.6875rem] font-medium text-(--ui-text-tertiary)">Hermes state database</p>
              <p className="mt-1 font-mono text-xs font-semibold">{bytes(snapshot.storage.stateDatabaseBytes)}</p>
            </div>
            <div className="px-4 py-3">
              <p className="text-[0.6875rem] font-medium text-(--ui-text-tertiary)">Data boundary</p>
              <p className="mt-1 truncate font-mono text-xs font-semibold">{snapshot.root}\data</p>
            </div>
          </div>
        </Surface>
      </div>
    )
  }

  if (section === 'services') {
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
        <Surface>
          <ServiceRow
            detail={`Authenticated OpenAI-compatible inference · ${snapshot.settings.network.host}:${snapshot.settings.network.modelPort}`}
            healthy={snapshot.health.model}
            name={`${snapshot.model.displayName} server`}
            pid={snapshot.runtime.model?.pid}
          />
          <ServiceRow
            detail={`Desktop JSON-RPC and WebSocket backend · ${snapshot.settings.network.host}:${snapshot.settings.network.hermesPort}`}
            healthy={snapshot.health.hermes}
            name="Hermes serve"
            pid={snapshot.runtime.hermes?.pid}
          />
          <ServiceRow
            detail="Persistent Windows Job Object owner"
            healthy={snapshot.runtime.controllerAlive}
            name="Stack supervisor"
            pid={snapshot.runtime.controllerPid}
          />
        </Surface>
        <Surface>
          <div className="flex flex-wrap gap-2 p-4">
            <ActionButton action="start" available={snapshot.actions.start} onRun={onRun} tasks={snapshot.tasks}>
              <IconPlayerPlay className="size-3.5" /> Start
            </ActionButton>
            <ActionButton action="restart" available={snapshot.actions.restart} onRun={onRun} tasks={snapshot.tasks}>
              <IconRefresh className="size-3.5" /> Restart
            </ActionButton>
            <ActionButton
              action="stop"
              available={snapshot.actions.stop}
              onRun={onRun}
              tasks={snapshot.tasks}
              variant="destructive"
            >
              <IconSquare className="size-3.5" /> Stop
            </ActionButton>
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
                  ['Harness commit', shortCommit(snapshot.updates.installed.harnessCommit)],
                  ['Harness tree', shortCommit(snapshot.updates.installed.harnessTree)],
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

  if (section === 'models') {
    const model = snapshot.model

    return (
      <div className="space-y-4">
        <div className="grid gap-4 xl:grid-cols-[1.2fr_1fr]">
          <Surface title="Selected model">
            <div className="p-5">
              <div className="flex items-start gap-3">
                <div className="grid size-10 place-items-center rounded-xl bg-(--ui-accent)/10 text-(--ui-accent)">
                  <IconBrain className="size-5" stroke={1.7} />
                </div>
                <div className="min-w-0">
                  <h3 className="text-base font-semibold">{model.displayName}</h3>
                  <p className="mt-0.5 text-xs text-(--ui-text-tertiary)">
                    {model.metadata.quantization || 'GGUF'} ·{' '}
                    {model.metadata.nativeToolCalling ? 'native tools' : 'chat'}
                    {model.metadata.reasoning ? ` · ${model.metadata.reasoning} reasoning` : ''}
                  </p>
                </div>
                <div className="ml-auto">
                  <StatusPill
                    label={
                      snapshot.lifecycle.switchingModel
                        ? `Switching · ${snapshot.lifecycle.switchingModel.stage || 'starting'}`
                        : snapshot.health.model &&
                            snapshot.lifecycle.identityMatches &&
                            snapshot.runtime.selectedModelId === model.id
                          ? 'Loaded'
                          : model.installed
                          ? 'On disk'
                          : 'File missing'
                    }
                    ok={model.installed}
                  />
                </div>
              </div>
              <dl className="mt-5 grid gap-x-5 gap-y-4 sm:grid-cols-2">
                {[
                  ['File', model.filename],
                  ['Size', bytes(model.actualSizeBytes || model.sizeBytes || 0)],
                  [
                    'Selected context',
                    `${snapshot.profiles?.profiles.find(profile => profile.name === snapshot.settings.selectedProfile)?.contextTokens.toLocaleString() || '—'} tokens`
                  ],
                  [
                    'Model maximum',
                    model.metadata.modelMaximumContextTokens
                      ? `${model.metadata.modelMaximumContextTokens.toLocaleString()} tokens`
                      : 'Not declared'
                  ],
                  ['Alias', model.alias],
                  ['Revision', model.revision ? shortCommit(model.revision) : null]
                ].map(([label, value]) => (
                  <div key={label}>
                    <dt className="text-[0.6875rem] font-medium text-(--ui-text-tertiary)">{label}</dt>
                    <dd className="mt-1 truncate font-mono text-xs">{value || 'Unavailable'}</dd>
                  </div>
                ))}
              </dl>
            </div>
          </Surface>
          <Surface title="Runtime">
            <div className="divide-y divide-(--ui-stroke-secondary)">
              {[
                ['Acceleration', snapshot.settings.runtime.acceleration, IconGauge],
                ['CPU', snapshot.hardware.cpu, IconCpu],
                ['Authentication', 'Bearer token · DPAPI protected', IconKey],
                [
                  'Endpoint',
                  `http://${snapshot.settings.network.host}:${snapshot.settings.network.modelPort}/v1`,
                  IconServer2
                ]
              ].map(([label, value, Icon]) => (
                <div className="flex items-center gap-3 px-4 py-3" key={String(label)}>
                  <Icon className="size-4 text-(--ui-accent)" stroke={1.7} />
                  <span className="text-xs font-medium">{label as string}</span>
                  <span className="ml-auto max-w-[65%] truncate text-right text-xs text-(--ui-text-tertiary)">
                    {value as string}
                  </span>
                </div>
              ))}
            </div>
          </Surface>
        </div>
        <ModelDownloadCard
          onNavigate={onNavigate}
          onRefresh={onRefresh}
          onTaskError={onTaskError}
          tasks={snapshot.tasks}
        />
        <ModelManager
          onRegister={onRegisterModel}
          onRemove={onRemoveModel}
          onSelect={onSelectModel}
          snapshot={snapshot}
        />
        <RuntimeSettings onSave={onSaveSettings} snapshot={snapshot} />
      </div>
    )
  }

  if (section === 'local-profiles') {
    return (
      <Surface className="overflow-hidden">
        <ProfileEditor
          onCreate={onCreateProfile}
          onDelete={onDeleteProfile}
          onSave={onSaveProfile}
          onSelect={onSelectProfile}
          selected={selectedProfile}
          snapshot={snapshot}
        />
      </Surface>
    )
  }

  if (section === 'logs') {
    return <LogViewer />
  }

  if (section === 'dashboard') {
    return <DashboardContent snapshot={snapshot} />
  }

  if (section === 'tasks') {
    return (
      <TaskCentre
        modelName={snapshot.model.displayName}
        onCancel={async taskId => {
          await window.hermesDesktop.localWorkstation.cancelAction(taskId)
          onRefresh()
        }}
        onError={onTaskError}
        onNavigate={onNavigate}
        onOpenResult={async taskId => {
          await window.hermesDesktop.localWorkstation.openActionResult(taskId)
        }}
        onPause={async taskId => {
          await window.hermesDesktop.localWorkstation.pauseAction(taskId)
          onRefresh()
        }}
        onResume={async taskId => {
          await window.hermesDesktop.localWorkstation.resumeAction(taskId)
          onRefresh()
        }}
        onRetry={async taskId => {
          await window.hermesDesktop.localWorkstation.retryAction(taskId)
          onRefresh()
        }}
        profileName={snapshot.runtime.profile || snapshot.profiles?.selected || ''}
        tasks={snapshot.tasks}
      />
    )
  }

  if (section === 'security') {
    const scanTask = latestSecurityTask(snapshot.tasks)
    const scanActive =
      scanTask?.status === 'queued' || scanTask?.status === 'running' || scanTask?.status === 'cancelling'
    const progress = scanTask?.progress
    const percent =
      progress?.mode === 'determinate' && progress.percent !== null
        ? Math.min(100, Math.max(0, progress.percent))
        : null
    const counters = progress?.counters || {}

    return (
      <div className="grid gap-4 xl:grid-cols-[1.15fr_0.85fr]">
        <Surface title="Security scan">
          <div className="p-5">
            <div className="flex flex-wrap items-start gap-4">
              <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-emerald-500/10 text-emerald-500">
                <IconShieldCheck className="size-5" />
              </div>
              <div className="min-w-0 flex-1">
                <h3 className="text-base font-semibold">Repository security workflow</h3>
                <p className="mt-1 text-sm leading-6 text-(--ui-text-secondary)">
                  Dependency, secret, static-analysis and packaged-distribution checks publish one durable task shared
                  with Task Centre. Scanner output and result paths are redacted before display.
                </p>
              </div>
              <StatusPill
                label={scanTask ? securityTaskState(scanTask) : snapshot.reports.security ? 'Latest report available' : 'Not run'}
                ok={scanTask?.status === 'succeeded' || (!scanTask && snapshot.reports.security)}
              />
            </div>

            {scanTask && (
              <div className="mt-5 rounded-xl border border-(--ui-stroke-secondary) bg-(--ui-control-background) p-4">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <span className="text-xs font-semibold">{securityTaskState(scanTask)}</span>
                  <span className="font-mono text-[0.6875rem] text-(--ui-text-tertiary)">Task {scanTask.id}</span>
                  <span className="ml-auto font-mono text-[0.6875rem] text-(--ui-text-tertiary)">
                    {scanTask.stage || 'waiting'}
                  </span>
                </div>
                <div
                  aria-label={percent === null ? 'Security scan progress is indeterminate' : `Security scan progress ${percent}%`}
                  aria-valuemax={100}
                  aria-valuemin={0}
                  aria-valuenow={percent ?? undefined}
                  className="mt-3 h-1.5 overflow-hidden rounded-full bg-(--ui-stroke-secondary)"
                  role="progressbar"
                >
                  <div
                    className={cn(
                      'h-full rounded-full bg-emerald-500 transition-[width]',
                      scanActive && percent === null && 'w-1/3 animate-pulse motion-reduce:animate-none',
                      !scanActive && scanTask.status === 'succeeded' && 'w-full',
                      !scanActive &&
                        (scanTask.status === 'failed' || scanTask.status === 'interrupted') &&
                        'w-full bg-red-500',
                      !scanActive && scanTask.status === 'cancelled' && 'w-full bg-amber-500'
                    )}
                    style={scanActive && percent !== null ? { width: `${percent}%` } : undefined}
                  />
                </div>
                <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 font-mono text-[0.6875rem] text-(--ui-text-tertiary)">
                  {progress?.completedUnits !== null && progress?.completedUnits !== undefined &&
                    progress.totalUnits !== null && (
                      <span>checks {progress.completedUnits}/{progress.totalUnits}</span>
                    )}
                  {Object.entries(counters).map(([key, value]) => (
                    <span key={key}>{key} {value}</span>
                  ))}
                  {progress?.mode === 'indeterminate' && <span>indeterminate</span>}
                </div>
                {progress?.message && <p className="mt-2 text-xs text-(--ui-text-secondary)">{progress.message}</p>}
                {scanTask.failure && <p className="mt-2 text-xs text-red-500">{scanTask.failure.message}</p>}
                {scanTask.result?.path && (
                  <p className="mt-2 truncate font-mono text-[0.6875rem] text-(--ui-text-tertiary)">
                    Results: {scanTask.result.path}
                  </p>
                )}
              </div>
            )}

            <div className="mt-5 flex flex-wrap gap-2">
              <ActionButton
                action="security"
                available={snapshot.actions.security}
                input={{ quick: true, skipDefender: true }}
                onRun={onRun}
                tasks={snapshot.tasks}
              >
                <IconBolt className="size-3.5" /> Quick scan
              </ActionButton>
              <ActionButton
                action="security"
                available={snapshot.actions.security}
                input={{ quick: false, skipDefender: false }}
                onRun={onRun}
                tasks={snapshot.tasks}
              >
                <IconShieldCheck className="size-3.5" /> Full scan
              </ActionButton>
              <Button
                className="h-8 gap-1.5 px-3 text-xs"
                disabled={!scanTask?.capabilities.cancel}
                onClick={() => {
                  if (!scanTask) {
                    return
                  }
                  void onCancelTask(scanTask.id)
                    .then(onRefresh)
                    .catch(error => onTaskError(error instanceof Error ? error.message : String(error)))
                }}
                size="sm"
                variant="outline"
              >
                <IconSquare className="size-3.5" /> Cancel scan
              </Button>
              <Button
                className="h-8 gap-1.5 px-3 text-xs"
                onClick={() => onNavigate('/tasks')}
                size="sm"
                variant="outline"
              >
                <IconExternalLink className="size-3.5" /> Open Task Centre
              </Button>
            </div>
          </div>
        </Surface>

        <Surface title="Scan phases and evidence">
          <div className="divide-y divide-(--ui-stroke-secondary)">
            {[
              ['Scope validation', 'Confirms the local source, runtime and trusted target boundary.'],
              ['Discovery and crawling', 'Audits dependencies and scans production source for credential patterns.'],
              ['Passive and active checks', 'Runs static analysis, SBOM, licence and optional Defender checks.'],
              ['Validation and reporting', 'Writes summary.json, findings.json, task.log and per-tool evidence.']
            ].map(([label, description]) => (
              <div className="px-4 py-3" key={label}>
                <div className="flex items-center gap-2 text-xs font-medium">
                  <IconCheck className="size-3.5 text-emerald-500" />
                  {label}
                </div>
                <p className="mt-1 pl-5 text-[0.6875rem] leading-5 text-(--ui-text-tertiary)">{description}</p>
              </div>
            ))}
          </div>
        </Surface>
      </div>
    )
  }

  if (section === 'benchmarks') {
    return (
      <div className="grid gap-4 md:grid-cols-2">
        <Surface>
          <div className="p-5">
            <div className="mb-4 grid size-10 place-items-center rounded-xl bg-(--ui-accent)/10 text-(--ui-accent)">
              <IconGauge className="size-5" />
            </div>
            <h3 className="text-base font-semibold">Inference tuning harness</h3>
            <p className="mt-1 text-sm leading-6 text-(--ui-text-secondary)">
              Context, cache, offload, threads and batch settings are compared using measured target-machine evidence.
            </p>
            <div className="mt-4 flex items-center gap-2">
              <StatusPill
                label={snapshot.reports.benchmark ? 'Latest report available' : 'Report pending'}
                ok={snapshot.reports.benchmark}
              />
              <ActionButton action="benchmark" available={snapshot.actions.benchmark} onRun={onRun} tasks={snapshot.tasks}>
                <IconBolt className="size-3.5" /> Run benchmark
              </ActionButton>
            </div>
          </div>
        </Surface>
        <Surface title="Selection gate">
          <div className="divide-y divide-(--ui-stroke-secondary)">
            {[
              'Correct output and stable tools',
              'No page-file thrashing or CUDA OOM',
              'Quality before peak throughput',
              'Sustained generation target near 15 tok/s'
            ].map(item => (
              <div className="flex items-center gap-2 px-4 py-3 text-xs" key={item}>
                <IconCheck className="size-3.5 text-emerald-500" />
                {item}
              </div>
            ))}
          </div>
        </Surface>
      </div>
    )
  }

  if (section === 'memory') {
    return (
      <div className="grid gap-4 md:grid-cols-2">
        <Surface title="Local state">
          <div className="divide-y divide-(--ui-stroke-secondary)">
            {[
              ['Session database', bytes(snapshot.storage.stateDatabaseBytes), IconDatabase],
              ['Memory files', snapshot.storage.memoryFiles.toLocaleString(), IconBrain],
              ['Write policy', 'Approval required', IconShieldCheck],
              ['Location', `${snapshot.root}\\data`, IconFolder]
            ].map(([label, value, Icon]) => (
              <div className="flex items-center gap-3 px-4 py-3" key={String(label)}>
                <Icon className="size-4 text-(--ui-accent)" />
                <span className="text-xs font-medium">{label as string}</span>
                <span className="ml-auto max-w-[65%] truncate font-mono text-xs text-(--ui-text-tertiary)">
                  {value as string}
                </span>
              </div>
            ))}
          </div>
        </Surface>
        <Surface>
          <div className="p-5">
            <IconBrain className="size-6 text-(--ui-accent)" stroke={1.6} />
            <h3 className="mt-3 text-sm font-semibold">Memory stays explicit</h3>
            <p className="mt-1 text-xs leading-5 text-(--ui-text-secondary)">
              External memory is disabled. Hermes keeps session state locally and requires approval before durable
              memory writes, matching the workstation configuration.
            </p>
          </div>
        </Surface>
      </div>
    )
  }

  if (section === 'trust') {
    return <TrustCentre />
  }

  if (section === 'about') {
    return <AboutHub onNavigate={onNavigate} onRun={onRun} snapshot={snapshot} />
  }

  return (
    <div className="grid gap-4">
      <Surface>
        <div className="grid gap-5 p-5 md:grid-cols-[1fr_auto] md:items-center">
          <div>
            <div className="flex items-center gap-2">
              <IconBrandWindows className="size-5 text-(--ui-accent)" />
              <h3 className="text-base font-semibold">{snapshot.version?.product.name || 'Hermes Launcher'}</h3>
            </div>
            <p className="mt-2 text-sm leading-6 text-(--ui-text-secondary)">
              Windows-native local AI workstation built on the official Hermes Agent Desktop and pinned llama.cpp
              runtime.
            </p>
            <dl className="mt-4 grid gap-3 text-xs sm:grid-cols-2 xl:grid-cols-3">
              {[
                ['Version', snapshot.version?.product.version || 'Unavailable'],
                ['Upstream base', snapshot.updates.installed.baseCommit || 'Unavailable'],
                ['Harness commit', snapshot.updates.installed.harnessCommit || 'Unavailable'],
                ['Harness tree', snapshot.updates.installed.harnessTree || 'Unavailable'],
                ['Patch series', `${snapshot.updates.installed.patchCount} patches`],
                ['Project root', snapshot.root]
              ].map(([label, value]) => (
                <div key={label}>
                  <dt className="text-(--ui-text-tertiary)">{label}</dt>
                  <dd className="mt-1 truncate font-mono">{value}</dd>
                </div>
              ))}
            </dl>
          </div>
          <Button
            className="gap-2"
            onClick={() => void window.hermesDesktop.localWorkstation.openRoot()}
            variant="outline"
          >
            <IconFolder className="size-4" />
            Open project
          </Button>
        </div>
      </Surface>
      <Surface title="Windows sign-in">
        <div className="grid gap-4 p-5 md:grid-cols-[1fr_auto] md:items-center">
          <div>
            <h3 className="text-sm font-semibold">Launch Hermes at sign-in</h3>
            <p className="mt-1 text-xs leading-5 text-(--ui-text-secondary)">
              Registers this executable for the current Windows user only. No elevation or scheduled task is used.
            </p>
            <p className="mt-2 truncate font-mono text-[0.6875rem] text-(--ui-text-tertiary)">
              {snapshot.startup.executable}
            </p>
          </div>
          <Button
            aria-label={`${snapshot.startup.enabled ? 'Disable' : 'Enable'} launch at sign-in`}
            disabled={!snapshot.startup.available}
            onClick={() => void onSetLaunchAtLogin(!snapshot.startup.enabled)}
            variant={snapshot.startup.enabled ? 'default' : 'outline'}
          >
            {snapshot.startup.enabled ? 'Enabled' : 'Enable'}
          </Button>
        </div>
      </Surface>
    </div>
  )
}

export function LocalWorkstationView() {
  const location = useLocation()
  const navigate = useNavigate()
  const routeSection = location.pathname.slice(1) as Section
  const section: Section = routeSection in SECTION_COPY ? routeSection : 'home'
  const [snapshot, setSnapshot] = useState<LocalWorkstationSnapshot | null>(null)
  const [error, setError] = useState('')
  const [profileView, setProfileView] = useState('')
  const [refreshing, setRefreshing] = useState(false)
  const [refreshVersion, setRefreshVersion] = useState(0)

  const refresh = useCallback(() => {
    setRefreshing(true)
    setRefreshVersion(current => current + 1)
  }, [])

  useEffect(() => {
    let active = true
    let latestRequest = 0

    const load = async () => {
      const request = ++latestRequest

      try {
        const next = await window.hermesDesktop.localWorkstation.snapshot()

        if (!active || request !== latestRequest) {
          return
        }

        setSnapshot(next)
        setProfileView(current =>
          next.profiles?.profiles.some(profile => profile.name === current) ? current : next.profiles?.selected || ''
        )
        setError('')
        setRefreshing(false)
      } catch (nextError) {
        if (active && request === latestRequest) {
          setError(nextError instanceof Error ? nextError.message : String(nextError))
          setRefreshing(false)
        }
      }
    }

    void load()
    const timer = window.setInterval(() => void load(), 2000)

    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [refreshVersion])

  const run = useCallback(
    async (action: LocalAction, input: Record<string, unknown> = {}) => {
      if (!snapshot) {
        return
      }

      try {
        await window.hermesDesktop.localWorkstation.startAction(action, {
          profile: snapshot.profiles?.selected,
          ...input
        })
        if (action === 'update') {
          navigate('/tasks')
        }
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [navigate, refresh, snapshot]
  )

  const saveProfile = useCallback(
    async (profile: LocalInferenceProfile, originalName: string) => {
      try {
        await window.hermesDesktop.localWorkstation.saveProfile(profile, originalName)
        setProfileView(profile.name)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
        throw nextError
      }
    },
    [refresh]
  )

  const createProfile = useCallback(
    async (profile: LocalInferenceProfile) => {
      try {
        await window.hermesDesktop.localWorkstation.saveProfile(profile)
        setProfileView(profile.name)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [refresh]
  )

  const deleteProfile = useCallback(
    async (name: string) => {
      try {
        const result = await window.hermesDesktop.localWorkstation.deleteProfile(name)

        setProfileView(result.selected)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [refresh]
  )

  const selectProfile = useCallback(
    async (name: string) => {
      try {
        await window.hermesDesktop.localWorkstation.selectProfile(name)
        setProfileView(name)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [refresh]
  )

  const registerModel = useCallback(
    async (model: Partial<LocalModel> & { localPath: string }) => {
      try {
        const registered = await window.hermesDesktop.localWorkstation.registerModel(model)

        await window.hermesDesktop.localWorkstation.selectModel(registered.id)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [refresh]
  )

  const selectModel = useCallback(
    async (id: string) => {
      try {
        const result = await window.hermesDesktop.localWorkstation.selectModel(id)

        if (result.task) {
          navigate('/tasks')
        }

        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [navigate, refresh]
  )

  const removeModel = useCallback(
    async (id: string) => {
      try {
        await window.hermesDesktop.localWorkstation.removeModel(id)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [refresh]
  )

  const saveSettings = useCallback(
    async (settings: Partial<LocalWorkstationSettings>) => {
      try {
        await window.hermesDesktop.localWorkstation.saveSettings(settings)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
        throw nextError
      }
    },
    [refresh]
  )

  const setLaunchAtLogin = useCallback(
    async (enabled: boolean) => {
      try {
        await window.hermesDesktop.localWorkstation.loginItem.set(enabled)
        refresh()
      } catch (nextError) {
        setError(nextError instanceof Error ? nextError.message : String(nextError))
      }
    },
    [refresh]
  )

  const copy = SECTION_COPY[section]

  if (!snapshot) {
    return (
      <div className="grid h-full place-items-center bg-(--ui-editor-surface-background)">
        <div className="text-center">
          <IconLoader2 className="mx-auto size-5 animate-spin text-(--ui-accent) motion-reduce:animate-none" />
          <p className="mt-3 text-xs text-(--ui-text-tertiary)">Reading workstation state…</p>
          {error && <p className="mt-2 max-w-md text-xs text-red-500">{error}</p>}
        </div>
      </div>
    )
  }

  return (
    <main className="flex h-full min-h-0 flex-col overflow-hidden bg-(--ui-editor-surface-background)">
      <header className="shrink-0 border-b border-(--ui-stroke-secondary) px-5 pb-4 pt-[calc(var(--titlebar-height)+0.75rem)]">
        <div className="mx-auto flex max-w-[92rem] items-end gap-4">
          <div className="min-w-0 flex-1">
            <p className="text-[0.625rem] font-bold uppercase tracking-[0.16em] text-(--ui-accent)">Hermes Local</p>
            <h1 className="mt-1 truncate text-xl font-semibold tracking-[-0.02em]">{copy[0]}</h1>
            <p className="mt-0.5 truncate text-xs text-(--ui-text-tertiary)">{copy[1]}</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Select
              disabled={!snapshot.profiles?.profiles.length}
              onValueChange={value => void selectProfile(value)}
              value={snapshot.profiles?.selected || ''}
            >
              <SelectTrigger aria-label="Inference profile" className="w-44 font-medium" size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent align="end">
                {snapshot.profiles?.profiles.map(profile => (
                  <SelectItem key={profile.name} value={profile.name}>
                    {profile.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button className="size-8 p-0" disabled={refreshing} onClick={refresh} size="sm" variant="outline">
              <IconRefresh className={cn('size-3.5', refreshing && 'animate-spin motion-reduce:animate-none')} />
              <span className="sr-only">Refresh workstation</span>
            </Button>
          </div>
        </div>
      </header>
      {error && (
        <div className="flex shrink-0 items-center gap-2 border-b border-red-500/20 bg-red-500/8 px-5 py-2 text-xs text-red-500">
          <IconAlertTriangle className="size-3.5" />
          <span className="min-w-0 flex-1 truncate">{error}</span>
          <button onClick={() => setError('')} type="button">
            <IconX className="size-3.5" />
            <span className="sr-only">Dismiss</span>
          </button>
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[92rem] p-5">
          <SectionContent
            onCancelTask={async taskId => {
              await window.hermesDesktop.localWorkstation.cancelAction(taskId)
            }}
            onCreateProfile={createProfile}
            onDeleteProfile={deleteProfile}
            onNavigate={navigate}
            onRefresh={refresh}
            onRegisterModel={registerModel}
            onRemoveModel={removeModel}
            onRun={(action, input) => void run(action, input)}
            onSaveProfile={saveProfile}
            onSaveSettings={saveSettings}
            onSelectModel={selectModel}
            onSelectProfile={selectProfile}
            onSetLaunchAtLogin={setLaunchAtLogin}
            onTaskError={setError}
            section={section}
            selectedProfile={profileView}
            snapshot={snapshot}
          />
        </div>
      </div>
    </main>
  )
}
