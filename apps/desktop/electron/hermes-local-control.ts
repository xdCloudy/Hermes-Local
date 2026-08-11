import { type ChildProcessWithoutNullStreams, execFile, execFileSync, spawn } from 'node:child_process'
import crypto from 'node:crypto'
import { EventEmitter } from 'node:events'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { promisify } from 'node:util'

import { app, ipcMain, shell } from 'electron'

import {
  type HermesLocalDashboardBounds,
  type HermesLocalDashboardViewController,
  normalizeHermesLocalDashboardUrl
} from './hermes-local-dashboard-view'
import {
  desktopUpdateTaskContext,
  expectedUpdateOperationComponent,
  parseDesktopUpdateHandoffMarker,
  parseDesktopUpdateResultMarker,
  parseDesktopUpdateStatusMarker,
  planDesktopUpdateAction
} from './hermes-local-desktop-update'
import { readGatewayStatus } from './hermes-local-gateway-status'
import {
  isPausedModelDownload,
  modelDownloadCompletionEvidence,
  type ModelDownloadProgressDocument,
  modelDownloadProgressPath,
  taskProgressFromModelDownload
} from './hermes-local-model-download'
import { activeModelSwitch, planModelSelection, runtimeModelIdentityMatches } from './hermes-local-model-switch'
import {
  deleteProfile,
  readLocalConfiguration,
  registerModel,
  removeModel,
  sanitizeEditableProfile,
  saveProfile,
  saveWorkstationSettings,
  selectModel,
  selectProfile,
  validProfileName
} from './hermes-local-settings'
import {
  admitTask,
  boundedTaskOutput,
  createTaskRecord,
  isTaskTerminal,
  reconcileRecoveredTask,
  requestTaskCancellation,
  type TaskAction,
  taskCapabilities,
  type TaskCompletionEvidence,
  type TaskProgress,
  type TaskRecord,
  type TaskView,
  transitionTask
} from './hermes-local-task-model'
import { loadTaskStore, saveTaskStore } from './hermes-local-task-store'
import { registerHermesLocalTrustCentreIpc } from './hermes-local-trust-centre'

const execFileAsync = promisify(execFile)
const MAX_TASK_OUTPUT = 128 * 1024
const MAX_COMPLETED_TASKS = 50
const TASK_RECONCILE_INTERVAL_MS = 1000
const TASK_STORE_RELATIVE_PATH = 'data\\runtime\\desktop-tasks.json'
const MAX_LOG_BYTES = 512 * 1024
const LOGIN_ITEM_ARGUMENTS = ['--hermes-local-autostart']

const ACTION_SCRIPTS = {
  backup: 'Backup-Hermes-Local.ps1',
  benchmark: 'Benchmark-Hermes-Local.ps1',
  diagnostics: 'Export-Hermes-Diagnostics.ps1',
  'model-download': 'Invoke-Hermes-ModelDownload.ps1',
  repair: 'Repair-Hermes-Local.ps1',
  restart: 'Restart-Hermes-Local.ps1',
  restore: 'Restore-Hermes-Local.ps1',
  security: 'Security-Scan-Hermes-Local.ps1',
  start: 'Start-Hermes-Local.ps1',
  stop: 'Stop-Hermes-Local.ps1',
  'switch-model': 'Switch-Hermes-Model.ps1',
  test: 'Test-Hermes-Local.ps1',
  update: 'Update-Hermes-Local.ps1'
} as const satisfies Record<TaskAction, string>

const LOG_FILES = {
  dashboard: 'logs\\dashboard\\dashboard.log',
  hermes: 'data\\hermes\\logs\\gui.log',
  launcher: 'logs\\launcher\\launcher.log',
  model: 'logs\\model-server\\llama-server.log',
  security: 'logs\\security\\security.log',
  supervisor: 'logs\\supervisor\\supervisor.log'
} as const

type ActionName = TaskAction
type LogName = keyof typeof LOG_FILES

interface ActionTask extends TaskRecord {
  child: ChildProcessWithoutNullStreams | null
  desktopUpdateMarkerBuffer?: string
  desktopUpdateResult?: Record<string, unknown> | null
  desktopUpdateStatus?: Record<string, unknown> | null
  events: EventEmitter
  input: Record<string, unknown>
}

const tasks = new Map<string, ActionTask>()
let taskRegistryLoaded = false
let taskPersistTimer: null | NodeJS.Timeout = null
let taskReconcileTimer: null | NodeJS.Timeout = null

function powershellExecutable(): string {
  const systemRoot = process.env.SystemRoot || 'C:\\Windows'

  const candidates = [
    process.env.HERMES_LOCAL_PWSH,
    path.join(process.env.ProgramFiles || 'C:\\Program Files', 'PowerShell', '7', 'pwsh.exe')
  ]

  try {
    const where = path.join(systemRoot, 'System32', 'where.exe')

    const discovered = execFileSync(where, ['pwsh.exe'], {
      encoding: 'utf8',
      maxBuffer: 16 * 1024,
      timeout: 5000,
      windowsHide: true
    })

    candidates.push(...discovered.split(/\r?\n/).map(value => value.trim()))
  } catch {
    // Continue to the built-in Windows PowerShell fallback.
  }

  candidates.push(path.join(systemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'))

  const resolved = candidates.find(candidate => candidate && path.isAbsolute(candidate) && fs.existsSync(candidate))

  if (!resolved) {
    throw new Error('PowerShell is not installed')
  }

  return resolved
}

function localRoot(): string {
  const configured = String(process.env.HERMES_LOCAL_ROOT || '').trim()

  if (configured && !path.isAbsolute(configured)) {
    throw new Error('HERMES_LOCAL_ROOT must be absolute')
  }

  if (configured) {
    return path.resolve(configured)
  }

  const commandLineRoot = process.argv
    .find(argument => argument.startsWith('--hermes-local-root='))
    ?.slice('--hermes-local-root='.length)

  if (commandLineRoot) {
    if (!path.isAbsolute(commandLineRoot)) {
      throw new Error('--hermes-local-root must be absolute')
    }

    return path.resolve(commandLineRoot)
  }

  const candidates = [
    process.env.PORTABLE_EXECUTABLE_DIR,
    path.dirname(process.execPath),
    typeof app.getAppPath === 'function' ? app.getAppPath() : null,
    process.cwd()
  ].filter((candidate): candidate is string => Boolean(candidate && path.isAbsolute(candidate)))

  for (const start of candidates) {
    let current = path.resolve(start)

    for (let depth = 0; depth < 8; depth += 1) {
      if (
        fs.existsSync(path.join(current, 'VERSION.json')) &&
        fs.existsSync(path.join(current, 'scripts', 'Common-Hermes.psm1'))
      ) {
        return current
      }

      const parent = path.dirname(current)

      if (parent === current) {
        break
      }

      current = parent
    }
  }

  throw new Error(
    'Hermes Local project root was not found. Set HERMES_LOCAL_ROOT or launch the portable app from the project.'
  )
}

function resolveUnderRoot(relativePath: string): string {
  const root = localRoot()
  const resolved = path.resolve(root, relativePath)
  const prefix = `${root.toLocaleLowerCase()}${path.sep}`

  if (resolved.toLocaleLowerCase() !== root.toLocaleLowerCase() && !resolved.toLocaleLowerCase().startsWith(prefix)) {
    throw new Error('Path escapes the Hermes Local root')
  }

  return resolved
}

async function openLocalPath(relativePathValue: unknown) {
  const relativePath = String(relativePathValue || '').trim()

  if (!relativePath || path.isAbsolute(relativePath) || relativePath.includes('\0')) {
    throw new Error('Hermes Local path must be relative to the installation root')
  }

  const target = resolveUnderRoot(relativePath)

  if (!fs.existsSync(target)) {
    return { error: `Path does not exist: ${relativePath}`, ok: false, path: target }
  }

  if (fs.statSync(target).isDirectory()) {
    const error = await shell.openPath(target)

    return { error, ok: !error, path: target }
  }

  shell.showItemInFolder(target)

  return { error: '', ok: true, path: target }
}

function readJson<T>(relativePath: string): null | T {
  const filePath = resolveUnderRoot(relativePath)

  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8')) as T
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return null
    }

    throw error
  }
}

