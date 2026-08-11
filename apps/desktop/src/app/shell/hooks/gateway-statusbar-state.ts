import type { RuntimeReadinessResult } from '@/lib/runtime-readiness'
import type { StatusResponse } from '@/types/hermes'

export type GatewayStatusbarKind = 'checking' | 'connecting' | 'needs-setup' | 'offline' | 'ready'

export interface GatewayStatusbarState {
  kind: GatewayStatusbarKind
  reason?: string
}

interface ResolveGatewayStatusbarStateOptions {
  gatewayState: string | undefined
  inferenceStatus: RuntimeReadinessResult | null
  statusSnapshot: StatusResponse | null
}

const STARTING_STATES = new Set(['initializing', 'restarting', 'start_pending', 'starting'])

export function resolveGatewayStatusbarState({
  gatewayState,
  inferenceStatus,
  statusSnapshot
}: ResolveGatewayStatusbarStateOptions): GatewayStatusbarState {
  const authoritativeState = statusSnapshot?.gateway_state?.trim().toLowerCase() || null

  // The REST status payload is authoritative for the messaging gateway process.
  // It must win over an independently open Desktop JSON-RPC/WebSocket transport.
  if (statusSnapshot && !statusSnapshot.gateway_running) {
    return {
      kind: authoritativeState && STARTING_STATES.has(authoritativeState) ? 'connecting' : 'offline',
      reason: statusSnapshot.gateway_exit_reason || statusSnapshot.gateway_state || undefined
    }
  }

  if (gatewayState === 'connecting') {
    return { kind: 'connecting' }
  }

  if (gatewayState !== 'open') {
    return { kind: 'offline' }
  }

  if (!statusSnapshot) {
    return { kind: 'checking' }
  }

  if (inferenceStatus?.ready === false) {
    return {
      kind: 'needs-setup',
      reason: inferenceStatus.reason ?? undefined
    }
  }

  if (inferenceStatus?.ready === true) {
    return { kind: 'ready' }
  }

  return { kind: 'checking' }
}