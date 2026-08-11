import { act, cleanup, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { getStatus } from '@/hermes'
import type { StatusResponse } from '@/types/hermes'

import { useStatusSnapshot } from './use-status-snapshot'

vi.mock('@/hermes', () => ({
  getStatus: vi.fn()
}))

type GatewayRequester = <T = unknown>(method: string, params?: Record<string, unknown>) => Promise<T>

function status(overrides: Partial<StatusResponse> = {}): StatusResponse {
  return {
    active_sessions: 0,
    config_path: '',
    config_version: 1,
    env_path: '',
    gateway_exit_reason: null,
    gateway_health_url: null,
    gateway_pid: 1,
    gateway_platforms: {},
    gateway_running: true,
    gateway_state: 'running',
    gateway_updated_at: null,
    hermes_home: '',
    latest_config_version: 1,
    release_date: '',
    version: 'test',
    ...overrides
  }
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined
  let reject: (reason?: unknown) => void = () => undefined

  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve
    reject = nextReject
  })

  return { promise, reject, resolve }
}

async function flushAsync() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0)
  })
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.mocked(getStatus).mockReset().mockResolvedValue(status())
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

describe('useStatusSnapshot', () => {
  it('polls authoritative REST status every two seconds without rerunning the heavy inference probe', async () => {
    const requestGatewayMock = vi.fn(
      async (method: string) =>
        (method === 'setup.runtime_check' ? { ok: true } : { provider_configured: true }) as never
    )
    const requestGateway = requestGatewayMock as unknown as GatewayRequester

    renderHook(() => useStatusSnapshot('open', requestGateway))
    await flushAsync()

    expect(getStatus).toHaveBeenCalledTimes(1)
    expect(requestGatewayMock).toHaveBeenCalledTimes(2)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000)
    })

    expect(getStatus).toHaveBeenCalledTimes(2)
    expect(requestGatewayMock).toHaveBeenCalledTimes(2)
  })

  it('clears a stale running snapshot when authoritative status becomes unavailable', async () => {
    vi.mocked(getStatus)
      .mockResolvedValueOnce(status())
      .mockRejectedValueOnce(new Error('status endpoint unavailable'))

    const requestGateway = vi.fn() as unknown as GatewayRequester
    const { result } = renderHook(() => useStatusSnapshot('closed', requestGateway))

    await flushAsync()
    expect(result.current.statusSnapshot?.gateway_running).toBe(true)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000)
    })

    expect(result.current.statusSnapshot).toBeNull()
  })

  it('keeps the last authoritative inference readiness through a transient RPC failure', async () => {
    let refresh = 0

    const requestGatewayMock = vi.fn(async (method: string) => {
      const cycle = Math.floor(refresh / 2)
      refresh += 1

      if (cycle > 0) {
        throw new Error(`${method} timed out`)
      }

      return (method === 'setup.runtime_check' ? { ok: true } : { provider_configured: true }) as never
    })
    const requestGateway = requestGatewayMock as unknown as GatewayRequester
    const { result } = renderHook(() => useStatusSnapshot('open', requestGateway))

    await flushAsync()
    expect(result.current.inferenceStatus).toMatchObject({ ready: true, source: 'runtime_check' })

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000)
    })

    expect(result.current.inferenceStatus).toMatchObject({ ready: true, source: 'runtime_check' })
  })

  it('does not present an initial transport failure as inference not ready', async () => {
    const requestGatewayMock = vi.fn(async (method: string) => {
      throw new Error(`${method} connection closed`)
    })
    const requestGateway = requestGatewayMock as unknown as GatewayRequester
    const { result } = renderHook(() => useStatusSnapshot('open', requestGateway))

    await flushAsync()

    expect(result.current.inferenceStatus).toBeNull()
  })

  it('still publishes an authoritative runtime failure', async () => {
    const requestGatewayMock = vi.fn(
      async (method: string) =>
        (method === 'setup.runtime_check'
          ? { error: 'No usable credentials found for nous.', ok: false }
          : { provider_configured: true }) as never
    )
    const requestGateway = requestGatewayMock as unknown as GatewayRequester
    const { result } = renderHook(() => useStatusSnapshot('open', requestGateway))

    await flushAsync()

    expect(result.current.inferenceStatus).toMatchObject({
      ready: false,
      reason: expect.stringContaining('No usable credentials found for nous.'),
      source: 'runtime_check'
    })
  })

  it('clears inference readiness immediately when the Desktop gateway transport disconnects', async () => {
    const requestGateway = vi.fn(
      async (method: string) =>
        (method === 'setup.runtime_check' ? { ok: true } : { provider_configured: true }) as never
    ) as unknown as GatewayRequester

    const { rerender, result } = renderHook(({ gatewayState }) => useStatusSnapshot(gatewayState, requestGateway), {
      initialProps: { gatewayState: 'open' }
    })

    await flushAsync()
    expect(result.current.inferenceStatus).toMatchObject({ ready: true, source: 'runtime_check' })

    rerender({ gatewayState: 'connecting' })

    expect(result.current.inferenceStatus).toBeNull()
  })

  it('waits for a slow inference refresh to settle before scheduling another one', async () => {
    const setup = deferred<unknown>()
    const runtime = deferred<unknown>()

    const requestGatewayMock = vi.fn(
      (method: string) => (method === 'setup.runtime_check' ? runtime.promise : setup.promise) as never
    )
    const requestGateway = requestGatewayMock as unknown as GatewayRequester

    renderHook(() => useStatusSnapshot('open', requestGateway))
    await flushAsync()

    expect(requestGatewayMock).toHaveBeenCalledTimes(2)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000)
    })

    expect(requestGatewayMock).toHaveBeenCalledTimes(2)

    await act(async () => {
      setup.resolve({ provider_configured: true })
      runtime.resolve({ ok: true })
      await vi.advanceTimersByTimeAsync(0)
    })

    await act(async () => {
      await vi.advanceTimersByTimeAsync(59_999)
    })
    expect(requestGatewayMock).toHaveBeenCalledTimes(2)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1)
    })
    expect(requestGatewayMock).toHaveBeenCalledTimes(4)
  })
})