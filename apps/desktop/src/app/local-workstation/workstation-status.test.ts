import { describe, expect, it } from 'vitest'

import type { LocalWorkstationSnapshot } from './types'
import { deriveLocalWorkstationStatus } from './workstation-status'

type GatewayOverride = Partial<NonNullable<LocalWorkstationSnapshot['health']['gateway']>>
type HealthOverride = Omit<Partial<LocalWorkstationSnapshot['health']>, 'gateway'> & {
  gateway?: GatewayOverride
}

function snapshot(health: HealthOverride): LocalWorkstationSnapshot {
  const { gateway, ...serviceHealth } = health

  return {
    health: {
      dashboard: true,
      hermes: true,
      model: true,
      ...serviceHealth,
      gateway: gateway
        ? {
            checked: gateway.checked ?? true,
            reachable: gateway.reachable ?? true,
            running: gateway.running ?? false,
            state: gateway.state ?? 'stopped',
            updatedAt: gateway.updatedAt ?? null
          }
        : undefined
    },
    model: {
      displayName: 'Portable test model'
    }
  } as LocalWorkstationSnapshot
}

describe('local workstation readiness', () => {
  it('shows Checking before an authoritative gateway result exists', () => {
    const result = deriveLocalWorkstationStatus(snapshot({}))

    expect(result.ready).toBe(false)
    expect(result.label).toBe('Checking gateway')
  })

  it('never reports Ready when the authoritative gateway is stopped', () => {
    const result = deriveLocalWorkstationStatus(
      snapshot({ gateway: { running: false, state: 'stopped' } })
    )

    expect(result.ready).toBe(false)
    expect(result.stackRunning).toBe(true)
    expect(result.label).toBe('Gateway stopped')
  })

  it('reports starting while the gateway starts', () => {
    const result = deriveLocalWorkstationStatus(
      snapshot({ gateway: { running: false, state: 'starting' } })
    )

    expect(result.ready).toBe(false)
    expect(result.label).toBe('Gateway starting')
  })

  it('reports unavailable when gateway status cannot be verified', () => {
    const result = deriveLocalWorkstationStatus(
      snapshot({ gateway: { reachable: false, running: false, state: 'unavailable' } })
    )

    expect(result.ready).toBe(false)
    expect(result.label).toBe('Gateway unavailable')
  })

  it('reports Ready only after every live check passes', () => {
    const result = deriveLocalWorkstationStatus(
      snapshot({ gateway: { running: true, state: 'running' } })
    )

    expect(result.ready).toBe(true)
    expect(result.label).toBe('Ready for local inference')
  })

  it('reports degraded when only part of the local stack is healthy', () => {
    const result = deriveLocalWorkstationStatus(
      snapshot({ hermes: false, gateway: { running: true, state: 'running' } })
    )

    expect(result.ready).toBe(false)
    expect(result.label).toBe('Stack degraded')
    expect(result.stackRunning).toBe(false)
  })
})