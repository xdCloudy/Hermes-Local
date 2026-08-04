export const HERMES_LOCAL_APPLICATION_COMPONENT = 'HermesLocal' as const

export interface DesktopUpdateActionPlan {
  arguments: string[]
  component: 'HermesAgent' | typeof HERMES_LOCAL_APPLICATION_COMPONENT
  scriptRelative: string
}

export interface DesktopUpdateHandoff {
  operationId: string
  pid: number
  planPath?: string
  taskId?: string | null
}

function inputText(input: Record<string, unknown>, key: string): string {
  return String(input[key] || '').trim()
}

function updateComponent(input: Record<string, unknown>): 'HermesAgent' | typeof HERMES_LOCAL_APPLICATION_COMPONENT {
  const explicit = inputText(input, 'component')

  if (explicit === 'HermesAgent' || explicit === HERMES_LOCAL_APPLICATION_COMPONENT) {
    return explicit
  }
  if (explicit) {
    throw new Error('Unsupported Hermes Local update component')
  }

  // Local-workstation actions always carry the selected inference profile.
  // Application update checks do not. Use that existing boundary to keep the
  // backend updater on NousResearch while unqualified app checks target this
  // Hermes Local repository.
  return inputText(input, 'profile') ? 'HermesAgent' : HERMES_LOCAL_APPLICATION_COMPONENT
}

function decodeMarker<T>(text: string, name: 'helper' | 'result' | 'status'): T | null {
  const pattern = new RegExp(`::hermes-desktop-update-${name}::([A-Za-z0-9+/=]+)`, 'g')
  const matches = [...text.matchAll(pattern)]
  const encoded = matches.at(-1)?.[1]

  if (!encoded) {
    return null
  }

  try {
    return JSON.parse(Buffer.from(encoded, 'base64').toString('utf8')) as T
  } catch {
    return null
  }
}

export function parseDesktopUpdateStatusMarker(text: string): Record<string, unknown> | null {
  return decodeMarker<Record<string, unknown>>(text, 'status')
}

export function parseDesktopUpdateResultMarker(text: string): Record<string, unknown> | null {
  return decodeMarker<Record<string, unknown>>(text, 'result')
}

export function parseDesktopUpdateHandoffMarker(text: string): DesktopUpdateHandoff | null {
  const value = decodeMarker<Record<string, unknown>>(text, 'helper')
  const pid = Number(value?.pid)
  const operationId = String(value?.operationId || '')

  if (!Number.isSafeInteger(pid) || pid <= 0 || !/^[0-9a-f]{32}$/i.test(operationId)) {
    return null
  }

  return {
    operationId,
    pid,
    planPath: value?.planPath ? String(value.planPath) : undefined,
    taskId: value?.taskId ? String(value.taskId) : null
  }
}

export function planDesktopUpdateAction(
  input: Record<string, unknown>,
  parentPid: number
): DesktopUpdateActionPlan {
  const component = updateComponent(input)
  const mode = inputText(input, 'mode') || 'Check'
  const targetCommit = inputText(input, 'targetCommit')

  if (component === HERMES_LOCAL_APPLICATION_COMPONENT) {
    const channel = inputText(input, 'channel') || 'development'

    if (!['Apply', 'Check', 'Rollback'].includes(mode)) {
      throw new Error('Unsupported Hermes Local application update mode')
    }
    if (!['beta', 'development', 'pinned', 'stable'].includes(channel)) {
      throw new Error('Unsupported Hermes Local update channel')
    }
    if (targetCommit && !/^[0-9a-f]{40}$/i.test(targetCommit)) {
      throw new Error('Hermes Local target commit must be a full 40-character SHA')
    }
    if (channel === 'pinned' && !targetCommit) {
      throw new Error('Pinned Hermes Local updates require a target commit')
    }

    const args = ['-Mode', mode, '-Channel', channel, '-ParentPid', String(parentPid)]

    if (targetCommit && mode !== 'Rollback') {
      args.push('-TargetCommit', targetCommit)
    }

    return {
      arguments: args,
      component: HERMES_LOCAL_APPLICATION_COMPONENT,
      scriptRelative: 'Invoke-Hermes-DesktopUpdate.ps1'
    }
  }

  const targetBranch = inputText(input, 'targetBranch')

  if (!['Apply', 'Check', 'Compatibility', 'Rollback'].includes(mode)) {
    throw new Error('Unsupported Hermes Agent update mode')
  }
  if (targetCommit && !/^[0-9a-f]{40}$/i.test(targetCommit)) {
    throw new Error('Hermes Agent target commit must be a full 40-character SHA')
  }
  if (targetBranch && !/^[A-Za-z0-9._/-]+$/.test(targetBranch)) {
    throw new Error('Hermes Agent target branch contains unsupported characters')
  }

  const args = ['-Mode', mode, '-Component', 'HermesAgent', '-Caller', 'Desktop']

  if (targetCommit && mode !== 'Rollback') {
    args.push('-TargetCommit', targetCommit)
  }
  if (targetBranch && mode !== 'Rollback') {
    args.push('-TargetBranch', targetBranch)
  }

  return { arguments: args, component: 'HermesAgent', scriptRelative: 'Update-Hermes-Local.ps1' }
}

export function desktopUpdateTaskContext(input: Record<string, unknown>): Record<string, string> {
  const component = updateComponent(input)
  const keys =
    component === HERMES_LOCAL_APPLICATION_COMPONENT
      ? ['channel', 'component', 'mode', 'targetCommit']
      : ['component', 'mode', 'targetBranch', 'targetCommit']

  return Object.fromEntries(
    keys
      .map(key => [key, inputText(input, key) || (key === 'mode' ? 'Check' : key === 'component' ? component : '')])
      .filter(([, value]) => value)
  )
}

export function expectedUpdateOperationComponent(context: Record<string, string>): 'HermesAgent' | 'Launcher' {
  return context.component === HERMES_LOCAL_APPLICATION_COMPONENT ? 'Launcher' : 'HermesAgent'
}
