import { IconAlertTriangle, IconExternalLink, IconFolder, IconRefresh } from '@tabler/icons-react'
import { type ReactNode, useMemo, useState } from 'react'

import { Button } from '@/components/ui/button'

import type { LocalAction, LocalWorkstationSnapshot } from './types'

interface Props {
  onNavigate: (path: string) => void
  onRun: (action: LocalAction, input?: Record<string, unknown>) => void
  snapshot: LocalWorkstationSnapshot
}

interface VersionInfo {
  product?: { channel?: string; name?: string; status?: string; version?: string }
  recordedAt?: string
  runtime?: { accelerationDefault?: string; cudaToolkit?: string; node?: string; python?: string }
  sources?: {
    hermesAgent?: { commit?: string; harnessCommit?: string; harnessTree?: string }
    llamaCpp?: { commit?: string }
  }
  starterModel?: { license?: string; revision?: string }
}

const LINKS = {
  docs: 'https://github.com/xdCloudy/Hermes-Local#readme',
  issues: 'https://github.com/xdCloudy/Hermes-Local/issues',
  project: 'https://github.com/xdCloudy/Hermes-Local',
  releases: 'https://github.com/xdCloudy/Hermes-Local/releases',
  upstream: 'https://github.com/NousResearch/hermes-agent',
  runtime: 'https://github.com/ggml-org/llama.cpp'
} as const

const PATHS = [
  ['Installation root', '.'],
  ['User data / state', 'data'],
  ['Models', 'models'],
  ['Logs', 'logs'],
  ['Reports', 'reports'],
  ['Backups', 'backups'],
  ['Configuration / profiles', 'config'],
  ['Harness source', 'source\\hermes-agent']
] as const

function short(value: null | string | undefined) {
  return value ? value.slice(0, 12) : 'Unavailable'
}

function size(value: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return '0 B'
  }
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let amount = value
  let unit = 0
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024
    unit += 1
  }
  return `${amount >= 10 ? amount.toFixed(0) : amount.toFixed(1)} ${units[unit]}`
}

function fullPath(root: string, relative: string) {
  return relative === '.' ? root : `${root.replace(/[\\/]+$/, '')}\\${relative}`
}

function Card({ children, title }: { children: ReactNode; title: string }) {
  return (
    <section className="overflow-hidden rounded-xl border border-(--ui-stroke-secondary)">
      <h3 className="border-b border-(--ui-stroke-secondary) px-4 py-3 text-xs font-semibold uppercase tracking-[0.08em] text-(--ui-text-tertiary)">
        {title}
      </h3>
      {children}
    </section>
  )
}

function Value({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="min-w-0 rounded-lg border border-(--ui-stroke-secondary) p-3">
      <dt className="text-[0.6875rem] text-(--ui-text-tertiary)">{label}</dt>
      <dd className="mt-1 break-words font-mono text-xs font-semibold">{value}</dd>
    </div>
  )
}

export function detectInstallationType(snapshot: LocalWorkstationSnapshot) {
  const executable = snapshot.startup.executable.toLowerCase()
  if (executable.includes('portable')) {
    return 'Portable'
  }
  if (executable.endsWith('\\electron.exe') || executable.includes('\\node_modules\\electron\\')) {
    return 'Development checkout'
  }
  return 'Installed'
}

export function buildSupportSummary(snapshot: LocalWorkstationSnapshot, includePaths = false) {
  const version = snapshot.version as VersionInfo | null
  const selectedProfile = snapshot.profiles?.selected || snapshot.settings.selectedProfile || 'Unavailable'
  const profile = snapshot.profiles?.profiles.find(item => item.name === selectedProfile)
  const root = includePaths ? snapshot.root : '<INSTALLATION_ROOT>'
  return [
    '# Hermes Local support summary',
    '',
    `Product: ${version?.product?.name || 'Hermes Launcher'} ${version?.product?.version || 'Unavailable'}`,
    `Channel: ${version?.product?.channel || 'development'}`,
    `Build date: ${version?.recordedAt || 'Unavailable'}`,
    `Installation type: ${detectInstallationType(snapshot)}`,
    `Agent base: ${short(snapshot.updates.installed.baseCommit)}`,
    `Harness commit: ${short(snapshot.updates.installed.harnessCommit)}`,
    `Harness tree: ${short(snapshot.updates.installed.harnessTree)}`,
    `Patch count: ${snapshot.updates.installed.patchCount}`,
    `llama.cpp: ${short(version?.sources?.llamaCpp?.commit)}`,
    `Model: ${snapshot.model?.displayName || snapshot.model?.alias || snapshot.model?.filename || 'Unavailable'}`,
    `Model revision: ${short(snapshot.model?.revision)}`,
    `Profile: ${selectedProfile}`,
    `Context: ${profile ? `${profile.contextTokens.toLocaleString()} tokens` : 'Unavailable'}`,
    `Acceleration: ${snapshot.settings.runtime.acceleration}`,
    `CPU: ${snapshot.hardware.cpu}`,
    `RAM: ${size(snapshot.hardware.memoryTotalBytes)}`,
    `GPU: ${snapshot.gpu ? `${snapshot.gpu.name} (${size(snapshot.gpu.memoryTotalMiB * 1024 ** 2)} VRAM)` : 'Unavailable'}`,
    `State database: ${size(snapshot.storage.stateDatabaseBytes)}`,
    `Memory files: ${snapshot.storage.memoryFiles}`,
    `Installation root: ${root}`,
    `Data: ${includePaths ? fullPath(snapshot.root, 'data') : '<INSTALLATION_ROOT>\\data'}`,
    `Logs: ${includePaths ? fullPath(snapshot.root, 'logs') : '<INSTALLATION_ROOT>\\logs'}`,
    '',
    'Privacy: generated locally. Tokens, API keys, credentials, private prompts, conversations, usernames and exact personal paths are excluded by default.'
  ].join('\n')
}