function redacted(text: string): string {
  let safe = text
    .replace(/(authorization\s*[:=]\s*bearer\s+)[^\s"']+/gi, '$1[REDACTED]')
    .replace(/((?:api[_-]?key|password|secret|token|credential)\s*[:=]\s*)[^\s,"']+/gi, '$1[REDACTED]')
    .replace(/\b(?:https?|wss?):\/\/[^\s/@:]+:[^\s/@]+@/gi, 'https://[REDACTED]@')
    .replace(
      /(?<![\d.])(?:10(?:\.\d{1,3}){3}|192\.168(?:\.\d{1,3}){2}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2})(?![\d.])/g,
      '[PRIVATE-TARGET]'
    )
    .replace(/[A-Za-z0-9_-]{48,}/g, '[REDACTED-LONG-VALUE]')

  const privatePaths = [process.env.USERPROFILE]

  try {
    privatePaths.push(localRoot())
  } catch {
    // Redaction still applies to credentials when the root is unavailable.
  }

  for (const privatePath of privatePaths.filter((value): value is string => Boolean(value))) {
    safe = safe.replaceAll(privatePath, '[PRIVATE-PATH]').replaceAll(privatePath.replaceAll('\\', '/'), '[PRIVATE-PATH]')
  }

  return safe
}

function appendTaskOutput(task: ActionTask, chunk: Buffer | string): void {
  const rawText = String(chunk)
  task.desktopUpdateMarkerBuffer = `${task.desktopUpdateMarkerBuffer || ''}${rawText}`.slice(-64 * 1024)
  const desktopUpdateStatus = parseDesktopUpdateStatusMarker(task.desktopUpdateMarkerBuffer)
  const desktopUpdateResult = parseDesktopUpdateResultMarker(task.desktopUpdateMarkerBuffer)
  const desktopUpdateHandoff = parseDesktopUpdateHandoffMarker(task.desktopUpdateMarkerBuffer)
  const text = redacted(rawText)
  const result = boundedTaskOutput(task.output, text, MAX_TASK_OUTPUT)
  const modelStageMatches = [...text.matchAll(/::hermes-model-switch-stage::([a-z0-9-]+)::([^\r\n]+)/gi)]
  const benchmarkStageMatches = [...text.matchAll(/::hermes-benchmark-stage::([a-z0-9-]+)::([^\r\n]+)/gi)]
  const updateStageMatches = [...text.matchAll(/::hermes-update-stage::([a-z0-9-]+)::([^\r\n]+)/gi)]
  const securityStageMatches = [...text.matchAll(/::hermes-security-stage::([a-z0-9-]+)::([^\r\n]+)/gi)]
  const restoreStageMatches = [...text.matchAll(/::hermes-restore-stage::([a-z0-9-]+)::([^\r\n]+)/gi)]
  const modelDownloadStageMatches = [
    ...text.matchAll(/::hermes-model-download-stage::([a-z0-9-]+)::([^\r\n]+)/gi)
  ]
  const stage =
    modelDownloadStageMatches.at(-1)?.[1] ||
    restoreStageMatches.at(-1)?.[1] ||
    securityStageMatches.at(-1)?.[1] ||
    updateStageMatches.at(-1)?.[1] ||
    benchmarkStageMatches.at(-1)?.[1] ||
    modelStageMatches.at(-1)?.[1]

  task.output = result.output
  task.outputTruncated ||= result.truncated
  task.stage = stage || task.stage
  task.desktopUpdateStatus = desktopUpdateStatus || task.desktopUpdateStatus
  task.desktopUpdateResult = desktopUpdateResult || task.desktopUpdateResult
  if (desktopUpdateHandoff && task.action === 'update' && task.context.component === 'HermesLocal') {
    task.owner = { kind: 'external-process', pid: desktopUpdateHandoff.pid }
    task.stage = 'waiting-for-restart'
    task.context = { ...task.context, operationId: desktopUpdateHandoff.operationId }
  }
  task.updatedAt = new Date().toISOString()
  scheduleTaskRegistryPersistence()
}

function safeReadJson<T>(relativePath: string): null | T {
  try {
    return readJson<T>(relativePath)
  } catch {
    return null
  }
}

function taskRecord(task: ActionTask): TaskRecord {
  const {
    child: _child,
    desktopUpdateMarkerBuffer: _desktopUpdateMarkerBuffer,
    desktopUpdateResult: _desktopUpdateResult,
    desktopUpdateStatus: _desktopUpdateStatus,
    events: _events,
    input: _input,
    ...record
  } = task

  return record
}

function publicTask(task: ActionTask): TaskView {
  return { ...taskRecord(task), capabilities: taskCapabilities(task) }
}

function replaceTaskRecord(task: ActionTask, record: TaskRecord): void {
  Object.assign(task, record)
}

function taskStorePath(): string {
  return resolveUnderRoot(TASK_STORE_RELATIVE_PATH)
}

function persistTaskRegistry(): void {
  saveTaskStore(
    taskStorePath(),
    [...tasks.values()].map(task => taskRecord(task)),
    new Date().toISOString(),
    MAX_COMPLETED_TASKS
  )
}

function persistTaskRegistrySafely(): void {
  try {
    persistTaskRegistry()
  } catch (error) {
    console.error('Failed to persist Hermes Local task registry', error)
  }
}

function scheduleTaskRegistryPersistence(): void {
  if (taskPersistTimer) {
    return
  }

  taskPersistTimer = setTimeout(() => {
    taskPersistTimer = null
    persistTaskRegistrySafely()
  }, 100)
  taskPersistTimer.unref()
}

function flushScheduledTaskPersistence(): void {
  if (taskPersistTimer) {
    clearTimeout(taskPersistTimer)
    taskPersistTimer = null
  }

  persistTaskRegistrySafely()
}

function fileEvidence(
  relativePath: string,
  status: TaskCompletionEvidence['status'] = 'succeeded',
  failure: TaskCompletionEvidence['failure'] = null,
  kind: NonNullable<TaskCompletionEvidence['result']>['kind'] = 'report'
): null | TaskCompletionEvidence {
  const filePath = resolveUnderRoot(relativePath)

  try {
    return {
      exitCode: status === 'succeeded' ? 0 : 1,
      failure,
      observedAt: fs.statSync(filePath).mtime.toISOString(),
      result: { kind, path: relativePath.replaceAll('\\', '/') },
      status
    }
  } catch {
    return null
  }
}

function jsonEvidence(
  relativePath: string,
  outcome: (document: Record<string, any>) => {
    failure?: TaskCompletionEvidence['failure']
    status: TaskCompletionEvidence['status']
  }
): null | TaskCompletionEvidence {
  const document = safeReadJson<Record<string, any>>(relativePath)
  const evidence = fileEvidence(relativePath)

  if (!document || !evidence) {
    return null
  }

  const result = outcome(document)

  return {
    ...evidence,
    exitCode: result.status === 'succeeded' ? 0 : 1,
    failure: result.failure || null,
    status: result.status
  }
}

function newestArchiveEvidence(relativeDirectory: string, prefix: string): null | TaskCompletionEvidence {
  const directory = resolveUnderRoot(relativeDirectory)

  try {
    const archive = fs
      .readdirSync(directory, { withFileTypes: true })
      .filter(entry => entry.isFile() && entry.name.startsWith(prefix) && entry.name.endsWith('.zip'))
      .map(entry => ({
        modified: fs.statSync(path.join(directory, entry.name)).mtimeMs,
        relativePath: path.join(relativeDirectory, entry.name)
      }))
      .sort((left, right) => right.modified - left.modified)[0]

    return archive ? fileEvidence(archive.relativePath, 'succeeded', null, 'archive') : null
  } catch {
    return null
  }
}

function benchmarkProgress(task: TaskRecord): null | Record<string, any> {
  const progress = safeReadJson<Record<string, any>>('data\\runtime\\benchmark-progress.json')

  return progress && String(progress.taskId || '') === task.id ? progress : null
}

function benchmarkProgressSummary(progress: Record<string, any>): string {
  const completed = Number.isFinite(Number(progress.completedUnits)) ? Number(progress.completedUnits) : null
  const total = Number.isFinite(Number(progress.totalUnits)) ? Number(progress.totalUnits) : null
  const units = completed !== null && total !== null ? ` ${completed}/${total}` : ''

  return `Benchmark progress: ${String(progress.stage || 'running')}${units} · ${String(progress.message || '')}`.trim()
}

function securityProgress(task: TaskRecord): null | Record<string, any> {
  const progress = safeReadJson<Record<string, any>>('data\\runtime\\security-scan-progress.json')

  return progress && String(progress.taskId || '') === task.id ? progress : null
}

function taskProgressFromSecurityDocument(progress: Record<string, any>): TaskProgress {
  const completed = Number.isFinite(Number(progress.completedChecks)) ? Math.max(0, Number(progress.completedChecks)) : null
  const total = Number.isFinite(Number(progress.totalChecks)) && Number(progress.totalChecks) > 0
    ? Number(progress.totalChecks)
    : null
  const percent = Number.isFinite(Number(progress.percent))
    ? Math.min(100, Math.max(0, Number(progress.percent)))
    : completed !== null && total !== null
      ? Math.min(100, Math.round((completed / total) * 1000) / 10)
      : null
  const counters = progress.counters && typeof progress.counters === 'object'
    ? Object.fromEntries(
        Object.entries(progress.counters)
          .filter(([key, value]) => key.length <= 64 && Number.isFinite(Number(value)) && Number(value) >= 0)
          .map(([key, value]) => [key, Number(value)])
      )
    : {}

  return {
    completedUnits: completed,
    counters,
    message: progress.message ? redacted(String(progress.message)) : null,
    mode: progress.mode === 'determinate' ? 'determinate' : 'indeterminate',
    percent,
    totalUnits: total
  }
}

function securityProgressSummary(progress: Record<string, any>): string {
  const counters = progress.counters || {}
  const checks = Number.isFinite(Number(counters.checks)) ? Number(counters.checks) : 0
  const findings = Number.isFinite(Number(counters.findings)) ? Number(counters.findings) : 0
  const targets = Number.isFinite(Number(counters.targets)) ? Number(counters.targets) : 0

  return redacted(
    `Security scan: ${String(progress.stage || 'running')} · ${checks} checks · ${findings} findings · ${targets} targets · ${String(progress.message || '')}`
  )
}

function securityCompletionEvidence(task: TaskRecord): null | TaskCompletionEvidence {
  const progress = securityProgress(task)
  const status = String(progress?.status || '')

  if (!progress || !['cancelled', 'failed', 'stale', 'succeeded'].includes(status)) {
    return null
  }

  const resultPath = String(progress.result?.directory || progress.result?.report || 'security/reports/latest-scan.json')
    .replaceAll('\\', '/')
  const observedAt = String(progress.completedAt || progress.updatedAt || new Date().toISOString())

  if (status === 'succeeded') {
    return {
      exitCode: 0,
      failure: null,
      observedAt,
      result: { kind: 'report', path: resultPath },
      status: 'succeeded'
    }
  }

  if (status === 'cancelled') {
    return {
      exitCode: 130,
      failure: null,
      observedAt,
      result: { kind: 'report', path: resultPath },
      status: 'cancelled'
    }
  }

  const stale = status === 'stale'

  return {
    exitCode: stale ? null : 1,
    failure: {
      code: String(progress.failure?.code || (stale ? 'security-scan-stale' : 'security-scan-failed')),
      message: redacted(
        String(progress.failure?.message || (stale ? 'Recovered security scan marker is stale' : 'Security scan failed'))
      )
    },
    observedAt,
    result: { kind: 'report', path: resultPath },
    status: stale ? 'interrupted' : 'failed'
  }
}

function restoreProgress(task: TaskRecord): null | Record<string, any> {
  const progress = safeReadJson<Record<string, any>>('data\\runtime\\restore-progress.json')

  return progress && String(progress.taskId || '') === task.id ? progress : null
}

function taskProgressFromRestoreDocument(progress: Record<string, any>): TaskProgress {
  const completed = Number.isFinite(Number(progress.completedUnits)) ? Math.max(0, Number(progress.completedUnits)) : null
  const total = Number.isFinite(Number(progress.totalUnits)) && Number(progress.totalUnits) > 0
    ? Number(progress.totalUnits)
    : null
  const percent = Number.isFinite(Number(progress.percent))
    ? Math.min(100, Math.max(0, Number(progress.percent)))
    : completed !== null && total !== null
      ? Math.min(100, Math.round((completed / total) * 1000) / 10)
      : null
  const counters = progress.counters && typeof progress.counters === 'object'
    ? Object.fromEntries(
        Object.entries(progress.counters)
          .filter(([key, value]) => key.length <= 64 && Number.isFinite(Number(value)) && Number(value) >= 0)
          .map(([key, value]) => [key, Number(value)])
      )
    : {}

  return {
    cancellable: progress.cancellable !== false,
    completedUnits: completed,
    counters,
    message: progress.message ? redacted(String(progress.message)) : null,
    mode: progress.mode === 'determinate' ? 'determinate' : 'indeterminate',
    percent,
    totalUnits: total
  }
}

function restoreProgressSummary(progress: Record<string, any>): string {
  const completed = Number.isFinite(Number(progress.completedUnits)) ? Number(progress.completedUnits) : null
  const total = Number.isFinite(Number(progress.totalUnits)) ? Number(progress.totalUnits) : null
  const units = completed !== null && total !== null ? ` ${completed}/${total}` : ''

  return redacted(
    `Restore progress: ${String(progress.stage || 'running')}${units} · ${String(progress.message || '')}`
  )
}

function restoreCompletionEvidence(task: TaskRecord): null | TaskCompletionEvidence {
  const progress = restoreProgress(task)
  const status = String(progress?.status || '')

  if (!progress || !['cancelled', 'failed', 'succeeded'].includes(status)) {
    return null
  }

  const reportPath = String(progress.result?.report || 'logs/restore/LATEST.json').replaceAll('\\', '/')
  const observedAt = String(progress.completedAt || progress.updatedAt || new Date().toISOString())

  if (status === 'succeeded') {
    return {
      exitCode: 0,
      failure: null,
      observedAt,
      result: { kind: 'report', path: reportPath },
      status: 'succeeded'
    }
  }

  if (status === 'cancelled') {
    return {
      exitCode: 130,
      failure: null,
      observedAt,
      result: { kind: 'report', path: reportPath },
      status: 'cancelled'
    }
  }

  return {
    exitCode: 1,
    failure: {
      code: String(progress.failure?.code || 'restore-failed'),
      message: redacted(String(progress.failure?.message || 'Hermes Local restore failed'))
    },
    observedAt,
    result: { kind: 'report', path: reportPath },
    status: 'failed'
  }
}

function restoreBackups() {
  const directory = resolveUnderRoot('backups')

  try {
    return fs
      .readdirSync(directory, { withFileTypes: true })
      .filter(entry => entry.isFile() && entry.name.endsWith('.zip') && !entry.name.includes('/') && !entry.name.includes('\\'))
      .map(entry => {
        const archivePath = path.join(directory, entry.name)
        const sidecarPath = `${archivePath}.sha256`
        const stat = fs.statSync(archivePath)
        const sidecar = fs.existsSync(sidecarPath) ? fs.readFileSync(sidecarPath, 'utf8').trim().split(/\s+/)[0] : ''
        const sha256 = /^[0-9a-f]{64}$/i.test(sidecar) ? sidecar.toLowerCase() : null

        return {
          id: sha256 ? sha256.slice(0, 16) : crypto.createHash('sha256').update(entry.name).digest('hex').slice(0, 16),
          modifiedAt: stat.mtime.toISOString(),
          name: entry.name,
          path: `backups/${entry.name}`,
          sha256,
          sizeBytes: stat.size,
          verified: Boolean(sha256)
        }
      })
      .sort((left, right) => Date.parse(right.modifiedAt) - Date.parse(left.modifiedAt))
  } catch {
    return []
  }
}

function readInstalledVersion(): null | Record<string, any> {
  const version = safeReadJson<Record<string, any>>('VERSION.json')
  const overrides = safeReadJson<Record<string, any>>('config\\launcher\\source-overrides.json')
  const hermesAgentOverride = overrides?.sources?.hermesAgent

  if (!version || !hermesAgentOverride) {
    return version
  }

  return {
    ...version,
    sources: {
      ...version.sources,
      hermesAgent: {
        ...version.sources?.hermesAgent,
        ...hermesAgentOverride
      }
    }
  }
}

function updateOperationDocument(): null | Record<string, any> {
  return safeReadJson<Record<string, any>>('data\\runtime\\update-operations\\LATEST.json')
}

function updateOperationForTask(task: TaskRecord): null | Record<string, any> {
  const operation = updateOperationDocument()

  const expectedComponent = expectedUpdateOperationComponent(task.context)

  if (!operation || String(operation.identity?.component || '') !== expectedComponent) {
    return null
  }

  if (String(operation.taskId || '') === task.id) {
    return operation
  }

  const requestedAt = Date.parse(String(operation.identity?.requestedAt || operation.createdAt || ''))
  const taskCreatedAt = Date.parse(task.createdAt)

  return (
    operation.caller === 'Desktop' &&
    Number.isFinite(requestedAt) &&
    Number.isFinite(taskCreatedAt) &&
    requestedAt >= taskCreatedAt
  )
    ? operation
    : null
}

function relativeUpdatePath(value: unknown, fallback: string): string {
  const text = String(value || '').trim()

  if (!text) {
    return fallback
  }

  const root = localRoot()
  const candidate = path.isAbsolute(text) ? path.resolve(text) : path.resolve(root, text)
  const relative = path.relative(root, candidate)

  if (!relative || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative))) {
    return (relative || path.basename(candidate)).replaceAll('\\', '/')
  }

  return fallback
}

function updateOperationTarget(operation: null | Record<string, any>) {
  const check = operation?.stageResults?.check
  const compatibility = operation?.stageResults?.compatibility
  const source = compatibility?.candidate ? compatibility : check

  if (!source) {
    return null
  }

  return {
    candidate: source.candidate ? String(source.candidate) : null,
    current: source.current ? String(source.current) : null,
    updateAvailable:
      typeof source.updateAvailable === 'boolean'
        ? source.updateAvailable
        : source.candidate && source.current
          ? String(source.candidate) !== String(source.current)
          : null
  }
}

function publicUpdateOperation(operation: null | Record<string, any>) {
  if (!operation?.operationId) {
    return null
  }

  const failure = operation.failure

  return {
    completedAt: operation.completedAt ? String(operation.completedAt) : null,
    currentStage: operation.currentStage ? String(operation.currentStage) : null,
    failure: failure
      ? {
          activePreserved:
            typeof failure.activePreserved === 'boolean' ? Boolean(failure.activePreserved) : undefined,
          code: String(failure.code || 'update-operation-failed'),
          message: String(failure.message || 'Hermes Agent update failed'),
          rollback: failure.rollback || undefined,
          stage: failure.stage ? String(failure.stage) : undefined
        }
      : null,
    mode: String(operation.identity?.mode || 'Check'),
    operationId: String(operation.operationId),
    progress: {
      completed: Number(operation.progress?.completed || 0),
      percent: Number(operation.progress?.percent || 0),
      total: Number(operation.progress?.total || 0)
    },
    recovery: {
      previousOperationId: operation.recovery?.previousOperationId
        ? String(operation.recovery.previousOperationId)
        : null,
      recoveredLockPath: operation.recovery?.recoveredLockPath
        ? relativeUpdatePath(operation.recovery.recoveredLockPath, 'data/runtime/locks')
        : null,
      staleLockRecovered: Boolean(operation.recovery?.staleLockRecovered)
    },
    reportPath: operation.reportPath
      ? relativeUpdatePath(
          operation.reportPath,
          `build/updates/operations/${String(operation.operationId)}.json`
        )
      : null,
    requestedAt: String(operation.identity?.requestedAt || operation.createdAt || ''),
    result: operation.result && typeof operation.result === 'object' ? operation.result : null,
    stageResults:
      operation.stageResults && typeof operation.stageResults === 'object' ? operation.stageResults : {},
    status: String(operation.status || 'queued'),
    taskId: operation.taskId ? String(operation.taskId) : null,
    target: updateOperationTarget(operation),
    updatedAt: String(operation.updatedAt || operation.createdAt || '')
  }
}

function updateOperationSummary(operation: Record<string, any>): string {
  const stage = String(operation.failure?.stage || operation.currentStage || operation.status || 'running')
  const message = String(
    operation.failure?.message ||
      [...(Array.isArray(operation.logs) ? operation.logs : [])].reverse().find(entry => entry?.message)?.message ||
      `Hermes Agent update ${String(operation.status || 'running')}`
  )

  return `Hermes Agent update: ${stage} · ${message}`
}

function desktopApplicationUpdateProgress(task: TaskRecord): null | Record<string, any> {
  if (task.context.component !== 'HermesLocal' || !task.context.operationId) {
    return null
  }

  return safeReadJson<Record<string, any>>(
    `build\\updates\\desktop-staging\\${task.context.operationId}\\progress.json`
  )
}

function desktopApplicationUpdateCompletionEvidence(task: TaskRecord): null | TaskCompletionEvidence {
  const progress = desktopApplicationUpdateProgress(task)
  const status = String(progress?.status || '')

  if (!progress || !['failed', 'rolled-back', 'succeeded'].includes(status)) {
    return null
  }

  const succeeded = status === 'succeeded'
  const rolledBack = status === 'rolled-back'
  return {
    exitCode: succeeded ? 0 : 1,
    failure: succeeded
      ? null
      : {
          code: rolledBack ? 'desktop-update-rolled-back' : String(progress.failure?.code || 'desktop-update-failed'),
          message: rolledBack ? 'The application update failed and the previous launcher was restored.' : String(progress.failure?.message || progress.message || 'Desktop update failed')
        },
    observedAt: String(progress.updatedAt || new Date().toISOString()),
    result: { kind: 'report', path: `build/updates/desktop-staging/${task.context.operationId}/result.json` },
    status: succeeded ? 'succeeded' : 'failed'
  }
}

function updateCompletionEvidence(task: TaskRecord): null | TaskCompletionEvidence {
  const desktopEvidence = desktopApplicationUpdateCompletionEvidence(task)

  if (desktopEvidence) {
    return desktopEvidence
  }
  const operation = updateOperationForTask(task)
  const status = String(operation?.status || '')

  if (!operation || !['failed', 'rolled-back', 'succeeded'].includes(status)) {
    return null
  }

  const reportPath = relativeUpdatePath(
    operation.reportPath,
    `build/updates/operations/${String(operation.operationId)}.json`
  )
  const succeeded = status === 'succeeded'
  const rolledBack = status === 'rolled-back'

  return {
    exitCode: succeeded ? 0 : 1,
    failure: succeeded
      ? null
      : {
          code: rolledBack ? 'update-rolled-back' : String(operation.failure?.code || 'update-operation-failed'),
          message: rolledBack
            ? `Update failed during ${String(operation.failure?.stage || 'apply')} and the previous backend was restored.`
            : String(operation.failure?.message || 'Hermes Agent update failed')
        },
    observedAt: String(operation.completedAt || operation.updatedAt || new Date().toISOString()),
    result: { kind: 'report', path: reportPath },
    status: succeeded ? 'succeeded' : 'failed'
  }
}

function modelDownloadProgress(task: TaskRecord): null | ModelDownloadProgressDocument {
  const progress = safeReadJson<ModelDownloadProgressDocument>(modelDownloadProgressPath(task.id))

  return progress && progress.taskId === task.id ? progress : null
}

function modelDownloadProgressSummary(progress: ModelDownloadProgressDocument): string {
  const taskProgress = taskProgressFromModelDownload(progress)
  const bytes = taskProgress.bytesCompleted === null || taskProgress.bytesCompleted === undefined
    ? ''
    : taskProgress.bytesTotal
      ? ` ${taskProgress.bytesCompleted}/${taskProgress.bytesTotal} bytes`
      : ` ${taskProgress.bytesCompleted} bytes`
  return `Model download: ${String(progress.stage || 'running')}${bytes} · ${String(progress.message || '')}`.trim()
}

function taskCompletionEvidence(task: TaskRecord): null | TaskCompletionEvidence {
  if (task.action === 'model-download') {
    return modelDownloadCompletionEvidence(task, modelDownloadProgress(task))
  }

  if (task.action === 'benchmark') {
    const progress = benchmarkProgress(task)
    const status = String(progress?.status || '')

    if (progress && ['cancelled', 'failed', 'succeeded'].includes(status)) {
      const reportPath = String(progress.result?.report || 'benchmarks/reports/LATEST.md').replaceAll('\\', '/')
      const failed = status === 'failed'

      return {
        exitCode: status === 'succeeded' ? 0 : status === 'cancelled' ? 130 : 1,
        failure: failed
          ? {
              code: String(progress.failure?.code || 'benchmark-failed'),
              message: String(progress.failure?.message || 'Recovered benchmark progress records a failure')
            }
          : null,
        observedAt: String(progress.completedAt || progress.updatedAt || new Date().toISOString()),
        result: { kind: 'report', path: reportPath },
        status: status as TaskCompletionEvidence['status']
      }
    }

    return jsonEvidence('benchmarks\\results\\latest.json', document => {
      const complete = document.lifecycle?.state === 'complete'
      const failed = Array.isArray(document.cases) && document.cases.some((entry: any) => entry?.succeeded === false)

      return complete && !failed
        ? { status: 'succeeded' }
        : {
            failure: { code: 'benchmark-report-failed', message: 'Recovered benchmark report is not successful' },
            status: 'failed'
          }
    })
  }

  if (task.action === 'restore') {
    return restoreCompletionEvidence(task)
  }

  if (task.action === 'security') {
    return (
      securityCompletionEvidence(task) ||
      jsonEvidence('security\\reports\\latest-scan.json', document =>
        String(document.status || '').startsWith('pass')
          ? { status: 'succeeded' }
          : {
              failure: { code: 'security-report-failed', message: 'Recovered security report records a failure' },
              status: 'failed'
            }
      )
    )
  }

  if (task.action === 'test') {
    return jsonEvidence('logs\\diagnostics\\latest-test.json', document =>
      document.passed === true
        ? { status: 'succeeded' }
        : {
            failure: { code: 'test-report-failed', message: 'Recovered diagnostics report records a failure' },
            status: 'failed'
          }
    )
  }

  if (task.action === 'update') {
    return updateCompletionEvidence(task)
  }

  if (task.action === 'backup') {
    return newestArchiveEvidence('backups', 'Hermes-Local-')
  }

  if (task.action === 'diagnostics') {
    return newestArchiveEvidence('logs\\diagnostics', 'Hermes-Local-Diagnostics-')
  }

  if (
    task.action === 'start' ||
    task.action === 'restart' ||
    task.action === 'repair' ||
    task.action === 'switch-model'
  ) {
    const runtime = safeReadJson<Record<string, any>>('data\\runtime\\status.json')
    const phase = String(runtime?.phase || '')

    const switchTargetMatches =
      task.action !== 'switch-model' ||
      (runtime?.selectedModelId === task.context.targetModelId && runtime?.model?.alias === task.context.targetAlias)

    if (
      runtime &&
      switchTargetMatches &&
      ['benchmark-preparing', 'benchmarking', 'running', 'starting-model'].includes(phase)
    ) {
      const evidence = fileEvidence('data\\runtime\\status.json', 'succeeded', null, 'runtime-state')

      return processAlive(runtime.controllerPid) ? evidence : null
    }
  }

  if (task.action === 'stop') {
    const runtime = safeReadJson<Record<string, any>>('data\\runtime\\status.json')

    if (!runtime || !processAlive(runtime.controllerPid)) {
      return {
        exitCode: 0,
        failure: null,
        observedAt: new Date().toISOString(),
        result: { kind: 'runtime-state', path: 'data/runtime/status.json' },
        status: 'succeeded'
      }
    }
  }

  return null
}

function reconcileTaskRegistry(): void {
  hydrateTaskRegistry()

  let changed = false
  const at = new Date().toISOString()

  for (const task of tasks.values()) {
    if (isTaskTerminal(task.status)) {
      continue
    }

    if (task.action === 'benchmark') {
      const progress = benchmarkProgress(task)
      const observedAt = Date.parse(String(progress?.updatedAt || ''))

      if (
        progress &&
        Number.isFinite(observedAt) &&
        observedAt > Date.parse(task.updatedAt) &&
        !['cancelled', 'failed', 'succeeded'].includes(String(progress.status || ''))
      ) {
        const summary = benchmarkProgressSummary(progress)
        const output = boundedTaskOutput(task.output, `\n${summary}\n`, MAX_TASK_OUTPUT)

        task.output = output.output
        task.outputTruncated ||= output.truncated
        task.stage = String(progress.stage || task.stage || '') || null
        task.updatedAt = new Date(observedAt).toISOString()
        changed = true
      }
    }

    if (task.action === 'security') {
      const progress = securityProgress(task)
      const observedAt = Date.parse(String(progress?.updatedAt || ''))

      if (progress && Number.isFinite(observedAt) && observedAt > Date.parse(task.updatedAt)) {
        const terminal = ['cancelled', 'failed', 'stale', 'succeeded'].includes(String(progress.status || ''))

        if (!terminal) {
          const summary = securityProgressSummary(progress)
          const output = boundedTaskOutput(task.output, `\n${summary}\n`, MAX_TASK_OUTPUT)

          task.output = output.output
          task.outputTruncated ||= output.truncated
        }

        task.progress = taskProgressFromSecurityDocument(progress)
        task.stage = String(progress.stage || task.stage || '') || null
        task.updatedAt = new Date(observedAt).toISOString()
        changed = true
      }
    }

    if (task.action === 'restore') {
      const progress = restoreProgress(task)
      const observedAt = Date.parse(String(progress?.updatedAt || ''))

      if (progress && Number.isFinite(observedAt) && observedAt > Date.parse(task.updatedAt)) {
        const terminal = ['cancelled', 'failed', 'succeeded'].includes(String(progress.status || ''))

        if (!terminal) {
          const summary = restoreProgressSummary(progress)
          const output = boundedTaskOutput(task.output, `\n${summary}\n`, MAX_TASK_OUTPUT)

          task.output = output.output
          task.outputTruncated ||= output.truncated
        }

        task.progress = taskProgressFromRestoreDocument(progress)
        task.stage = String(progress.stage || task.stage || '') || null
        task.updatedAt = new Date(observedAt).toISOString()
        changed = true
      }
    }

    if (task.action === 'update') {
      const operation = updateOperationForTask(task)
      const observedAt = Date.parse(String(operation?.updatedAt || ''))

      if (
        operation &&
        Number.isFinite(observedAt) &&
        observedAt > Date.parse(task.updatedAt) &&
        !['failed', 'rolled-back', 'succeeded'].includes(String(operation.status || ''))
      ) {
        const summary = updateOperationSummary(operation)
        const output = boundedTaskOutput(task.output, `\n${summary}\n`, MAX_TASK_OUTPUT)

        task.output = output.output
        task.outputTruncated ||= output.truncated
        task.stage = String(operation.currentStage || task.stage || '') || null
        task.updatedAt = new Date(observedAt).toISOString()
        changed = true
      }
    }

    if (task.child) {
      continue
    }

    if (task.action === 'model-download') {
      const progress = modelDownloadProgress(task)
      const observedAt = Date.parse(String(progress?.updatedAt || ''))

      if (progress && Number.isFinite(observedAt) && observedAt >= Date.parse(task.updatedAt)) {
        task.progress = taskProgressFromModelDownload(progress)
        task.stage = String(progress.stage || task.stage || '') || null
        const summary = modelDownloadProgressSummary(progress)
        const output = boundedTaskOutput(task.output, `\n${summary}\n`, MAX_TASK_OUTPUT)
        task.output = output.output
        task.outputTruncated ||= output.truncated
        task.updatedAt = new Date(observedAt).toISOString()
        if (isPausedModelDownload(progress) && task.status !== 'paused') {
          replaceTaskRecord(
            task,
            transitionTask(task, 'paused', task.updatedAt, {
              owner: { kind: 'desktop-child-process', pid: null }
            })
          )
        }
        changed = true
      }
    }

    const recovered = reconcileRecoveredTask(
      task,
      processAlive(task.owner.pid),
      taskCompletionEvidence(task),
      at
    )

    if (recovered !== task) {
      replaceTaskRecord(task, recovered)
      changed = true

      if (isTaskTerminal(task.status)) {
        task.events.emit('terminal', task)
      }
    }
  }

  if (changed) {
    pruneCompletedTasks()
    drainQueuedTasks()
    persistTaskRegistrySafely()
  }
}

function hydrateTaskRegistry(): void {
  if (taskRegistryLoaded) {
    return
  }

  taskRegistryLoaded = true
  const loaded = loadTaskStore(taskStorePath(), MAX_TASK_OUTPUT, MAX_COMPLETED_TASKS)

  for (const warning of loaded.warnings) {
    console.warn(warning)
  }

  for (const record of loaded.records) {
    tasks.set(record.id, {
      ...record,
      child: null,
      events: new EventEmitter(),
      input: { ...record.context }
    })
  }
}

function startTaskReconciliation(): void {
  hydrateTaskRegistry()
  reconcileTaskRegistry()

  if (taskReconcileTimer) {
    return
  }

  taskReconcileTimer = setInterval(() => {
    try {
      reconcileTaskRegistry()
    } catch (error) {
      console.error('Failed to reconcile Hermes Local task registry', error)
    }
  }, TASK_RECONCILE_INTERVAL_MS)
  taskReconcileTimer.unref()
}

function completeTask(
  task: ActionTask,
  status: 'cancelled' | 'failed' | 'interrupted' | 'succeeded',
  exitCode: null | number,
  failure: TaskRecord['failure'] = null,
  result: TaskRecord['result'] = null
): void {
  replaceTaskRecord(
    task,
    transitionTask(task, status, new Date().toISOString(), {
      exitCode,
      failure,
      result
    })
  )
  task.events.emit('terminal', task)
  pruneCompletedTasks()
  drainQueuedTasks()
  flushScheduledTaskPersistence()
}

function actionArguments(action: ActionName, input: Record<string, unknown>): string[] {
  const updatePlan = action === 'update' ? planDesktopUpdateAction(input, process.pid) : null
  const scriptRelative = updatePlan?.scriptRelative || ACTION_SCRIPTS[action]
  const scriptPath = resolveUnderRoot(scriptRelative)

  if (!fs.existsSync(scriptPath)) {
    throw new Error(`${scriptRelative} is not installed`)
  }

  const args = ['-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', scriptPath]

  if (action === 'start' || action === 'restart') {
    const configuredProfile = readLocalConfiguration(localRoot(), os.cpus().length).selectedProfile

    args.push('-Profile', validProfileName(input.profile || configuredProfile))
  }

  if (action === 'model-download') {
    const sourceUrl = String(input.sourceUrl || '').trim()
    const repository = String(input.repository || '').trim()
    const revision = String(input.revision || '').trim()
    const modelId = String(input.modelId || '').trim()
    const displayName = String(input.displayName || '').trim()
    const alias = String(input.alias || '').trim()
    const filename = String(input.filename || '').trim()
    const targetRelativePath = String(input.targetRelativePath || '').replaceAll('/', '\\').trim()
    const sha256 = String(input.sha256 || '').trim().toLowerCase()
    const sizeBytes = Number(input.sizeBytes || 0)
    const license = String(input.license || '').trim()
    const partialRetention = input.partialRetention === 'discard' ? 'discard' : 'keep'
    const auxiliaryFilesJson = typeof input.auxiliaryFilesJson === 'string'
      ? input.auxiliaryFilesJson
      : JSON.stringify(Array.isArray(input.auxiliaryFiles) ? input.auxiliaryFiles : [])
    const parsedUrl = new URL(sourceUrl)

    if (
      parsedUrl.protocol !== 'https:' ||
      parsedUrl.username ||
      parsedUrl.password ||
      parsedUrl.search ||
      parsedUrl.hash
    ) {
      throw new Error('Model downloads require a public HTTPS URL without embedded credentials or query secrets')
    }
    if (!/^[a-z0-9][a-z0-9._-]{0,63}$/.test(modelId) || !/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(alias)) {
      throw new Error('Model download id or alias is invalid')
    }
    if (!displayName || !filename.endsWith('.gguf') || !targetRelativePath.toLowerCase().startsWith('models\\')) {
      throw new Error('Model download requires a managed GGUF target under models')
    }
    if (sha256 && !/^[0-9a-f]{64}$/.test(sha256)) {
      throw new Error('Model download SHA-256 is invalid')
    }

    args.push(
      '-SourceUrl', sourceUrl,
      '-Repository', repository,
      '-Revision', revision,
      '-ModelId', modelId,
      '-DisplayName', displayName,
      '-Alias', alias,
      '-Filename', filename,
      '-TargetRelativePath', targetRelativePath,
      '-AuxiliaryFilesJson', auxiliaryFilesJson,
      '-PartialRetention', partialRetention
    )
    if (sha256) {args.push('-Sha256', sha256)}
    if (Number.isSafeInteger(sizeBytes) && sizeBytes > 0) {args.push('-SizeBytes', String(sizeBytes))}
    if (license) {args.push('-License', license)}
    if (input.requiresConsent === true || input.requiresConsent === 'true') {args.push('-RequiresConsent')}
    if (input.consentConfirmed === true || input.consentConfirmed === 'true') {args.push('-ConsentConfirmed')}
  }

  if (action === 'switch-model') {
    const targetModelId = String(input.targetModelId || '')
    const previousModelId = String(input.previousModelId || '')
    const profile = validProfileName(input.profile || readLocalConfiguration(localRoot(), os.cpus().length).selectedProfile)

    if (!targetModelId || !previousModelId) {
      throw new Error('Model switch requires targetModelId and previousModelId')
    }

    args.push('-TargetModelId', targetModelId, '-PreviousModelId', previousModelId, '-Profile', profile)
  }

  if (action === 'restore') {
    const backupPath = String(input.backupPath || '').replaceAll('\\', '/').trim()
    const backupId = String(input.backupId || '').trim()

    if (!/^backups\/[^/]+\.zip$/i.test(backupPath) || backupPath.includes('..')) {
      throw new Error('Restore requires a backup selected from the managed backups directory')
    }
    if (backupId && !/^[0-9a-f]{16}$/i.test(backupId)) {
      throw new Error('Restore backup identity must be a 16-character SHA-256 prefix')
    }

    const archivePath = resolveUnderRoot(backupPath.replaceAll('/', '\\'))
    if (!fs.existsSync(archivePath) || !fs.statSync(archivePath).isFile()) {
      throw new Error('Selected restore archive is no longer available')
    }

    const sidecarPath = `${archivePath}.sha256`
    const sidecar = fs.existsSync(sidecarPath) ? fs.readFileSync(sidecarPath, 'utf8').trim().split(/\s+/)[0] : ''
    if (!/^[0-9a-f]{64}$/i.test(sidecar) || (backupId && !sidecar.toLowerCase().startsWith(backupId.toLowerCase()))) {
      throw new Error('Selected restore archive identity or integrity evidence is invalid')
    }

    args.push('-BackupPath', backupPath)
  }

  if (action === 'security') {
    if (input.quick === true) {
      args.push('-Quick')
    }
    if (input.skipDefender === true) {
      args.push('-SkipDefender')
    }
  }

  if (action === 'update') {
    args.push(...(updatePlan?.arguments || []))
  }

  args.push('-NonInteractive')

  return args
}

function spawnActionTask(task: ActionTask): void {
  try {
    const child = spawn(powershellExecutable(), actionArguments(task.action, task.input), {
      cwd: localRoot(),
      env: {
        ...process.env,
        HERMES_LOCAL_ROOT: localRoot(),
        HERMES_LOCAL_TASK_ID: task.id
      },
      shell: false,
      windowsHide: true
    })

    task.child = child
    replaceTaskRecord(
      task,
      transitionTask(task, 'running', new Date().toISOString(), {
        owner: { kind: 'desktop-child-process', pid: child.pid ?? null }
      })
    )
    persistTaskRegistrySafely()
    child.stdout.on('data', chunk => appendTaskOutput(task, chunk))
    child.stderr.on('data', chunk => appendTaskOutput(task, chunk))
    child.on('error', error => appendTaskOutput(task, `\n${error.message}\n`))
    child.on('close', code => {
      if (
        task.action === 'update' &&
        task.context.component === 'HermesLocal' &&
        task.owner.kind === 'external-process' &&
        task.owner.pid &&
        code === 0
      ) {
        task.child = null
        task.stage = 'waiting-for-restart'
        task.updatedAt = new Date().toISOString()
        scheduleTaskRegistryPersistence()
        return
      }

      if (task.action === 'security') {
        const progress = securityProgress(task)

        if (progress) {
          task.progress = taskProgressFromSecurityDocument(progress)
          task.stage = String(progress.stage || task.stage || '') || null
        }
      }

      if (task.action === 'restore') {
        const progress = restoreProgress(task)

        if (progress) {
          task.progress = taskProgressFromRestoreDocument(progress)
          task.stage = String(progress.stage || task.stage || '') || null
        }
      }

      if (task.action === 'model-download') {
        const progress = modelDownloadProgress(task)

        if (progress) {
          task.progress = taskProgressFromModelDownload(progress)
          task.stage = String(progress.stage || task.stage || '') || null
        }
        if (isPausedModelDownload(progress)) {
          replaceTaskRecord(
            task,
            transitionTask(task, 'paused', String(progress?.updatedAt || new Date().toISOString()), {
              exitCode: code,
              owner: { kind: 'desktop-child-process', pid: null }
            })
          )
          task.child = null
          flushScheduledTaskPersistence()
          return
        }
      }

      const evidence =
        task.action === 'model-download'
          ? modelDownloadCompletionEvidence(task, modelDownloadProgress(task))
          : task.action === 'update'
          ? updateCompletionEvidence(task)
          : task.action === 'security'
            ? securityCompletionEvidence(task)
            : task.action === 'restore'
              ? restoreCompletionEvidence(task)
              : null

      if (evidence) {
        completeTask(task, evidence.status, evidence.exitCode, evidence.failure, evidence.result)
      } else if (task.status === 'cancelling') {
        completeTask(task, 'cancelled', code)
      } else if (code === 0) {
        completeTask(task, 'succeeded', code)
      } else {
        completeTask(task, 'failed', code, {
          code: 'process-exit',
          message: `Action process exited with code ${code ?? 'unknown'}`
        })
      }
    })
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    appendTaskOutput(task, `\n${message}\n`)
    completeTask(task, 'failed', null, { code: 'spawn-failed', message })
  }
}

function drainQueuedTasks(): void {
  for (const task of tasks.values()) {
    if (task.status !== 'queued') {
      continue
    }

    const otherTasks = [...tasks.values()].filter(candidate => candidate.id !== task.id)
    const admission = admitTask(task.action, otherTasks, task.context)

    if (admission.kind === 'start') {
      spawnActionTask(task)
    }
  }
}

function pruneCompletedTasks(taskMap: Map<string, ActionTask> = tasks, maximum = MAX_COMPLETED_TASKS): void {
  let completed = [...taskMap.values()].filter(task => isTaskTerminal(task.status)).length

  if (completed <= maximum) {
    return
  }

  for (const [taskId, task] of taskMap) {
    if (isTaskTerminal(task.status)) {
      taskMap.delete(taskId)
      completed -= 1
    }

    if (completed <= maximum) {
      return
    }
  }
}

function taskContext(action: ActionName, input: Record<string, unknown>): Record<string, string> {
  if (action === 'model-download') {
    const auxiliaryFilesJson = typeof input.auxiliaryFilesJson === 'string'
      ? input.auxiliaryFilesJson
      : JSON.stringify(Array.isArray(input.auxiliaryFiles) ? input.auxiliaryFiles : [])
    const targetRelativePath = String(input.targetRelativePath || '').replaceAll('/', '\\').trim()

    return {
      alias: String(input.alias || ''),
      auxiliaryFilesJson,
      consentConfirmed: String(input.consentConfirmed === true || input.consentConfirmed === 'true'),
      displayName: String(input.displayName || ''),
      filename: String(input.filename || ''),
      license: String(input.license || ''),
      modelId: String(input.modelId || ''),
      partialRetention: input.partialRetention === 'discard' ? 'discard' : 'keep',
      repository: String(input.repository || ''),
      requiresConsent: String(input.requiresConsent === true || input.requiresConsent === 'true'),
      revision: String(input.revision || ''),
      sha256: String(input.sha256 || ''),
      sizeBytes: String(input.sizeBytes || ''),
      sourceUrl: String(input.sourceUrl || ''),
      targetIdentity: crypto
        .createHash('sha256')
        .update(path.resolve(localRoot(), targetRelativePath).toLowerCase())
        .digest('hex')
        .slice(0, 24),
      targetRelativePath
    }
  }

  if (action === 'update') {
    return desktopUpdateTaskContext(input)
  }

  if (action === 'restore') {
    return Object.fromEntries(
      ['backupId', 'backupPath', 'verifyIntegrity']
        .map(key => [key, String(input[key] ?? (key === 'verifyIntegrity' ? 'true' : ''))])
        .filter(([, value]) => value)
    )
  }

  if (action === 'security') {
    return {
      mode: input.quick === true ? 'quick' : 'full',
      defender: input.skipDefender === true ? 'skipped' : 'enabled'
    }
  }

  if (action !== 'switch-model') {
    return {}
  }

  return Object.fromEntries(
    ['previousModelId', 'profile', 'targetAlias', 'targetModelId']
      .map(key => [key, String(input[key] || '')])
      .filter(([, value]) => value)
  )
}

function startActionTask(actionValue: unknown, input: unknown) {
  reconcileTaskRegistry()

  const action = String(actionValue || '') as ActionName
  const scriptRelative = ACTION_SCRIPTS[action]

  if (!scriptRelative) {
    throw new Error('Unsupported Hermes Local action')
  }

  const payload = input && typeof input === 'object' ? (input as Record<string, unknown>) : {}
  actionArguments(action, payload)
  const context = taskContext(action, payload)
  const admission = admitTask(action, tasks.values(), context)

  if (admission.kind === 'join') {
    const existing = tasks.get(admission.taskId)

    if (!existing) {
      throw new Error(`Hermes Local task ${admission.taskId} disappeared during admission`)
    }

    return publicTask(existing)
  }

  if (admission.kind === 'reject') {
    throw new Error(admission.message)
  }

  const id = crypto.randomUUID()

  const record = createTaskRecord(
    action,
    id,
    { kind: 'desktop-child-process', pid: null },
    new Date().toISOString(),
    context
  )

  const task: ActionTask = { ...record, child: null, events: new EventEmitter(), input: payload }

  tasks.set(id, task)

  try {
    persistTaskRegistry()
  } catch (error) {
    tasks.delete(id)
    throw error
  }

  if (admission.kind === 'start') {
    spawnActionTask(task)
  }

  return publicTask(task)
}

function getActionTask(taskId: unknown) {
  reconcileTaskRegistry()

  const task = tasks.get(String(taskId || ''))

  if (!task) {
    throw new Error('Hermes Local task not found')
  }

  return publicTask(task)
}

function listActionTasks(): TaskView[] {
  reconcileTaskRegistry()

  return [...tasks.values()].map(task => publicTask(task))
}

function writeModelDownloadControl(task: ActionTask, action: 'cancel' | 'pause'): void {
  const controlPath = resolveUnderRoot(`data\\runtime\\model-download-controls\\${task.id}.json`)
  const temporaryPath = `${controlPath}.${process.pid}.${Date.now()}.tmp`
  fs.mkdirSync(path.dirname(controlPath), { recursive: true })
  fs.writeFileSync(
    temporaryPath,
    `${JSON.stringify({ schemaVersion: 1, taskId: task.id, action, requestedAt: new Date().toISOString(), requestedBy: 'desktop' }, null, 2)}\n`,
    'utf8'
  )
  fs.rmSync(controlPath, { force: true })
  fs.renameSync(temporaryPath, controlPath)
}

function pauseActionTask(taskId: unknown): TaskView {
  reconcileTaskRegistry()
  const task = tasks.get(String(taskId || ''))
  if (!task || task.action !== 'model-download' || !taskCapabilities(task).pause) {
    throw new Error('This task cannot be paused in its current stage')
  }
  writeModelDownloadControl(task, 'pause')
  appendTaskOutput(task, '\nPause requested. The backend will checkpoint the current partial file.\n')
  flushScheduledTaskPersistence()
  return publicTask(task)
}

function resumeActionTask(taskId: unknown): TaskView {
  reconcileTaskRegistry()
  const task = tasks.get(String(taskId || ''))
  if (!task || task.action !== 'model-download' || !taskCapabilities(task).resume) {
    throw new Error('This task cannot be resumed')
  }
  fs.rmSync(resolveUnderRoot(`data\\runtime\\model-download-controls\\${task.id}.json`), { force: true })
  task.input = { ...task.context }
  replaceTaskRecord(
    task,
    transitionTask(task, 'queued', new Date().toISOString(), {
      exitCode: null,
      owner: { kind: 'desktop-child-process', pid: null }
    })
  )
  const admission = admitTask(task.action, [...tasks.values()].filter(candidate => candidate.id !== task.id), task.context)
  if (admission.kind === 'start') {spawnActionTask(task)}
  flushScheduledTaskPersistence()
  return publicTask(task)
}

function cancelActionTask(taskId: unknown): TaskView {
  reconcileTaskRegistry()

  const task = tasks.get(String(taskId || ''))

  if (!task) {
    throw new Error('Hermes Local task not found')
  }

  if (!taskCapabilities(task).cancel) {
    throw new Error(`Task '${task.id}' cannot be safely cancelled in its current state`)
  }

  if (task.status === 'queued') {
    replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
    task.events.emit('terminal', task)
    pruneCompletedTasks()
    drainQueuedTasks()
    flushScheduledTaskPersistence()

    return publicTask(task)
  }

  if (!task.child || task.owner.kind !== 'desktop-child-process' || task.owner.pid !== task.child.pid) {
    throw new Error(`Task '${task.id}' no longer has a cancellable Desktop-owned process`)
  }

  if (task.action === 'benchmark') {
    const cancellationPath = resolveUnderRoot('data\\runtime\\benchmark-cancel.json')
    const temporaryPath = `${cancellationPath}.${process.pid}.${Date.now()}.tmp`
    const request = {
      schemaVersion: 1,
      taskId: task.id,
      ownerPid: task.owner.pid,
      requestedAt: new Date().toISOString(),
      requestedBy: 'desktop'
    }

    fs.mkdirSync(path.dirname(cancellationPath), { recursive: true })
    fs.writeFileSync(temporaryPath, `${JSON.stringify(request, null, 2)}\n`, 'utf8')
    fs.rmSync(cancellationPath, { force: true })
    fs.renameSync(temporaryPath, cancellationPath)
    replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
    appendTaskOutput(
      task,
      '\nCancellation requested. The active native case will finish before the benchmark restores the model stack.\n'
    )
    flushScheduledTaskPersistence()

    return publicTask(task)
  }

  if (task.action === 'security') {
    const cancellationPath = resolveUnderRoot('data\\runtime\\security-scan-cancel.json')
    const temporaryPath = `${cancellationPath}.${process.pid}.${Date.now()}.tmp`
    const request = {
      schemaVersion: 1,
      taskId: task.id,
      ownerPid: task.owner.pid,
      requestedAt: new Date().toISOString(),
      requestedBy: 'desktop'
    }

    fs.mkdirSync(path.dirname(cancellationPath), { recursive: true })
    fs.writeFileSync(temporaryPath, `${JSON.stringify(request, null, 2)}\n`, 'utf8')
    fs.rmSync(cancellationPath, { force: true })
    fs.renameSync(temporaryPath, cancellationPath)
    replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
    appendTaskOutput(
      task,
      '\nCancellation requested. The owned scanner will stop at its next safe polling boundary.\n'
    )
    flushScheduledTaskPersistence()

    return publicTask(task)
  }

  if (task.action === 'model-download') {
    if (task.status === 'paused') {
      if (task.context.partialRetention === 'discard') {
        const paths = [task.context.targetRelativePath]
        try {
          const auxiliary = JSON.parse(task.context.auxiliaryFilesJson || '[]')
          for (const entry of Array.isArray(auxiliary) ? auxiliary : []) {
            if (typeof entry?.targetRelativePath === 'string') {paths.push(entry.targetRelativePath)}
          }
        } catch { /* invalid auxiliary metadata was already rejected on admission */ }
        for (const relativePath of paths) {
          fs.rmSync(`${resolveUnderRoot(relativePath)}.partial`, { force: true })
        }
      }
      fs.rmSync(resolveUnderRoot(`data\\runtime\\model-download-locks\\${task.context.targetIdentity}.json`), { force: true })
      replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
      flushScheduledTaskPersistence()
      return publicTask(task)
    }
    writeModelDownloadControl(task, 'cancel')
    replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
    appendTaskOutput(task, '\nCancellation requested. Existing verified model files will remain untouched.\n')
    flushScheduledTaskPersistence()
    return publicTask(task)
  }

  if (task.action === 'restore') {
    const cancellationPath = resolveUnderRoot('data\\runtime\\restore-cancel.json')
    const temporaryPath = `${cancellationPath}.${process.pid}.${Date.now()}.tmp`
    const request = {
      schemaVersion: 1,
      taskId: task.id,
      ownerPid: task.owner.pid,
      requestedAt: new Date().toISOString(),
      requestedBy: 'desktop'
    }

    fs.mkdirSync(path.dirname(cancellationPath), { recursive: true })
    fs.writeFileSync(temporaryPath, `${JSON.stringify(request, null, 2)}\n`, 'utf8')
    fs.rmSync(cancellationPath, { force: true })
    fs.renameSync(temporaryPath, cancellationPath)
    replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
    appendTaskOutput(
      task,
      '\nCancellation requested. Restore will stop only before the destructive replacement boundary.\n'
    )
    flushScheduledTaskPersistence()

    return publicTask(task)
  }

  replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
  flushScheduledTaskPersistence()

  try {
    if (process.platform === 'win32' && task.owner.pid) {
      const taskkill = path.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'taskkill.exe')

      execFileSync(taskkill, ['/PID', String(task.owner.pid), '/T', '/F'], {
        stdio: 'ignore',
        timeout: 15_000,
        windowsHide: true
      })
    } else {
      task.child.kill('SIGTERM')
    }
  } catch (error) {
    if (processAlive(task.owner.pid)) {
      const message = error instanceof Error ? error.message : String(error)

      appendTaskOutput(task, `\nCancellation failed: ${message}\n`)
      completeTask(task, 'failed', null, { code: 'cancel-failed', message })
    }
  }

  return publicTask(task)
}

