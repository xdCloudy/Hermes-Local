import type { TaskCompletionEvidence, TaskProgress, TaskRecord } from './hermes-local-task-model'

export interface ModelDownloadProgressDocument {
  completedAt?: null | string
  failure?: null | { code?: unknown; message?: unknown }
  message?: unknown
  progress?: null | Record<string, unknown>
  result?: null | Record<string, unknown>
  stage?: unknown
  status?: unknown
  taskId?: unknown
  updatedAt?: unknown
}

function finite(value: unknown, minimum = 0): null | number {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed >= minimum ? parsed : null
}

function text(value: unknown, maximum: number): null | string {
  if (typeof value !== 'string') {
    return null
  }
  return value.slice(0, maximum)
}

export function modelDownloadProgressPath(taskId: string): string {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{7,127}$/.test(taskId)) {
    throw new Error('Invalid model download task id')
  }
  return `data/runtime/model-downloads/${taskId}.json`
}

export function taskProgressFromModelDownload(document: ModelDownloadProgressDocument): TaskProgress {
  const source = document.progress && typeof document.progress === 'object' ? document.progress : {}
  const bytesCompleted = finite(source.bytesCompleted)
  const bytesTotal = finite(source.bytesTotal, 1)
  const percent = finite(source.percent)
  const countersValue = source.counters && typeof source.counters === 'object' ? source.counters : {}
  const counters = Object.fromEntries(
    Object.entries(countersValue)
      .filter(([key, value]) => key.length <= 64 && finite(value) !== null)
      .map(([key, value]) => [key, Number(value)])
  )

  return {
    bytesCompleted,
    bytesTotal,
    cancellable: source.cancellable !== false,
    completedUnits: bytesCompleted,
    counters,
    etaSeconds: finite(source.etaSeconds),
    message: text(document.message, 2048),
    mode: source.mode === 'determinate' && bytesTotal !== null ? 'determinate' : 'indeterminate',
    pauseSupported: source.pauseSupported === true,
    percent: percent === null ? null : Math.min(100, percent),
    rateBytesPerSecond: finite(source.rateBytesPerSecond),
    resumeSupported: source.resumeSupported === true,
    totalUnits: bytesTotal
  }
}

export function modelDownloadCompletionEvidence(
  task: Pick<TaskRecord, 'id'>,
  document: null | ModelDownloadProgressDocument
): null | TaskCompletionEvidence {
  if (!document || document.taskId !== task.id) {
    return null
  }
  const status = String(document.status || '')
  if (!['cancelled', 'failed', 'succeeded'].includes(status)) {
    return null
  }
  const result = document.result && typeof document.result === 'object' ? document.result : {}
  const report = text(result.report, 2048) || `logs/model-downloads/${task.id}.json`
  const observedAt = text(document.completedAt || document.updatedAt, 128) || new Date().toISOString()

  if (status === 'succeeded') {
    return { exitCode: 0, failure: null, observedAt, result: { kind: 'report', path: report }, status: 'succeeded' }
  }
  if (status === 'cancelled') {
    return { exitCode: 130, failure: null, observedAt, result: { kind: 'report', path: report }, status: 'cancelled' }
  }
  return {
    exitCode: 1,
    failure: {
      code: text(document.failure?.code, 128) || 'model-download-failed',
      message: text(document.failure?.message, 2048) || 'Model download failed'
    },
    observedAt,
    result: { kind: 'report', path: report },
    status: 'failed'
  }
}

export function isPausedModelDownload(document: null | ModelDownloadProgressDocument): boolean {
  return document?.status === 'paused'
}

export function activeModelDownloadForTarget(
  tasks: Iterable<TaskRecord>,
  targetIdentity: string
): TaskRecord | null {
  return (
    [...tasks].find(
      task =>
        task.action === 'model-download' &&
        task.context.targetIdentity === targetIdentity &&
        ['cancelling', 'paused', 'queued', 'running'].includes(task.status)
    ) || null
  )
}
