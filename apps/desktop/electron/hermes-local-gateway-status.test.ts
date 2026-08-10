import { describe, expect, it, vi } from 'vitest'

import { normalizeGatewayStatus, readGatewayStatus } from './hermes-local-gateway-status'

describe('Hermes Local gateway status probe', () => {
  it('normalizes the authoritative running response', () => {
    expect(
      normalizeGatewayStatus({
        gateway_running: true,
        gateway_state: 'running',
        gateway_updated_at: '2026-07-29T21:00:00+00:00'
      })
    ).toEqual({
      checked: true,
      reachable: true,
      running: true,
      state: 'running',
      updatedAt: '2026-07-29T21:00:00+00:00'
    })
  })

  it('preserves non-running lifecycle states', () => {
    expect(normalizeGatewayStatus({ gateway_running: false, gateway_state: 'starting' })).toMatchObject({
      checked: true,
      reachable: true,
      running: false,
      state: 'starting'
    })

    expect(normalizeGatewayStatus({ gateway_running: false, gateway_state: 'stopped' })).toMatchObject({
      checked: true,
      reachable: true,
      running: false,
      state: 'stopped'
    })
  })

  it('returns unavailable when the status endpoint cannot be verified', async () => {
    const failedFetch = vi.fn(async () => {
      throw new Error('offline')
    })

    await expect(readGatewayStatus('http://localhost/api/status', failedFetch as never)).resolves.toEqual({
      checked: true,
      reachable: false,
      running: false,
      state: 'unavailable',
      updatedAt: null
    })
  })

  it('queries the supplied portable status URL', async () => {
    const fetchImpl = vi.fn(async (_input: string) => ({
      ok: true,
      json: async () => ({
        gateway_running: false,
        gateway_state: 'stopped',
        gateway_updated_at: null
      })
    }))

    const url = 'http://localhost:9119/api/status'
    const result = await readGatewayStatus(url, fetchImpl as never)

    expect(fetchImpl).toHaveBeenCalledOnce()
    expect(fetchImpl.mock.calls[0]?.[0]).toBe(url)
    expect(result).toMatchObject({
      checked: true,
      reachable: true,
      running: false,
      state: 'stopped'
    })
  })
})