export function AboutHub({ onNavigate, onRun, snapshot }: Props) {
  const version = snapshot.version as VersionInfo | null
  const latest = snapshot.updates.latest
  const selectedProfile = snapshot.profiles?.selected || snapshot.settings.selectedProfile || 'Unavailable'
  const profile = snapshot.profiles?.profiles.find(item => item.name === selectedProfile)
  const [preview, setPreview] = useState(false)
  const [includePaths, setIncludePaths] = useState(false)
  const summary = useMemo(() => buildSupportSummary(snapshot, includePaths), [includePaths, snapshot])
  const runtimeVersions = {
    chromium: navigator.userAgent.match(/Chrome\/([0-9.]+)/)?.[1] || 'Unavailable',
    electron: navigator.userAgent.match(/Electron\/([0-9.]+)/)?.[1] || 'Unavailable'
  }

  const copy = (value: string) => void navigator.clipboard.writeText(value)
  const openRelative = (relativePath: string) => void window.hermesDesktop.localWorkstation.openPath(relativePath)
  const external = (url: string) => void window.hermesDesktop.openExternal(url)

  const components = [
    ['Hermes Local launcher', version?.product?.version || 'Unavailable', version?.product?.version || 'Unavailable', version?.product?.channel || 'development'],
    ['Hermes Agent harness', short(snapshot.updates.installed.harnessCommit), snapshot.health.hermes ? short(snapshot.updates.installed.harnessCommit) : 'Not running', latest?.target?.candidate ? short(latest.target.candidate) : 'Not checked'],
    ['Hermes gateway / backend', short(snapshot.updates.installed.baseCommit), snapshot.health.dashboard ? short(snapshot.updates.installed.baseCommit) : 'Not running', latest?.target?.candidate ? short(latest.target.candidate) : 'Not checked'],
    ['llama.cpp', short(version?.sources?.llamaCpp?.commit), snapshot.health.model ? short(version?.sources?.llamaCpp?.commit) : 'Not running', 'Pinned by build'],
    ['Selected model', snapshot.model?.displayName || snapshot.model?.alias || 'Unavailable', snapshot.health.model ? snapshot.model?.alias || 'Running' : 'Not running', short(snapshot.model?.revision)],
    ['Python', version?.runtime?.python || snapshot.settings.runtime.pythonVersion, version?.runtime?.python || snapshot.settings.runtime.pythonVersion, 'Pinned by build'],
    ['Node.js', version?.runtime?.node || 'Unavailable', version?.runtime?.node || 'Unavailable', 'Pinned by build'],
    ['CUDA / backend', version?.runtime?.cudaToolkit || 'Unavailable', snapshot.settings.runtime.acceleration, 'Pinned by build'],
    ['Electron', runtimeVersions.electron, runtimeVersions.electron, 'Bundled'],
    ['Chromium', runtimeVersions.chromium, runtimeVersions.chromium, 'Bundled']
  ]

  const health = [
    ['Launcher/version metadata', Boolean(version?.product?.version), version?.product?.version ? `VERSION.json reports ${version.product.version}.` : 'Build metadata is unavailable.'],
    ['Harness revision', Boolean(snapshot.updates.installed.harnessTree), snapshot.updates.installed.harnessTree ? `${snapshot.updates.installed.patchCount} patches resolve to ${short(snapshot.updates.installed.harnessTree)}.` : 'Harness tree is unavailable.'],
    ['Runtime entrypoints', Boolean(snapshot.actions.start && snapshot.actions.stop), snapshot.actions.start && snapshot.actions.stop ? 'Managed start/stop entrypoints are present.' : 'Managed runtime entrypoints are incomplete.'],
    ['Selected model', Boolean(snapshot.model?.installed), snapshot.model?.installed ? 'Installed model manifest is valid.' : 'Selected model is not installed.'],
    ['Configuration', Boolean(selectedProfile), `Active profile: ${selectedProfile}.`],
    ['Data/state', snapshot.storage.stateDatabaseBytes >= 0, `${size(snapshot.storage.stateDatabaseBytes)} state database; ${snapshot.storage.memoryFiles} memory files.`],
    ['Update metadata', !latest || Boolean(latest.status), latest ? `${latest.mode} is ${latest.status}.` : 'No update check has been recorded yet.']
  ]

  return (
    <div className="grid min-w-0 gap-4" data-testid="about-diagnostics-hub">
      <Card title="Product identity">
        <div className="grid gap-4 p-4 lg:grid-cols-[1fr_auto]">
          <div>
            <h2 className="text-lg font-semibold">{version?.product?.name || 'Hermes Launcher'}</h2>
            <p className="mt-1 text-xs leading-5 text-(--ui-text-secondary)">
              Local product, build and diagnostics metadata. Live service controls remain on Home/Services; active work remains in Task Centre.
            </p>
            {!version && <p className="mt-3 rounded-lg border border-amber-500/25 p-3 text-xs text-amber-500">Build metadata is unavailable. Local support actions remain usable.</p>}
            <dl className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
              <Value label="Version" value={version?.product?.version || 'Unavailable'} />
              <Value label="Channel" value={version?.product?.channel || 'development'} />
              <Value label="Build date" value={version?.recordedAt || 'Unavailable'} />
              <Value label="Installation type" value={detectInstallationType(snapshot)} />
              <Value label="Base revision" value={short(snapshot.updates.installed.baseCommit)} />
              <Value label="Harness commit" value={short(snapshot.updates.installed.harnessCommit)} />
              <Value label="Harness tree" value={short(snapshot.updates.installed.harnessTree)} />
              <Value label="Patch series" value={`${snapshot.updates.installed.patchCount} patches`} />
              <Value label="Published-build match" value={snapshot.updates.installed.harnessTree ? 'Verified harness metadata' : 'Unverified'} />
              <Value label="Last update check" value={latest?.updatedAt || latest?.requestedAt || 'Never'} />
            </dl>
          </div>
          <div className="flex flex-wrap content-start gap-2">
            <Button onClick={() => copy(version?.product?.version || 'Unavailable')} size="sm" variant="outline">Copy version</Button>
            <Button onClick={() => copy(snapshot.updates.installed.harnessCommit || '')} size="sm" variant="outline">Copy build</Button>
            <Button onClick={() => openRelative('.')} size="sm" variant="outline"><IconFolder className="size-3.5" /> Open installation folder</Button>
          </div>
        </div>
      </Card>

      <Card title="Components">
        <div className="overflow-x-auto">
          <table className="w-full min-w-[44rem] text-left text-xs">
            <thead className="border-b border-(--ui-stroke-secondary) text-(--ui-text-tertiary)">
              <tr><th className="px-4 py-2">Component</th><th className="px-4 py-2">Installed</th><th className="px-4 py-2">Running</th><th className="px-4 py-2">Available / source</th></tr>
            </thead>
            <tbody>
              {components.map(([name, installed, running, available]) => (
                <tr className="border-b border-(--ui-stroke-secondary) last:border-0" key={name}>
                  <th className="px-4 py-3 font-semibold">{name}</th><td className="px-4 py-3 font-mono">{installed}</td><td className="px-4 py-3 font-mono">{running}</td><td className="px-4 py-3 font-mono">{available}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>

      <div className="grid gap-4 xl:grid-cols-2">
        <Card title="System summary">
          <dl className="grid gap-3 p-4 sm:grid-cols-2">
            <Value label="Operating system" value={navigator.userAgent.match(/Windows NT [^;)]+/)?.[0] || 'Unavailable'} />
            <Value label="CPU" value={snapshot.hardware.cpu} />
            <Value label="RAM" value={size(snapshot.hardware.memoryTotalBytes)} />
            <Value label="GPU" value={snapshot.gpu?.name || 'Unavailable'} />
            <Value label="VRAM" value={snapshot.gpu ? size(snapshot.gpu.memoryTotalMiB * 1024 ** 2) : 'Unavailable'} />
            <Value label="Acceleration" value={snapshot.settings.runtime.acceleration} />
            <Value label="Model" value={snapshot.model?.displayName || snapshot.model?.alias || 'Unavailable'} />
            <Value label="Context" value={profile ? `${profile.contextTokens.toLocaleString()} tokens` : 'Unavailable'} />
            <Value label="Profile" value={selectedProfile} />
            <Value label="Network mode" value={snapshot.settings.network.host === '127.0.0.1' ? 'Loopback only' : `LAN: ${snapshot.settings.network.host}`} />
            <Value label="State database" value={size(snapshot.storage.stateDatabaseBytes)} />
            <Value label="Memory files" value={snapshot.storage.memoryFiles} />
          </dl>
        </Card>
        <Card title="Installation health">
          <div>
            {health.map(([label, ok, detail]) => (
              <div className="flex gap-3 border-b border-(--ui-stroke-secondary) px-4 py-3 last:border-0" key={String(label)}>
                <span className={ok ? 'text-emerald-500' : 'text-amber-500'}>{ok ? '✓' : '!'}</span>
                <div><p className="text-xs font-semibold">{label}</p><p className="mt-1 text-[0.6875rem] text-(--ui-text-tertiary)">{detail}</p></div>
              </div>
            ))}
          </div>
        </Card>
      </div>

      <Card title="Installation and data locations">
        <div className="divide-y divide-(--ui-stroke-secondary)">
          {PATHS.map(([label, relative]) => {
            const absolute = fullPath(snapshot.root, relative)
            return (
              <div className="flex min-w-0 flex-wrap items-center gap-2 px-4 py-3" key={label}>
                <div className="min-w-0 flex-1"><p className="text-xs font-semibold">{label}</p><p className="mt-1 break-all font-mono text-[0.6875rem] text-(--ui-text-tertiary)">{absolute}</p></div>
                <Button aria-label={`Copy ${label} path`} onClick={() => copy(absolute)} size="sm" variant="outline">Copy path</Button>
                <Button aria-label={`Reveal ${label}`} onClick={() => openRelative(relative)} size="sm" variant="outline">Reveal in File Explorer</Button>
              </div>
            )
          })}
        </div>
      </Card>

      <Card title="Diagnostics & support">
        <div className="grid gap-4 p-4 xl:grid-cols-[1fr_auto]">
          <div>
            <p className="text-sm font-semibold">Privacy-aware diagnostic bundle</p>
            <p className="mt-1 text-xs text-(--ui-text-secondary)">Generated locally and never uploaded automatically. Preview the manifest before export.</p>
            {preview && (
              <div className="mt-3 rounded-lg border border-(--ui-stroke-secondary) p-3 text-xs leading-5">
                <b>Included:</b> build/runtime metadata, hardware summary, profile/model metadata, redacted log tails, VERSION.json, release/checksum metadata and generated reports when present.
                <p className="mt-2 font-semibold text-emerald-500">Excluded: tokens, API keys, passwords, cookies, prompts, conversations, private files and secret environment values.</p>
              </div>
            )}
          </div>
          <div className="flex flex-wrap content-start gap-2 xl:max-w-[26rem] xl:justify-end">
            <Button onClick={() => setPreview(value => !value)} size="sm" variant="outline">{preview ? 'Hide export preview' : 'Preview diagnostic export'}</Button>
            <Button disabled={!preview || !snapshot.actions.diagnostics} onClick={() => onRun('diagnostics')} size="sm">Export diagnostic bundle</Button>
            <Button onClick={() => onNavigate('/logs')} size="sm" variant="outline">Open logs</Button>
            <Button onClick={() => openRelative('reports')} size="sm" variant="outline">Open reports</Button>
            <Button onClick={() => openRelative('logs\\diagnostics\\latest-test.json')} size="sm" variant="outline">Latest test report</Button>
            <Button disabled={!snapshot.reports.benchmark} onClick={() => openRelative('benchmarks\\reports\\LATEST.md')} size="sm" variant="outline">Latest benchmark</Button>
            <Button disabled={!snapshot.reports.security} onClick={() => openRelative('security\\reports\\SECURITY_REPORT.md')} size="sm" variant="outline">Latest security report</Button>
            <Button disabled={!snapshot.actions.test} onClick={() => onRun('test')} size="sm" variant="outline">Run quick diagnostics</Button>
            <Button disabled={!snapshot.actions.repair} onClick={() => onRun('repair')} size="sm" variant="outline">Repair installation</Button>
          </div>
        </div>
      </Card>

      <Card title="Support summary">
        <div className="grid gap-3 p-4 xl:grid-cols-[1fr_auto]">
          <pre className="max-h-80 overflow-auto whitespace-pre-wrap rounded-lg bg-(--ui-control-background) p-3 font-mono text-[0.6875rem] leading-5">{summary}</pre>
          <div className="flex flex-wrap content-start gap-2">
            <Button onClick={() => copy(summary)} size="sm">Copy Markdown summary</Button>
            <Button onClick={() => setIncludePaths(value => !value)} size="sm" variant="outline">{includePaths ? 'Redact personal paths' : 'Include exact paths'}</Button>
          </div>
        </div>
      </Card>

      <div className="grid gap-4 xl:grid-cols-2">
        <Card title="Updates & release notes">
          <div className="p-4">
            <dl className="grid gap-3 sm:grid-cols-2">
              <Value label="Current version" value={version?.product?.version || 'Unavailable'} />
              <Value label="Available Agent base" value={short(latest?.target?.candidate)} />
              <Value label="Update state" value={latest ? `${latest.mode} · ${latest.status}` : 'Not checked'} />
              <Value label="Last successful update" value={latest?.status === 'succeeded' ? latest.completedAt || latest.updatedAt : 'Unavailable'} />
            </dl>
            <div className="mt-4 flex flex-wrap gap-2">
              <Button disabled={!snapshot.actions.update} onClick={() => onRun('update', { mode: 'Check' })} size="sm" variant="outline"><IconRefresh className="size-3.5" /> Check updates</Button>
              <Button onClick={() => onNavigate('/services')} size="sm" variant="outline">Inspect updates</Button>
              <Button onClick={() => openRelative('CHANGELOG-LOCAL.md')} size="sm" variant="outline">Local changelog</Button>
              <Button onClick={() => external(LINKS.releases)} size="sm" variant="outline">Release page <IconExternalLink className="size-3.5" /></Button>
            </div>
          </div>
        </Card>
        <Card title="Licences, acknowledgements & links">
          <div className="grid gap-2 p-4 text-xs">
            <p><b>Hermes Local licence:</b> installed LICENSE file.</p>
            <p><b>Hermes Agent licence:</b> upstream/integration LICENSE file.</p>
            <p><b>Model licence:</b> {snapshot.model?.license || version?.starterModel?.license || 'Not declared in installed metadata.'}</p>
            <p><b>Third-party inventory:</b> security/SBOM output when generated.</p>
            <div className="flex flex-wrap gap-2 pt-2">
              <Button onClick={() => openRelative('LICENSE')} size="sm" variant="outline">Open local licence</Button>
              <Button onClick={() => openRelative('source\\hermes-agent\\LICENSE')} size="sm" variant="outline">Open Agent licence</Button>
              <Button onClick={() => openRelative('security\\sbom')} size="sm" variant="outline">Open SBOM</Button>
              {[["Hermes Local", LINKS.project], ["Hermes Agent", LINKS.upstream], ["llama.cpp", LINKS.runtime], ["Documentation", LINKS.docs], ["Issue tracker", LINKS.issues]].map(([label, url]) => (
                <Button key={label} onClick={() => external(url)} size="sm" variant="outline">{label} <IconExternalLink className="size-3.5" /></Button>
              ))}
            </div>
          </div>
        </Card>
      </div>

      <Card title="Historical validation evidence">
        <div className="p-4">
          <p className="text-xs leading-5 text-(--ui-text-secondary)">TASKS.md is historical reference-build evidence, not current task state. Active diagnostics, updates and repairs are managed in Task Centre.</p>
          <details className="mt-3"><summary className="cursor-pointer text-xs font-semibold">View reference build ledger</summary><pre className="mt-3 max-h-96 overflow-auto whitespace-pre-wrap rounded-lg bg-(--ui-control-background) p-3 font-mono text-[0.6875rem] leading-5">{snapshot.taskLedger || 'Historical ledger unavailable.'}</pre></details>
          <Button className="mt-3" onClick={() => openRelative('TASKS.md')} size="sm" variant="outline">Reveal TASKS.md</Button>
          <Button className="mt-3" onClick={() => onNavigate('/tasks')} size="sm" variant="outline">Open current Task Centre</Button>
        </div>
      </Card>

      {latest?.failure && (
        <div className="flex gap-2 rounded-xl border border-red-500/25 p-4 text-xs text-red-500">
          <IconAlertTriangle className="size-4 shrink-0" />
          <span>{latest.failure.stage || latest.currentStage || 'update'}: {latest.failure.message}</span>
        </div>
      )}
    </div>
  )
}
