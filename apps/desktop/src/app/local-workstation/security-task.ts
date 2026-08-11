import type { LocalActionTask } from './types'

export function latestSecurityTask(tasks: LocalActionTask[]): LocalActionTask | undefined {
  return [...tasks]
    .filter(task => task.action === 'security')
    .sort((left, right) => Date.parse(right.createdAt) - Date.parse(left.createdAt))[0]
}

export function securityTaskState(task: LocalActionTask | undefined): string {
  if (!task) {
    return 'Not started'
  }

  return task.status === 'succeeded'
    ? 'Completed'
    : task.status === 'cancelling'
      ? 'Cancelling'
      : task.status.charAt(0).toUpperCase() + task.status.slice(1)
}
