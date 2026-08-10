import { act, cleanup, render, waitFor } from '@testing-library/react'
import type { MutableRefObject } from 'react'
import { useEffect } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  $currentBranch,
  $currentCwd,
  $newChatWorkspaceTarget,
  setCurrentBranch,
  setCurrentCwd,
  setCurrentCwdTransient,
  setNewChatWorkspaceTarget
} from '@/store/session'

import { useCwdActions } from './use-cwd-actions'

type CwdActionsHandle = ReturnType<typeof useCwdActions>

function deferred<T>() {
  let resolve!: (value: T) => void

  const promise = new Promise<T>(done => {
    resolve = done
  })

  return { promise, resolve }
}

function Harness({
  activeSessionIdRef,
  onSessionRuntimeInfo,
  onReady,
  requestGateway
}: {
  activeSessionIdRef: MutableRefObject<string | null>
  onSessionRuntimeInfo?: (info: { branch?: string; cwd?: string }) => void
  onReady: (handle: CwdActionsHandle) => void
  requestGateway: <T>(method: string, params?: Record<string, unknown>) => Promise<T>
}) {
  const actions = useCwdActions({
    activeSessionIdRef,
    onSessionRuntimeInfo,
    requestGateway
  })

  useEffect(() => {
    onReady(actions)
  }, [actions, onReady])

  return null
}

describe('useCwdActions draft workspace target', () => {
  beforeEach(() => {
    setCurrentCwd('')
    setCurrentBranch('')
    setNewChatWorkspaceTarget(undefined)
  })

  afterEach(() => {
    cleanup()
    setCurrentCwd('')
    setCurrentBranch('')
    setNewChatWorkspaceTarget(undefined)
    vi.restoreAllMocks()
  })

  it('ignores stale draft cwd normalization after a newer no-workspace target wins', async () => {
    const projectInfo = deferred<{ branch?: string; cwd?: string }>()
    const requestGateway = vi.fn(async () => projectInfo.promise as never)
    const activeSessionIdRef: MutableRefObject<string | null> = { current: null }
    let handle: CwdActionsHandle | null = null

    render(
      <Harness activeSessionIdRef={activeSessionIdRef} onReady={h => (handle = h)} requestGateway={requestGateway} />
    )
    await waitFor(() => expect(handle).not.toBeNull())

    let pendingChange!: Promise<void>

    await act(async () => {
      pendingChange = handle!.changeSessionCwd('/stale-workspace')
    })

    expect($newChatWorkspaceTarget.get()).toBe('/stale-workspace')

    setNewChatWorkspaceTarget(null)
    setCurrentCwdTransient('')
    projectInfo.resolve({ branch: 'main', cwd: '/normalized-stale-workspace' })

    await act(async () => {
      await pendingChange
    })

    expect($newChatWorkspaceTarget.get()).toBeNull()
    expect($currentCwd.get()).toBe('')
    expect($currentBranch.get()).toBe('')
  })

  it('detaches a draft locally without asking the gateway', async () => {
    const requestGateway = vi.fn()
    const activeSessionIdRef: MutableRefObject<string | null> = { current: null }
    let handle: CwdActionsHandle | null = null

    setCurrentCwd('/attached')
    setCurrentBranch('main')
    setNewChatWorkspaceTarget('/attached')
    render(
      <Harness activeSessionIdRef={activeSessionIdRef} onReady={h => (handle = h)} requestGateway={requestGateway} />
    )
    await waitFor(() => expect(handle).not.toBeNull())

    await act(async () => {
      await handle!.changeSessionCwd('')
    })

    expect($currentCwd.get()).toBe('')
    expect($currentBranch.get()).toBe('')
    expect($newChatWorkspaceTarget.get()).toBeNull()
    expect(requestGateway).not.toHaveBeenCalled()
  })

  it('detaches the active chat through session.cwd.set and applies blank runtime cwd', async () => {
    const requestGateway = vi.fn(async () => ({ branch: '', cwd: '' }) as never)
    const onSessionRuntimeInfo = vi.fn()
    const activeSessionIdRef: MutableRefObject<string | null> = { current: 'runtime-1' }
    let handle: CwdActionsHandle | null = null

    setCurrentCwd('/attached')
    setCurrentBranch('main')
    render(
      <Harness
        activeSessionIdRef={activeSessionIdRef}
        onReady={h => (handle = h)}
        onSessionRuntimeInfo={onSessionRuntimeInfo}
        requestGateway={requestGateway}
      />
    )
    await waitFor(() => expect(handle).not.toBeNull())

    await act(async () => {
      await handle!.changeSessionCwd('')
    })

    expect(requestGateway).toHaveBeenCalledWith('session.cwd.set', {
      cwd: '',
      session_id: 'runtime-1'
    })
    expect($currentCwd.get()).toBe('')
    expect($currentBranch.get()).toBe('')
    expect(onSessionRuntimeInfo).toHaveBeenCalledWith({ branch: '', cwd: '' })
  })
})
