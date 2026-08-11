import { useEffect, useState } from 'react'

import { getStatus } from '@/hermes'
import { evaluateRuntimeReadiness, type RuntimeReadinessResult } from '@/lib/runtime-readiness'
import type { StatusResponse } from '@/types/hermes'

const STATUS_REFRESH_MS = 2_000
const INFERENCE_REFRESH_MS = 60_000

type GatewayRequester = <T = unknown>(method: string, params?: Record<string, unknown>) => Promise<T>

export function useStatusSnapshot(gatewayState: string | undefined, requestGateway: GatewayRequester) {
  const [statusSnapshot, setStatusSnapshot] = useState<StatusResponse | null>(null)
  const [inferenceStatus, setInferenceStatus] = useState<RuntimeReadinessResult | null>(null)

  useEffect(() => {
    let cancelled = false
    let timer: number | undefined

    const scheduleRefresh = () => {
      if (!cancelled) {
        timer = window.setTimeout(() => void refresh(), STATUS_REFRESH_MS)
      }
    }

    const refresh = async () => {
      if (document.visibilityState !== 'visible') {
        scheduleRefresh()

        return
      }

      try {
        const status = await getStatus()

        if (!cancelled) {
          setStatusSnapshot(status)
        }
      } catch {
        // Never preserve a stale "running" result when the authoritative REST
        // status endpoint can no longer be verified.
        if (!cancelled) {
          setStatusSnapshot(null)
        }
      } finally {
        scheduleRefresh()
      }
    }

    const onVisible = () => {
      if (document.visibilityState === 'visible' && !cancelled) {
        if (timer !== undefined) {
          window.clearTimeout(timer)
        }

        void refresh()
      }
    }

    document.addEventListener('visibilitychange', onVisible)
    void refresh()

    return () => {
      cancelled = true
      document.removeEventListener('visibilitychange', onVisible)

      if (timer !== undefined) {
        window.clearTimeout(timer)
      }
    }
  }, [])

  useEffect(() => {
    let cancelled = false
    let timer: number | undefined

    if (gatewayState !== 'open') {
      setInferenceStatus(null)

      return
    }

    const scheduleRefresh = () => {
      if (!cancelled) {
        timer = window.setTimeout(() => void refresh(), INFERENCE_REFRESH_MS)
      }
    }

    const refresh = async () => {
      if (document.visibilityState !== 'visible') {
        scheduleRefresh()

        return
      }

      try {
        const inference = await evaluateRuntimeReadiness(requestGateway)

        if (!cancelled && inference.source !== 'fallback') {
          // A fallback means both RPCs failed or returned no authoritative
          // boolean. Keep the last authoritative inference result instead of
          // flashing a false setup failure during a transport flap.
          setInferenceStatus(inference)
        }
      } catch {
        // Preserve the last authoritative result through an unexpected probe
        // failure. Transport lifecycle changes still clear it immediately.
      } finally {
        scheduleRefresh()
      }
    }

    const onVisible = () => {
      if (document.visibilityState === 'visible' && !cancelled) {
        if (timer !== undefined) {
          window.clearTimeout(timer)
        }

        void refresh()
      }
    }

    document.addEventListener('visibilitychange', onVisible)
    void refresh()

    return () => {
      cancelled = true
      document.removeEventListener('visibilitychange', onVisible)

      if (timer !== undefined) {
        window.clearTimeout(timer)
      }
    }
  }, [gatewayState, requestGateway])

  return { inferenceStatus, statusSnapshot }
}