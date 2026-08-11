export const TASK_SCHEMA_VERSION = 1 as const

export type TaskAction =
  | 'backup'
  | 'benchmark'
  | 'diagnostics'
  | 'model-download'
  | 'repair'
  | 'restart'
  | 'restore'
  | 'security'
  | 'start'
  | 'stop'
  | 'switch-model'
  | 'test'
  | 'update'

export type TaskState =
  | 'cancelled'
  | 'cancelling'
  | 'failed'
  | 'interrupted'
  | 'paused'
  | 'queued'
  | 'running'
  | 'succeeded'

export type TaskResource = 'installation' | 'model-runtime' | 'model-storage' | 'user-data' | 'workstation'
export type TaskResourceMode = 'exclusive' | 'shared'
export type TaskConflictPolicy = 'queue' | 'reject'

export interface TaskResourceClaim {
  mode: TaskResourceMode
  resource: TaskResource
}

export interface TaskOwner {
  kind: 'desktop-child-process' | 'external-process'
  pid: null | number
}

export interface TaskFailure {
  code: string
  message: string
}

export interface TaskResult {
  kind: 'archive' | 'report' | 'runtime-state'
  path: string
}

export interface TaskProgress {
  bytesCompleted?: null | number
  bytesTotal?: null | number
  cancellable?: boolean
  completedUnits: null | number
  counters: Record<string, number>
  etaSeconds?: null | number
  message: null | string
  mode: 'determinate' | 'indeterminate'
  pauseSupported?: boolean
  percent: null | number
  rateBytesPerSecond?: null | number
  resumeSupported?: boolean
  totalUnits: null | number
}

export interface TaskCapabilities {
  cancel: boolean
  pause: boolean
  resume: boolean
  retry: boolean
}

export interface TaskRecord {
  action: TaskAction
  context: Record<string, string>
  completedAt: null | string
  conflictPolicy: TaskConflictPolicy
  createdAt: string
  exitCode: null | number
  failure: null | TaskFailure
  id: string
  output: string
  outputTruncated: boolean
  owner: TaskOwner
  progress?: null | TaskProgress
  queuedAt: string
  resources: TaskResourceClaim[]
  result: null | TaskResult
  schemaVersion: typeof TASK_SCHEMA_VERSION
  stage: null | string
  startedAt: null | string
  status: TaskState
  updatedAt: string
}

export interface TaskView extends TaskRecord {
  capabilities: TaskCapabilities
}

export interface TaskPolicy {
  cancellable: boolean
  conflictPolicy: TaskConflictPolicy
  resources: readonly TaskResourceClaim[]
}

export interface TaskCompletionEvidence {
  exitCode: null | number
  failure: null | TaskFailure
  observedAt: string
  result: null | TaskResult
  status: 'cancelled' | 'failed' | 'interrupted' | 'succeeded'
}

const sharedWorkstation = { mode: 'shared', resource: 'workstation' } as const
const exclusiveWorkstation = { mode: 'exclusive', resource: 'workstation' } as const
const exclusiveInstallation = { mode: 'exclusive', resource: 'installation' } as const
const exclusiveModelStorage = { mode: 'exclusive', resource: 'model-storage' } as const
const exclusiveUserData = { mode: 'exclusive', resource: 'user-data' } as const

export const TASK_POLICIES = {
  backup: {
    cancellable: true,
    conflictPolicy: 'reject',
    resources: [sharedWorkstation, { mode: 'shared', resource: 'user-data' }]
  },
  benchmark: {
    cancellable: true,
    conflictPolicy: 'reject',
    resources: [sharedWorkstation, { mode: 'exclusive', resource: 'model-runtime' }]
  },
  diagnostics: { cancellable: true, conflictPolicy: 'reject', resources: [] },
  'model-download': {
    cancellable: true,
    conflictPolicy: 'queue',
    resources: [exclusiveModelStorage]
  },
  repair: { cancellable: false, conflictPolicy: 'reject', resources: [exclusiveWorkstation, exclusiveModelStorage] },
  restart: { cancellable: false, conflictPolicy: 'reject', resources: [exclusiveWorkstation] },
  restore: {
    cancellable: true,
    conflictPolicy: 'reject',
    resources: [exclusiveWorkstation, exclusiveInstallation, exclusiveUserData, exclusiveModelStorage]
  },
  security: { cancellable: true, conflictPolicy: 'reject', resources: [] },
  start: { cancellable: false, conflictPolicy: 'queue', resources: [sharedWorkstation] },
  stop: { cancellable: false, conflictPolicy: 'reject', resources: [exclusiveWorkstation] },
  'switch-model': {
    cancellable: false,
    conflictPolicy: 'reject',
    resources: [exclusiveWorkstation, exclusiveModelStorage]
  },
  test: {
    cancellable: true,
    conflictPolicy: 'reject',
    resources: [sharedWorkstation, { mode: 'shared', resource: 'model-runtime' }]
  },
  // This action only runs Update-Hermes-Local.ps1 -Mode Check. Update
  // activation is coordinated separately and retains its exclusive locks.
  update: { cancellable: false, conflictPolicy: 'reject', resources: [exclusiveModelStorage] }
} as const satisfies Record<TaskAction, TaskPolicy>

