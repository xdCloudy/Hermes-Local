import { describe, expect, it } from 'vitest'

import {
  activeModelSwitch,
  planModelSelection,
  runtimeModelIdentityMatches,
  shouldBlockHermesApiDuringModelSwitch
} from './hermes-local-model-switch'
import { createTaskRecord, taskCapabilities, type TaskView, transitionTask } from './hermes-local-task-model'

function switchTask(targetModelId: string, status: 'queued' | 'running' = 'running'): TaskView {
  const created = createTaskRecord(
    'switch-model',
    'switch-task',
    { kind: 'desktop-child-process', pid: status === 'running' ? 41 : null },
    '2026-08-02T10:00:00.000Z',
    { previousModelId: 'qwen', profile: 'Daily', targetModelId }
  )
  const record = status === 'running'
    ? transitionTask(created, 'running', '2026-08-02T10:00:01.000Z', {
        owner: { kind: 'desktop-child-process', pid: 41 }
      })
    : created

  return { ...record, capabilities: taskCapabilities(record) }
}

describe('Hermes Local model switch admission', () => {
  it('persists a stopped-stack selection without starting services', () => {
    expect(
      planModelSelection({ activeTask: null, currentModelId: 'qwen', runtimeRunning: false, targetModelId: 'agents' })
    ).toEqual({ kind: 'persist' })
  })

  it('starts a managed switch for a running stack', () => {
    expect(
      planModelSelection({ activeTask: null, currentModelId: 'qwen', runtimeRunning: true, targetModelId: 'agents' })
    ).toEqual({ kind: 'start' })
  })

  it('joins rapid duplicate selections and rejects a different target', () => {
    const activeTask = switchTask('agents')

    expect(planModelSelection({ activeTask, currentModelId: 'qwen', runtimeRunning: true, targetModelId: 'agents' })).toEqual({
      kind: 'join',
      taskId: 'switch-task'
    })
    expect(
      planModelSelection({ activeTask, currentModelId: 'qwen', runtimeRunning: true, targetModelId: 'llama' })
    ).toMatchObject({ kind: 'reject' })
  })

  it('recovers the active target from a persisted task after renderer reload', () => {
    expect(activeModelSwitch([switchTask('agents', 'queued')])?.context.targetModelId).toBe('agents')
  })

  it('gates mutating Chat API requests while the provider is replacing', () => {
    expect(shouldBlockHermesApiDuringModelSwitch('POST', true)).toBe(true)
    expect(shouldBlockHermesApiDuringModelSwitch('GET', true)).toBe(false)
    expect(shouldBlockHermesApiDuringModelSwitch('POST', false)).toBe(false)
  })

  it('requires configured and runtime identities to agree', () => {
    expect(
      runtimeModelIdentityMatches({
        configuredAlias: 'agents-local',
        configuredModelId: 'agents',
        runtimeAlias: 'agents-local',
        runtimeModelId: 'agents'
      })
    ).toBe(true)
    expect(
      runtimeModelIdentityMatches({
        configuredAlias: 'agents-local',
        configuredModelId: 'agents',
        runtimeAlias: 'qwen-local',
        runtimeModelId: 'qwen'
      })
    ).toBe(false)
  })
})
