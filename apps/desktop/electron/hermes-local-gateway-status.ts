export interface GatewayStatusSnapshot {
  checked: boolean
  reachable: boolean
  running: boolean
  state: string
  updatedAt: null | string
}

interface GatewayStatusPayload {
  gateway_running?: unknown
  gateway_state?: unknown
  gateway_updated_at?: unknown
}

type GatewayFetch = (
  input: string,
  init?: { signal?: AbortSignal }
) => Promise<{
  json: () => Promise<unknown>
  ok: boolean
}>

function unavailableGatewayStatus(): GatewayStatusSnapshot {
  return {
    checked: true,
    reachable: false,
    running: false,
    state: 'unavailable',
    updatedAt: null
  }
}

export function normalizeGatewayStatus(payload: unknown): GatewayStatusSnapshot {
  if (!payload || typeof payload !== 'object') {
    return {
      checked: true,
      reachable: true,
      running: false,
      state: 'unknown',
      updatedAt: null
    }
  }

  const record = payload as GatewayStatusPayload
  const running = record.gateway_running === true
  const suppliedState = typeof record.gateway_state === 'string' ? record.gateway_state.trim() : ''
  const updatedAt = typeof record.gateway_updated_at === 'string' ? record.gateway_updated_at : null

  return {
    checked: true,
    reachable: true,
    running,
    state: suppliedState || (running ? 'running' : 'stopped'),
    updatedAt
  }
}

export async function readGatewayStatus(
  url: string,
  fetchImpl: GatewayFetch = fetch
): Promise<GatewayStatusSnapshot> {
  try {
    const response = await fetchImpl(url, {
      signal: AbortSignal.timeout(2500)
    })

    if (!response.ok) {
      return unavailableGatewayStatus()
    }

    return normalizeGatewayStatus(await response.json())
  } catch {
    return unavailableGatewayStatus()
  }
}