const TASK_TRANSITIONS: Readonly<Record<TaskState, readonly TaskState[]>> = {
  cancelled: [],
  cancelling: ['cancelled', 'failed', 'interrupted', 'paused', 'succeeded'],
  failed: [],
  interrupted: [],
  paused: ['cancelled', 'failed', 'interrupted', 'queued', 'running'],
  queued: ['cancelled', 'failed', 'interrupted', 'running'],
  running: ['cancelling', 'failed', 'interrupted', 'paused', 'succeeded'],
  succeeded: []
}

const terminalStates = new Set<TaskState>(['cancelled', 'failed', 'interrupted', 'succeeded'])
const lockOwningStates = new Set<TaskState>(['cancelling', 'paused', 'running'])

export function isTaskTerminal(state: TaskState): boolean {
  return terminalStates.has(state)
}

export function taskOwnsResources(state: TaskState): boolean {
  return lockOwningStates.has(state)
}

export function taskCapabilities(task: TaskRecord): TaskCapabilities {
  const cancellableOwner =
    task.status === 'queued' ||
    (task.action === 'model-download' && task.status === 'paused') ||
    (task.status === 'running' && task.owner.kind === 'desktop-child-process' && task.owner.pid !== null)

  const progressAllowsCancellation =
    (task.action !== 'restore' && task.action !== 'model-download') || task.progress?.cancellable !== false
  const ownedDownload = task.action === 'model-download' && task.owner.kind === 'desktop-child-process'

  return {
    cancel: TASK_POLICIES[task.action].cancellable && cancellableOwner && progressAllowsCancellation,
    pause: ownedDownload && task.status === 'running' && task.progress?.pauseSupported === true,
    resume: ownedDownload && task.status === 'paused' && task.progress?.resumeSupported === true,
    retry: isTaskTerminal(task.status)
  }
}

export function createTaskRecord(
  action: TaskAction,
  id: string,
  owner: TaskOwner,
  createdAt: string,
  context: Record<string, string> = {}
): TaskRecord {
  const policy = TASK_POLICIES[action]

  return {
    action,
    context: { ...context },
    completedAt: null,
    conflictPolicy: policy.conflictPolicy,
    createdAt,
    exitCode: null,
    failure: null,
    id,
    output: '',
    outputTruncated: false,
    owner,
    progress: null,
    queuedAt: createdAt,
    resources: policy.resources.map(claim => ({ ...claim })),
    result: null,
    schemaVersion: TASK_SCHEMA_VERSION,
    stage: null,
    startedAt: null,
    status: 'queued',
    updatedAt: createdAt
  }
}

export function transitionTask(
  task: TaskRecord,
  status: TaskState,
  at: string,
  details: Partial<Pick<TaskRecord, 'exitCode' | 'failure' | 'owner' | 'result'>> = {}
): TaskRecord {
  if (task.status === status) {
    return task
  }

  if (!TASK_TRANSITIONS[task.status].includes(status)) {
    throw new Error(`Invalid task transition '${task.status}' -> '${status}'`)
  }

  return {
    ...task,
    ...details,
    completedAt: isTaskTerminal(status) ? at : task.completedAt,
    startedAt: status === 'running' && !task.startedAt ? at : task.startedAt,
    status,
    updatedAt: at
  }
}

export function requestTaskCancellation(task: TaskRecord, at: string): TaskRecord {
  if (!TASK_POLICIES[task.action].cancellable) {
    throw new Error(`Task '${task.action}' cannot be cancelled after admission`)
  }

  if (task.status === 'queued' || task.status === 'paused') {
    return transitionTask(task, 'cancelled', at)
  }

  if (task.status === 'running') {
    return transitionTask(task, 'cancelling', at)
  }

  if (task.status === 'cancelling') {
    return task
  }

  throw new Error(`Task '${task.id}' is already ${task.status}`)
}

