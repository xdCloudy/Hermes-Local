import { describe, expect, it } from 'vitest'

import {
  desktopUpdateTaskContext,
  expectedUpdateOperationComponent,
  parseDesktopUpdateHandoffMarker,
  parseDesktopUpdateResultMarker,
  parseDesktopUpdateStatusMarker,
  planDesktopUpdateAction
} from './hermes-local-desktop-update'

function marker(name: string, value: object): string {
  return `::hermes-desktop-update-${name}::${Buffer.from(JSON.stringify(value)).toString('base64')}`
}

describe('Hermes Local native application update bridge', () => {
  it('routes application checks through the trusted updater', () => {
    expect(planDesktopUpdateAction({ component: 'HermesLocal', mode: 'Check' }, 4242)).toEqual({
      arguments: ['-Mode', 'Check', '-Channel', 'development', '-ParentPid', '4242'],
      component: 'HermesLocal',
      scriptRelative: 'Invoke-Hermes-DesktopUpdate.ps1'
    })
  })

  it('defaults unqualified desktop checks to the Hermes Local application', () => {
    expect(planDesktopUpdateAction({ mode: 'Check' }, 4242)).toEqual({
      arguments: ['-Mode', 'Check', '-Channel', 'development', '-ParentPid', '4242'],
      component: 'HermesLocal',
      scriptRelative: 'Invoke-Hermes-DesktopUpdate.ps1'
    })
    expect(desktopUpdateTaskContext({ mode: 'Check' })).toMatchObject({
      component: 'HermesLocal',
      mode: 'Check'
    })
  })

  it('preserves the existing Hermes Agent transactional route', () => {
    expect(planDesktopUpdateAction({ component: 'HermesAgent', mode: 'Apply', targetBranch: 'main' }, 1)).toEqual({
      arguments: ['-Mode', 'Apply', '-Component', 'HermesAgent', '-Caller', 'Desktop', '-TargetBranch', 'main'],
      component: 'HermesAgent',
      scriptRelative: 'Update-Hermes-Local.ps1'
    })
  })

  it('rejects unsafe pinned identities and branch input', () => {
    expect(() => planDesktopUpdateAction({ component: 'HermesLocal', channel: 'pinned' }, 1)).toThrow(/target commit/i)
    expect(() =>
      planDesktopUpdateAction({ component: 'HermesAgent', targetBranch: 'main\n--upload-pack=bad' }, 1)
    ).toThrow(/unsupported characters/i)
  })

  it('parses status, ready-to-restart results, and legacy helper handoffs', () => {
    expect(parseDesktopUpdateStatusMarker(marker('status', { supported: true, behind: 2 }))).toMatchObject({
      supported: true,
      behind: 2
    })
    expect(
      parseDesktopUpdateResultMarker(
        marker('result', {
          status: 'ready-to-restart',
          launcherStayedOpen: true,
          pendingActivation: true,
          restartRequired: true
        })
      )
    ).toMatchObject({
      status: 'ready-to-restart',
      launcherStayedOpen: true,
      pendingActivation: true,
      restartRequired: true
    })
    expect(
      parseDesktopUpdateHandoffMarker(
        marker('helper', { pid: 9001, operationId: 'a'.repeat(32), taskId: 'task-1', planPath: 'D:/safe/plan.json' })
      )
    ).toMatchObject({ pid: 9001, operationId: 'a'.repeat(32), taskId: 'task-1' })
  })

  it('records the application component and reconciles against launcher operations', () => {
    const context = desktopUpdateTaskContext({ component: 'HermesLocal', mode: 'Apply', channel: 'stable' })

    expect(context).toMatchObject({ component: 'HermesLocal', mode: 'Apply', channel: 'stable' })
    expect(expectedUpdateOperationComponent(context)).toBe('Launcher')
    expect(expectedUpdateOperationComponent({ component: 'HermesAgent' })).toBe('HermesAgent')
  })
})