function retryActionTask(taskId: unknown): TaskView {
  reconcileTaskRegistry()

  const task = tasks.get(String(taskId || ''))

  if (!task) {
    throw new Error('Hermes Local task not found')
  }

  if (!taskCapabilities(task).retry) {
    throw new Error(`Task '${task.id}' is not ready to retry`)
  }

  return startActionTask(task.action, task.input)
}

async function openActionTaskResult(taskId: unknown) {
  reconcileTaskRegistry()

  const task = tasks.get(String(taskId || ''))

  if (!task?.result) {
    throw new Error('Hermes Local task has no result to open')
  }

  const resultPath = resolveUnderRoot(task.result.path)

  if (!fs.existsSync(resultPath)) {
    throw new Error(`Task result is no longer available: ${task.result.path}`)
  }

  const error = await shell.openPath(resultPath)

  return { error, ok: !error, path: task.result.path }
}

function waitForActionTask(task: ActionTask): Promise<ActionTask> {
  if (isTaskTerminal(task.status)) {
    return Promise.resolve(task)
  }

  return new Promise(resolve => {
    task.events.once('terminal', () => resolve(task))
  })
}

export async function ensureHermesLocalWorkstationReady(): Promise<void> {
  const started = startActionTask('start', null)
  const task = tasks.get(started.id)

  if (!task) {
    throw new Error('Hermes Local workstation start task was not created')
  }

  const completed = await waitForActionTask(task)

  if (completed.status !== 'succeeded') {
    const detail = completed.output.trim() || `exit code ${completed.exitCode ?? 'unknown'}`

    throw new Error(`Hermes Local workstation failed to start: ${detail}`)
  }
}