export function reconcileTaskOwner(task: TaskRecord, ownerAlive: boolean, at: string): TaskRecord {
  if (!taskOwnsResources(task.status) || task.owner.pid === null || ownerAlive) {
    return task
  }

  return transitionTask(task, 'interrupted', at, {
    failure: {
      code: 'owner-exited',
      message: `Task owner process ${task.owner.pid} is no longer running`
    }
  })
}

export function reconcileRecoveredTask(
  task: TaskRecord,
  ownerAlive: boolean,
  evidence: null | TaskCompletionEvidence,
  at: string
): TaskRecord {
  if (isTaskTerminal(task.status)) {
    return task
  }

  if (task.status === 'queued') {
    return transitionTask(task, 'interrupted', at, {
      failure: {
        code: 'desktop-restarted-before-start',
        message: 'Desktop restarted before the queued task acquired its resources'
      }
    })
  }

  if (ownerAlive) {
    if (task.owner.kind === 'external-process') {
      return task
    }

    return {
      ...task,
      owner: { kind: 'external-process', pid: task.owner.pid },
      updatedAt: at
    }
  }

  const taskStart = Date.parse(task.startedAt || task.createdAt)
  const evidenceTime = evidence ? Date.parse(evidence.observedAt) : Number.NaN

  if (evidence && Number.isFinite(evidenceTime) && evidenceTime >= taskStart) {
    const source =
      evidence.status === 'cancelled' && task.status === 'running'
        ? transitionTask(task, 'cancelling', at)
        : task

    return transitionTask(source, evidence.status, at, {
      exitCode: evidence.exitCode,
      failure: evidence.failure,
      result: evidence.result
    })
  }

  if (task.status === 'cancelling') {
    return transitionTask(task, 'cancelled', at)
  }

  return transitionTask(task, 'interrupted', at, {
    failure: {
      code: 'owner-exited-without-result',
      message: task.owner.pid
        ? `Recovered task owner process ${task.owner.pid} exited without authoritative result evidence`
        : 'Recovered task has no live owner or authoritative result evidence'
    }
  })
}

const taskActions = new Set<TaskAction>(Object.keys(TASK_POLICIES) as TaskAction[])
const taskStates = new Set<TaskState>(Object.keys(TASK_TRANSITIONS) as TaskState[])

function validTimestamp(value: unknown, nullable = false): null | string {
  if (nullable && value === null) {
    return null
  }

  return typeof value === 'string' && Number.isFinite(Date.parse(value)) ? value : null
}

