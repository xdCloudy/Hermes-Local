import { describe, expect, it } from 'vitest'

import type { RuntimeReadinessResult } from '@/lib/runtime-readiness'
import type { StatusResponse } from '@/types/hermes'

import { resolveGatewayStatusbarState } from './gateway-statusbar-state'

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

const readyInference: RuntimeReadinessResult = {
  checksDisagree: false,
  ready: true,
  reason: null,
  source: 'runtime_check'
}

describe('resolveGatewayStatusbarState', () => {
  it('never reports ready when authoritative gateway status is stopped', () => {
    expect(
      resolveGatewayStatusbarState({
        gatewayState: 'open',
        inferenceStatus: readyInference,
        statusSnapshot: status({
          gateway_pid: null,
          gateway_running: false,
          gateway_state: 'stopped'
        })
      })
    ).toMatchObject({
      kind: 'offline',
      reason: 'stopped'
    })
  })

  it('reports a starting authoritative gateway as connecting', () => {
    expect(
      resolveGatewayStatusbarState({
        gatewayState: 'open',
        inferenceStatus: readyInference,
        statusSnapshot: status({
          gateway_running: false,
          gateway_state: 'starting'
        })
      })
    ).toMatchObject({ kind: 'connecting' })
  })

  it('reports ready only after status and inference are both authoritative', () => {
    expect(
      resolveGatewayStatusbarState({
        gatewayState: 'open',
        inferenceStatus: readyInference,
        statusSnapshot: status()
      })
    ).toEqual({ kind: 'ready' })
  })

  it('reports checking while authoritative REST status is unavailable', () => {
    expect(
      resolveGatewayStatusbarState({
        gatewayState: 'open',
        inferenceStatus: readyInference,
        statusSnapshot: null
      })
    ).toEqual({ kind: 'checking' })
  })

  it('keeps inference setup state separate after the gateway process is running', () => {
    expect(
      resolveGatewayStatusbarState({
        gatewayState: 'open',
        inferenceStatus: {
          checksDisagree: false,
          ready: false,
          reason: 'Provider setup required',
          source: 'runtime_check'
        },
        statusSnapshot: status()
      })
    ).toMatchObject({
      kind: 'needs-setup',
      reason: 'Provider setup required'
    })
  })

  it('does not report ready when the Desktop transport is offline', () => {
    expect(
      resolveGatewayStatusbarState({
        gatewayState: 'closed',
        inferenceStatus: readyInference,
        statusSnapshot: status()
      })
    ).toEqual({ kind: 'offline' })
  })
})