async function health(url: string): Promise<boolean> {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(2500) })

    return response.ok
  } catch {
    return false
  }
}

async function serviceHealth(modelBase: string, hermesBase: string, probe: (url: string) => Promise<boolean> = health) {
  const [modelHealthy, hermesHealthy, dashboardHealthy] = await Promise.all([
    probe(`${modelBase}/health`),
    probe(`${hermesBase}/api/health`),
    probe(`${hermesBase}/`)
  ])

  return {
    dashboard: dashboardHealthy,
    hermes: hermesHealthy,
    model: modelHealthy
  }
}

async function gpuSnapshot() {
  if (process.platform !== 'win32') {
    return null
  }

  try {
    const { stdout } = await execFileAsync(
      'nvidia-smi.exe',
      [
        '--query-gpu=name,memory.total,memory.used,memory.free,utilization.gpu,temperature.gpu,power.draw',
        '--format=csv,noheader,nounits'
      ],
      { timeout: 5000, windowsHide: true }
    )

    const values = stdout
      .trim()
      .split(',')
      .map(value => value.trim())

    if (values.length < 7) {
      return null
    }

    return {
      memoryFreeMiB: Number(values[3]) || 0,
      memoryTotalMiB: Number(values[1]) || 0,
      memoryUsedMiB: Number(values[2]) || 0,
      name: values[0],
      powerWatts: Number(values[6]) || 0,
      temperatureCelsius: Number(values[5]) || 0,
      utilizationPercent: Number(values[4]) || 0
    }
  } catch {
    return null
  }
}