export function restoreTaskRecord(value: unknown, maximumOutput: number): null | TaskRecord {
  if (!value || typeof value !== 'object') {
    return null
  }

  const candidate = value as Record<string, unknown>
  const action = candidate.action as TaskAction
  const status = candidate.status as TaskState
  const createdAt = validTimestamp(candidate.createdAt)
  const queuedAt = validTimestamp(candidate.queuedAt)
  const updatedAt = validTimestamp(candidate.updatedAt)
  const startedAt = validTimestamp(candidate.startedAt, true)
  const completedAt = validTimestamp(candidate.completedAt, true)

  if (
    candidate.schemaVersion !== TASK_SCHEMA_VERSION ||
    typeof candidate.id !== 'string' ||
    candidate.id.length < 1 ||
    !taskActions.has(action) ||
    !taskStates.has(status) ||
    !createdAt ||
    !queuedAt ||
    !updatedAt ||
    (candidate.startedAt !== null && !startedAt) ||
    (candidate.completedAt !== null && !completedAt) ||
    typeof candidate.output !== 'string' ||
    typeof candidate.outputTruncated !== 'boolean' ||
    (!isTaskTerminal(status) && completedAt !== null) ||
    ((status === 'running' || status === 'cancelling') && !startedAt) ||
    (status === 'succeeded' && !startedAt) ||
    (isTaskTerminal(status) && !completedAt)
  ) {
    return null
  }

  const ownerValue = candidate.owner as undefined | Record<string, unknown>
  const ownerKind = ownerValue?.kind
  const ownerPid = ownerValue?.pid

  if (
    (ownerKind !== 'desktop-child-process' && ownerKind !== 'external-process') ||
    (ownerPid !== null && (!Number.isSafeInteger(ownerPid) || Number(ownerPid) <= 0))
  ) {
    return null
  }

  const exitCodeValue = candidate.exitCode

  if (exitCodeValue !== null && !Number.isSafeInteger(exitCodeValue)) {
    return null
  }

  const failureValue = candidate.failure as null | undefined | Record<string, unknown>

  const failure =
    failureValue === null
      ? null
      : failureValue && typeof failureValue.code === 'string' && typeof failureValue.message === 'string'
        ? { code: failureValue.code, message: failureValue.message }
        : null

  if (candidate.failure !== null && !failure) {
    return null
  }

  const contextValue = candidate.context
  const context: Record<string, string> = {}

  if (contextValue !== undefined) {
    if (!contextValue || typeof contextValue !== 'object' || Array.isArray(contextValue)) {
      return null
    }

    for (const [key, value] of Object.entries(contextValue as Record<string, unknown>)) {
      if (typeof value !== 'string' || key.length > 64 || value.length > 1024) {
        return null
      }

      context[key] = value
    }
  }

  const stageValue = candidate.stage

  if (
    stageValue !== null &&
    stageValue !== undefined &&
    (typeof stageValue !== 'string' || stageValue.length > 128)
  ) {
    return null
  }

  const stage = typeof stageValue === 'string' ? stageValue : null
  const progressValue = candidate.progress as null | undefined | Record<string, unknown>
  let progress: null | TaskProgress = null

  if (progressValue !== null && progressValue !== undefined) {
    const mode = progressValue.mode
    const bytesCompleted = progressValue.bytesCompleted
    const bytesTotal = progressValue.bytesTotal
    const cancellable = progressValue.cancellable
    const completedUnits = progressValue.completedUnits
    const etaSeconds = progressValue.etaSeconds
    const pauseSupported = progressValue.pauseSupported
    const percent = progressValue.percent
    const rateBytesPerSecond = progressValue.rateBytesPerSecond
    const resumeSupported = progressValue.resumeSupported
    const totalUnits = progressValue.totalUnits
    const message = progressValue.message
    const countersValue = progressValue.counters

    if (
      (mode !== 'determinate' && mode !== 'indeterminate') ||
      (bytesCompleted !== undefined && bytesCompleted !== null && (!Number.isFinite(Number(bytesCompleted)) || Number(bytesCompleted) < 0)) ||
      (bytesTotal !== undefined && bytesTotal !== null && (!Number.isFinite(Number(bytesTotal)) || Number(bytesTotal) <= 0)) ||
      (cancellable !== undefined && typeof cancellable !== 'boolean') ||
      (completedUnits !== null && (!Number.isFinite(Number(completedUnits)) || Number(completedUnits) < 0)) ||
      (etaSeconds !== undefined && etaSeconds !== null && (!Number.isFinite(Number(etaSeconds)) || Number(etaSeconds) < 0)) ||
      (pauseSupported !== undefined && typeof pauseSupported !== 'boolean') ||
      (percent !== null && (!Number.isFinite(Number(percent)) || Number(percent) < 0 || Number(percent) > 100)) ||
      (rateBytesPerSecond !== undefined && rateBytesPerSecond !== null && (!Number.isFinite(Number(rateBytesPerSecond)) || Number(rateBytesPerSecond) < 0)) ||
      (resumeSupported !== undefined && typeof resumeSupported !== 'boolean') ||
      (totalUnits !== null && (!Number.isFinite(Number(totalUnits)) || Number(totalUnits) <= 0)) ||
      (message !== null && (typeof message !== 'string' || message.length > 2048)) ||
      !countersValue ||
      typeof countersValue !== 'object' ||
      Array.isArray(countersValue)
    ) {
      return null
    }

    const counters: Record<string, number> = {}
    for (const [key, value] of Object.entries(countersValue as Record<string, unknown>)) {
      if (key.length > 64 || !Number.isFinite(Number(value)) || Number(value) < 0) {
        return null
      }
      counters[key] = Number(value)
    }

    progress = {
      ...(bytesCompleted === undefined ? {} : { bytesCompleted: bytesCompleted === null ? null : Number(bytesCompleted) }),
      ...(bytesTotal === undefined ? {} : { bytesTotal: bytesTotal === null ? null : Number(bytesTotal) }),
      ...(typeof cancellable === 'boolean' ? { cancellable } : {}),
      completedUnits: completedUnits === null ? null : Number(completedUnits),
      counters,
      ...(etaSeconds === undefined ? {} : { etaSeconds: etaSeconds === null ? null : Number(etaSeconds) }),
      message: message === null ? null : String(message),
      mode,
      ...(typeof pauseSupported === 'boolean' ? { pauseSupported } : {}),
      percent: percent === null ? null : Number(percent),
      ...(rateBytesPerSecond === undefined
        ? {}
        : { rateBytesPerSecond: rateBytesPerSecond === null ? null : Number(rateBytesPerSecond) }),
      ...(typeof resumeSupported === 'boolean' ? { resumeSupported } : {}),
      totalUnits: totalUnits === null ? null : Number(totalUnits)
    }
  }

  const bounded = boundedTaskOutput('', candidate.output, maximumOutput)
  const policy = TASK_POLICIES[action]
  const resultValue = candidate.result as null | undefined | Record<string, unknown>

  const result: null | TaskResult =
    resultValue === null
      ? null
      : resultValue &&
          (resultValue.kind === 'archive' || resultValue.kind === 'report' || resultValue.kind === 'runtime-state') &&
          typeof resultValue.path === 'string'
        ? { kind: resultValue.kind, path: resultValue.path }
        : null

  if (candidate.result !== null && !result) {
    return null
  }

  return {
    action,
    context,
    completedAt,
    conflictPolicy: policy.conflictPolicy,
    createdAt,
    exitCode: exitCodeValue as null | number,
    failure,
    id: candidate.id,
    output: bounded.output,
    outputTruncated: candidate.outputTruncated || bounded.truncated,
    owner: { kind: ownerKind, pid: ownerPid as null | number },
    progress,
    queuedAt,
    resources: policy.resources.map(claim => ({ ...claim })),
    result,
    schemaVersion: TASK_SCHEMA_VERSION,
    stage,
    startedAt,
    status,
    updatedAt
  }
}

