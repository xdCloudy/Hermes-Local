import type { TaskView } from './hermes-local-task-model'

export type ModelSelectionPlan =
  | { kind: 'join'; taskId: string }
  | { kind: 'persist' }
  | { kind: 'reject'; message: string }
  | { kind: 'start' }
  | { kind: 'unchanged' }

export function activeModelSwitch(tasks: Iterable<TaskView>): TaskView | null {
  return (
    [...tasks].find(
      task =>
        task.action === 'switch-model' &&
        (task.status === 'queued' || task.status === 'running' || task.status === 'cancelling')
    ) || null
  )
}

export function planModelSelection(input: {
  activeTask: TaskView | null
  currentModelId: string
  runtimeRunning: boolean
  targetModelId: string
}): ModelSelectionPlan {
  const { activeTask, currentModelId, runtimeRunning, targetModelId } = input

  if (activeTask) {
    if (activeTask.context.targetModelId === targetModelId) {
      return { kind: 'join', taskId: activeTask.id }
    }

    return {
      kind: 'reject',
      message: `Model switch to '${activeTask.context.targetModelId || 'another model'}' is already in progress`
    }
  }

  if (targetModelId === currentModelId) {
    return { kind: 'unchanged' }
  }

  return runtimeRunning ? { kind: 'start' } : { kind: 'persist' }
}

export function runtimeModelIdentityMatches(input: {
  configuredAlias: string
  configuredModelId: string
  runtimeAlias: unknown
  runtimeModelId: unknown
}): boolean {
  return (
    input.runtimeModelId === input.configuredModelId &&
    input.runtimeAlias === input.configuredAlias
  )
}

export function shouldBlockHermesApiDuringModelSwitch(method: unknown, active: boolean): boolean {
  const normalized = String(method || 'GET').toUpperCase()
  return active && normalized !== 'GET' && normalized !== 'HEAD'
}