function processAlive(pid: unknown): boolean {
  const value = Number(pid)

  if (!Number.isSafeInteger(value) || value <= 0) {
    return false
  }

  try {
    process.kill(value, 0)

    return true
  } catch {
    return false
  }
}

function loginItemExecutable(): string {
  const portableExecutable = process.env.PORTABLE_EXECUTABLE_FILE

  if (portableExecutable && path.isAbsolute(portableExecutable) && fs.existsSync(portableExecutable)) {
    return path.resolve(portableExecutable)
  }

  return process.execPath
}

function loginItemStatus() {
  if (process.platform !== 'win32') {
    return {
      available: false,
      enabled: false,
      executable: process.execPath
    }
  }

  const executable = loginItemExecutable()

  const settings = app.getLoginItemSettings({
    args: LOGIN_ITEM_ARGUMENTS,
    path: executable
  })

  return {
    available: true,
    enabled: Boolean(settings.openAtLogin),
    executable
  }
}

function setLoginItem(enabledValue: unknown) {
  if (typeof enabledValue !== 'boolean') {
    throw new Error('Launch-at-login state must be a boolean')
  }

  if (process.platform !== 'win32') {
    throw new Error('Launch at login is only available in the Windows workstation')
  }

  const executable = loginItemExecutable()

  app.setLoginItemSettings({
    args: LOGIN_ITEM_ARGUMENTS,
    openAtLogin: enabledValue,
    path: executable
  })

  return loginItemStatus()
}