export function boundedTaskOutput(current: string, addition: string, maximum: number) {
  if (!Number.isInteger(maximum) || maximum < 1) {
    throw new Error('Task output bound must be a positive integer')
  }

  const combined = `${current}${addition}`
  const truncated = combined.length > maximum

  return {
    output: truncated ? combined.slice(combined.length - maximum) : combined,
    truncated
  }
}

export interface TaskConflict {
  action: TaskAction
  resources: TaskResource[]
  taskId: string
}

export type TaskAdmission =
  | { kind: 'join'; message: string; taskId: string }
  | { conflicts: TaskConflict[]; kind: 'queue' | 'reject'; message: string }
  | { kind: 'start'; message: string }

function conflictingResources(
  requested: readonly TaskResourceClaim[],
  active: readonly TaskResourceClaim[]
): TaskResource[] {
  const conflicts = new Set<TaskResource>()

  for (const request of requested) {
    for (const claim of active) {
      if (request.resource === claim.resource && (request.mode === 'exclusive' || claim.mode === 'exclusive')) {
        conflicts.add(request.resource)
      }
    }
  }

  return [...conflicts].sort()
}

export function admitTask(
  action: TaskAction,
  tasks: Iterable<TaskRecord>,
  context: Record<string, string> = {}
): TaskAdmission {
  const records = [...tasks]
  const duplicate = records.find(task => {
    if (task.action !== action || isTaskTerminal(task.status)) {
      return false
    }
    if (action !== 'model-download') {
      return true
    }
    return Boolean(context.targetIdentity) && task.context.targetIdentity === context.targetIdentity
  })

  if (duplicate) {
    return {
      kind: 'join',
      message: `Joined existing '${action}' task ${duplicate.id}`,
      taskId: duplicate.id
    }
  }

  const policy = TASK_POLICIES[action]

  const conflicts = records
    .filter(task => taskOwnsResources(task.status))
    .map(task => {
      const resources = conflictingResources(policy.resources, task.resources)
      const restoreUpdateConflict =
        (action === 'restore' && task.action === 'update') ||
        (action === 'update' && task.action === 'restore')

      if (restoreUpdateConflict && !resources.includes('installation')) {
        resources.push('installation')
        resources.sort()
      }

      return { action: task.action, resources, taskId: task.id }
    })
    .filter(conflict => conflict.resources.length > 0)
    .sort((left, right) => left.taskId.localeCompare(right.taskId))

  if (conflicts.length === 0) {
    return { kind: 'start', message: `Resources available for '${action}'` }
  }

  const summary = conflicts
    .map(conflict => `'${conflict.action}' (${conflict.resources.join(', ')})`)
    .join('; ')

  return {
    conflicts,
    kind: policy.conflictPolicy,
    message: `Cannot start '${action}' while ${summary} owns conflicting resources`
  }
}