async function buildWorkstationSnapshot() {
  reconcileTaskRegistry()

  const state = readJson<Record<string, any>>('data\\runtime\\status.json')
  const version = readInstalledVersion()
  const gpu = await gpuSnapshot()
  const latestUpdate = updateOperationDocument()
  const patchSeries = String(version?.sources?.hermesAgent?.patchSeries || 'source/hermes-launcher/patches')
  const patchDirectory = resolveUnderRoot(patchSeries)
  const patchCount = fs.existsSync(patchDirectory)
    ? fs.readdirSync(patchDirectory, { withFileTypes: true }).filter(entry => entry.isFile() && entry.name.endsWith('.patch'))
        .length
    : 0
  const configuration = readLocalConfiguration(localRoot(), os.cpus().length, gpu?.memoryTotalMiB || 0)
  const modelBase = `http://${configuration.network.host}:${configuration.network.modelPort}`
  const hermesBase = `http://${configuration.network.host}:${configuration.network.hermesPort}`

  const [snapshotHealth, gateway] = await Promise.all([
    serviceHealth(modelBase, hermesBase),
    readGatewayStatus(`${hermesBase}/api/status`)
  ])

  const actions = Object.fromEntries(
    Object.entries(ACTION_SCRIPTS).map(([name, relativePath]) => [name, fs.existsSync(resolveUnderRoot(relativePath))])
  )

  const identityMatches = runtimeModelIdentityMatches({
    configuredAlias: configuration.selectedModel.alias,
    configuredModelId: configuration.selectedModelId,
    runtimeAlias: state?.model?.alias,
    runtimeModelId: state?.selectedModelId
  })

  const switchingTask = activeModelSwitch([...tasks.values()].map(task => publicTask(task)))

  return {
    actions,
    backups: restoreBackups(),
    generatedAt: new Date().toISOString(),
    hardware: {
      cpu: os.cpus()[0]?.model || 'Unknown CPU',
      logicalProcessors: os.cpus().length,
      memoryFreeBytes: os.freemem(),
      memoryTotalBytes: os.totalmem()
    },
    health: {
      dashboard: snapshotHealth.dashboard,
      gateway,
      hermes: snapshotHealth.hermes,
      model: snapshotHealth.model && identityMatches
    },
    lifecycle: {
      identityMatches,
      switchingModel: switchingTask
        ? {
            previousModelId: switchingTask.context.previousModelId || '',
            stage: switchingTask.stage,
            targetAlias: switchingTask.context.targetAlias || '',
            targetModelId: switchingTask.context.targetModelId || '',
            taskId: switchingTask.id
          }
        : null
    },
    model: configuration.selectedModel,
    models: configuration.models,
    profiles: {
      profiles: configuration.profiles,
      schemaVersion: 1,
      selected: configuration.selectedProfile
    },
    reports: {
      benchmark: fs.existsSync(resolveUnderRoot('benchmarks\\reports\\LATEST.md')),
      security: fs.existsSync(resolveUnderRoot('security\\reports\\SECURITY_REPORT.md'))
    },
    root: localRoot(),
    storage: {
      memoryFiles: fs.existsSync(resolveUnderRoot('data\\memory'))
        ? fs.readdirSync(resolveUnderRoot('data\\memory'), { withFileTypes: true }).filter(entry => entry.isFile())
            .length
        : 0,
      stateDatabaseBytes: fs.existsSync(resolveUnderRoot('data\\hermes\\state.db'))
        ? fs.statSync(resolveUnderRoot('data\\hermes\\state.db')).size
        : 0
    },
    startup: loginItemStatus(),
    settings: {
      autoTuning: configuration.autoTuning,
      network: configuration.network,
      runtime: configuration.runtime,
      selectedModelId: configuration.selectedModelId,
      selectedProfile: configuration.selectedProfile
    },
    taskLedger: fs.existsSync(resolveUnderRoot('TASKS.md'))
      ? fs.readFileSync(resolveUnderRoot('TASKS.md'), 'utf8').slice(0, 128 * 1024)
      : '',
    tasks: [...tasks.values()].map(task => publicTask(task)),
    runtime: {
      ...state,
      controllerAlive: processAlive(state?.controllerPid),
      hermesAlive: processAlive(state?.hermes?.pid),
      modelAlive: processAlive(state?.model?.pid),
      identityMismatch:
        state?.phase === 'running' && !identityMatches
          ? `Configured model '${configuration.selectedModel.alias}' does not match the active runtime`
          : null
    },
    updates: {
      installed: {
        baseCommit: version?.sources?.hermesAgent?.commit ? String(version.sources.hermesAgent.commit) : null,
        harnessCommit: version?.sources?.hermesAgent?.harnessCommit
          ? String(version.sources.hermesAgent.harnessCommit)
          : null,
        harnessTree: version?.sources?.hermesAgent?.harnessTree
          ? String(version.sources.hermesAgent.harnessTree)
          : null,
        patchCount
      },
      latest: publicUpdateOperation(latestUpdate)
    },
    version,
    gpu
  }
}

let workstationSnapshotPromise: null | ReturnType<typeof buildWorkstationSnapshot> = null

function workstationSnapshot() {
  workstationSnapshotPromise ??= buildWorkstationSnapshot().finally(() => {
    workstationSnapshotPromise = null
  })

  return workstationSnapshotPromise
}

function readLog(nameValue: unknown, requestedLines: unknown) {
  const name = String(nameValue || '') as LogName
  const relativePath = LOG_FILES[name]

  if (!relativePath) {
    throw new Error('Unsupported log')
  }

  const filePath = resolveUnderRoot(relativePath)

  if (!fs.existsSync(filePath)) {
    return { content: '', name, path: filePath }
  }

  const stats = fs.statSync(filePath)
  const length = Math.min(stats.size, MAX_LOG_BYTES)
  const handle = fs.openSync(filePath, 'r')
  const buffer = Buffer.alloc(length)

  try {
    fs.readSync(handle, buffer, 0, length, Math.max(0, stats.size - length))
  } finally {
    fs.closeSync(handle)
  }

  const lineCount = Math.min(2000, Math.max(20, Number(requestedLines) || 400))
  const content = redacted(buffer.toString('utf8').split(/\r?\n/).slice(-lineCount).join('\n'))

  return { content, name, path: filePath }
}

export function hermesLocalTuiLaunch() {
  const root = localRoot()
  const scriptPath = resolveUnderRoot('scripts\\launch\\Start-Hermes-Tui.ps1')

  if (!fs.existsSync(scriptPath)) {
    throw new Error('Hermes Local TUI launcher is not installed')
  }

  const command = powershellExecutable()

  return {
    args: ['-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', scriptPath],
    command,
    cwd: resolveUnderRoot('data\\user'),
    env: {
      ...process.env,
      HERMES_HOME: resolveUnderRoot('data\\hermes'),
      HERMES_LOCAL_ROOT: root
    },
    name: 'Hermes TUI'
  }
}

export function configureHermesLocalDesktopEnvironment(): boolean {
  if (process.platform !== 'win32') {
    return false
  }

  const root = localRoot()
  const tokenScript = resolveUnderRoot('scripts\\launch\\Get-Hermes-Local-Token.ps1')
  const hermesHome = resolveUnderRoot('data\\hermes')
  const sourceRoot = resolveUnderRoot('source\\hermes-agent')

  if (!fs.existsSync(tokenScript) || !fs.existsSync(sourceRoot)) {
    return false
  }

  process.env.HERMES_LOCAL_ROOT = root
  process.env.HERMES_HOME = hermesHome
  process.env.HERMES_DESKTOP_CWD = resolveUnderRoot('data\\user')
  process.env.HERMES_DESKTOP_HERMES_ROOT = sourceRoot

  if (!process.env.HERMES_DESKTOP_REMOTE_URL) {
    const configuration = readLocalConfiguration(root, os.cpus().length)
    const pwsh = powershellExecutable()

    const token = execFileSync(
      pwsh,
      ['-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', tokenScript],
      {
        encoding: 'utf8',
        maxBuffer: 4096,
        timeout: 10_000,
        windowsHide: true
      }
    ).trim()

    if (!/^[A-Za-z0-9_-]{40,128}$/.test(token)) {
      throw new Error('Hermes Local returned an invalid protected session token')
    }

    process.env.HERMES_DESKTOP_REMOTE_TOKEN = token
    process.env.HERMES_DESKTOP_REMOTE_URL = `http://${configuration.network.host}:${configuration.network.hermesPort}`
  }

  return true
}

function dashboardConfiguration() {
  const configuration = readLocalConfiguration(localRoot(), os.cpus().length)

  const host = configuration.network.host.includes(':')
    ? `[${configuration.network.host.replace(/^\[|\]$/g, '')}]`
    : configuration.network.host

  const url = normalizeHermesLocalDashboardUrl(`http://${host}:${configuration.network.hermesPort}`)
  const token = String(process.env.HERMES_DESKTOP_REMOTE_TOKEN || '').trim()

  return { token, url: url.toString() }
}

function dashboardBoundsAtZoom(
  bounds: HermesLocalDashboardBounds,
  zoomFactor: number
): HermesLocalDashboardBounds {
  const factor = Number.isFinite(zoomFactor) && zoomFactor > 0 ? zoomFactor : 1
  const left = Math.round(bounds.x * factor)
  const top = Math.round(bounds.y * factor)
  const right = Math.round((bounds.x + bounds.width) * factor)
  const bottom = Math.round((bounds.y + bounds.height) * factor)

  return {
    height: bottom - top,
    width: right - left,
    x: left,
    y: top
  }
}

function requireDashboardSender(
  dashboardView: HermesLocalDashboardViewController | undefined,
  sender: Electron.WebContents
): HermesLocalDashboardViewController {
  if (!dashboardView || !dashboardView.isTrustedSender(sender)) {
    throw new Error('Dashboard controls are available only to the primary Desktop renderer')
  }

  return dashboardView
}

async function waitForDesktopUpdateTask(taskId: string, stopAtHandoff: boolean): Promise<ActionTask> {
  for (;;) {
    reconcileTaskRegistry()
    const task = tasks.get(taskId)

    if (!task) {
      throw new Error('Hermes Local update task disappeared')
    }
    if (isTaskTerminal(task.status) || (stopAtHandoff && task.owner.kind === 'external-process')) {
      return task
    }

    await new Promise(resolve => setTimeout(resolve, 150))
  }
}

export async function checkHermesLocalDesktopUpdates() {
  const started = startActionTask('update', {
    channel: 'development',
    component: 'HermesLocal',
    mode: 'Check'
  })
  const task = await waitForDesktopUpdateTask(started.id, false)

  if (task.desktopUpdateStatus) {
    return task.desktopUpdateStatus
  }

  return {
    supported: true,
    branch: 'main',
    behind: 0,
    updateAvailable: false,
    commits: [],
    error: 'check-failed',
    message: task.failure?.message || 'Hermes Local update check did not return authoritative metadata.',
    fetchedAt: Date.now()
  }
}

export async function applyHermesLocalDesktopUpdate(payload: Record<string, unknown>) {
  const started = startActionTask('update', {
    channel: String(payload.channel || 'development'),
    component: 'HermesLocal',
    mode: 'Apply',
    targetCommit: payload.targetCommit
  })
  const task = await waitForDesktopUpdateTask(started.id, true)

  if (task.owner.kind === 'external-process') {
    return {
      ok: true,
      handedOff: true,
      message: 'The verified update is staged. Hermes Local will close, install, validate and relaunch.',
      taskId: task.id
    }
  }

  if (task.desktopUpdateResult) {
    return task.desktopUpdateResult
  }

  return {
    ok: task.status === 'succeeded',
    handedOff: false,
    error: task.failure?.code || (task.status === 'succeeded' ? undefined : 'desktop-update-failed'),
    message: task.failure?.message || (task.status === 'succeeded' ? 'Hermes Local is already up to date.' : 'Hermes Local update failed.'),
    taskId: task.id
  }
}

export function isHermesLocalModelSwitchActive(): boolean {
  reconcileTaskRegistry()

  return Boolean(activeModelSwitch([...tasks.values()].map(task => publicTask(task))))
}

function runtimeStackRunning(state: null | Record<string, any>): boolean {
  return Boolean(
    state &&
      processAlive(state.controllerPid) &&
      ['benchmark-preparing', 'benchmarking', 'running', 'starting-model'].includes(String(state.phase || ''))
  )
}

function assertModelStorageAvailable(): void {
  const active = [...tasks.values()].find(
    task => task.action === 'model-download' && ['cancelling', 'paused', 'queued', 'running'].includes(task.status)
  )
  if (active) {
    throw new Error(`Model storage is owned by download task '${active.id}'`)
  }
}

function selectManagedModel(idValue: unknown) {
  reconcileTaskRegistry()
  assertModelStorageAvailable()

  const targetModelId = String(idValue || '').trim()
  const configuration = readLocalConfiguration(localRoot(), os.cpus().length)
  const target = configuration.models.find(model => model.id === targetModelId)

  if (!target) {
    throw new Error(`Model '${targetModelId}' is not registered`)
  }

  if (!target.installed) {
    throw new Error(`Model '${target.displayName}' is not installed at '${target.resolvedPath}'`)
  }

  const currentTask = activeModelSwitch([...tasks.values()].map(task => publicTask(task)))
  const state = safeReadJson<Record<string, any>>('data/runtime/status.json')

  const plan = planModelSelection({
    activeTask: currentTask,
    currentModelId: configuration.selectedModelId,
    runtimeRunning: runtimeStackRunning(state),
    targetModelId
  })

  if (plan.kind === 'reject') {
    throw new Error(plan.message)
  }

  if (plan.kind === 'join') {
    const task = tasks.get(plan.taskId)

    if (!task) {
      throw new Error(`Model switch task '${plan.taskId}' disappeared during admission`)
    }

    return { id: targetModelId, mode: 'joined', task: publicTask(task) }
  }

  if (plan.kind === 'unchanged') {
    return { id: targetModelId, mode: 'unchanged', task: null }
  }

  if (plan.kind === 'persist') {
    selectModel(localRoot(), targetModelId, os.cpus().length)

    return { id: targetModelId, mode: 'selected', task: null }
  }

  const task = startActionTask('switch-model', {
    previousModelId: configuration.selectedModelId,
    profile: configuration.selectedProfile,
    targetAlias: target.alias,
    targetModelId
  })

  return { id: targetModelId, mode: 'switching', task }
}

export function registerHermesLocalControlIpc(
  dashboardView?: HermesLocalDashboardViewController
): void {
  if (process.env.HERMES_LOCAL_ROOT) {
    startTaskReconciliation()
  }

  ipcMain.handle('hermes:local:snapshot', workstationSnapshot)
  ipcMain.handle('hermes:local:action:start', (_event, action, input) => startActionTask(action, input))
  ipcMain.handle('hermes:local:action:status', (_event, taskId) => getActionTask(taskId))
  ipcMain.handle('hermes:local:action:list', listActionTasks)
  ipcMain.handle('hermes:local:action:cancel', (_event, taskId) => cancelActionTask(taskId))
  ipcMain.handle('hermes:local:action:pause', (_event, taskId) => pauseActionTask(taskId))
  ipcMain.handle('hermes:local:action:resume', (_event, taskId) => resumeActionTask(taskId))
  ipcMain.handle('hermes:local:action:retry', (_event, taskId) => retryActionTask(taskId))
  ipcMain.handle('hermes:local:action:open-result', (_event, taskId) => openActionTaskResult(taskId))
  ipcMain.handle('hermes:local:logs', (_event, name, lines) => readLog(name, lines))
  ipcMain.handle('hermes:local:login-item:get', loginItemStatus)
  ipcMain.handle('hermes:local:login-item:set', (_event, enabled) => setLoginItem(enabled))
  ipcMain.handle('hermes:local:profile:save', (_event, profile, originalName) =>
    saveProfile(localRoot(), profile, os.cpus().length, 0, originalName)
  )
  ipcMain.handle('hermes:local:profile:delete', (_event, name) => deleteProfile(localRoot(), name, os.cpus().length))
  ipcMain.handle('hermes:local:profile:select', (_event, name) => selectProfile(localRoot(), name, os.cpus().length))
  ipcMain.handle('hermes:local:model:register', (_event, model) => {
    assertModelStorageAvailable()
    return registerModel(localRoot(), model)
  })
  ipcMain.handle('hermes:local:model:remove', (_event, id) => {
    assertModelStorageAvailable()
    return removeModel(localRoot(), id, os.cpus().length)
  })
  ipcMain.handle('hermes:local:model:select', (_event, id) => selectManagedModel(id))
  ipcMain.handle('hermes:local:settings:save', (_event, settings) =>
    saveWorkstationSettings(localRoot(), settings, os.cpus().length)
  )
  ipcMain.handle('hermes:local:dashboard:state', event => {
    return requireDashboardSender(dashboardView, event.sender).getState()
  })
  ipcMain.handle('hermes:local:dashboard:show', (event, bounds: HermesLocalDashboardBounds) => {
    const controller = requireDashboardSender(dashboardView, event.sender)
    const nativeBounds = dashboardBoundsAtZoom(bounds, event.sender.getZoomFactor())

    return controller.show(dashboardConfiguration(), nativeBounds)
  })
  ipcMain.handle('hermes:local:dashboard:resize', (event, bounds: HermesLocalDashboardBounds) => {
    const controller = requireDashboardSender(dashboardView, event.sender)
    const nativeBounds = dashboardBoundsAtZoom(bounds, event.sender.getZoomFactor())

    return controller.resize(nativeBounds)
  })
  ipcMain.handle('hermes:local:dashboard:hide', event => {
    return requireDashboardSender(dashboardView, event.sender).hide()
  })
  ipcMain.handle('hermes:local:dashboard:reload', event => {
    return requireDashboardSender(dashboardView, event.sender).reload(dashboardConfiguration())
  })
  ipcMain.handle('hermes:local:dashboard:open', async event => {
    requireDashboardSender(dashboardView, event.sender)
    const { url } = dashboardConfiguration()

    await shell.openExternal(url)

    return { url }
  })
  ipcMain.handle('hermes:local:path:open', (_event, relativePath) => openLocalPath(relativePath))
  registerHermesLocalTrustCentreIpc()
  ipcMain.handle('hermes:local:root:open', async () => {
    const error = await shell.openPath(localRoot())

    return { error, ok: !error }
  })
}

export const hermesLocalControlTest = {
  configureHermesLocalDesktopEnvironment,
  dashboardBoundsAtZoom,
  localRoot,
  loginItemExecutable,
  loginItemStatus,
  powershellExecutable,
  pruneCompletedTasks,
  redacted,
  resolveUnderRoot,
  sanitizeEditableProfile,
  serviceHealth,
  setLoginItem,
  actionArguments,
  publicUpdateOperation,
  modelDownloadCompletionEvidence,
  modelDownloadProgress,
  modelDownloadProgressSummary,
  pauseActionTask,
  resumeActionTask,
  securityCompletionEvidence,
  securityProgressSummary,
  restoreCompletionEvidence,
  restoreProgressSummary,
  taskCompletionEvidence,
  taskProgressFromSecurityDocument,
  taskProgressFromRestoreDocument,
  updateOperationSummary,
  waitForActionTask
}
