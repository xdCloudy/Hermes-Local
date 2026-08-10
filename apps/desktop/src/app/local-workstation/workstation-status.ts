import type { LocalWorkstationSnapshot } from './types'

export interface LocalWorkstationPresentation {
  description: string
  label: string
  ready: boolean
  stackRunning: boolean
  title: string
}

export function deriveLocalWorkstationStatus(
  snapshot: LocalWorkstationSnapshot
): LocalWorkstationPresentation {
  const { health } = snapshot
  const stackRunning = health.model && health.hermes && health.dashboard
  const anyServiceRunning = health.model || health.hermes || health.dashboard

  if (!stackRunning) {
    if (anyServiceRunning) {
      return {
        description: 'One or more local services failed their live health check. Review the service logs before restarting.',
        label: 'Stack degraded',
        ready: false,
        stackRunning: false,
        title: 'Local workstation is partially available'
      }
    }

    return {
      description: 'The model and Hermes services are stopped. User data and the verified model remain on disk.',
      label: 'Stack stopped',
      ready: false,
      stackRunning: false,
      title: 'Start the local workstation'
    }
  }

  const gateway = health.gateway

  if (!gateway?.checked) {
    return {
      description: 'The local services are online while the Desktop verifies the authoritative gateway state.',
      label: 'Checking gateway',
      ready: false,
      stackRunning: true,
      title: 'Verifying gateway state'
    }
  }

  if (!gateway.reachable) {
    return {
      description: 'The model and dashboard are online, but the gateway status endpoint could not be verified.',
      label: 'Gateway unavailable',
      ready: false,
      stackRunning: true,
      title: 'Gateway status is unavailable'
    }
  }

  if (gateway.running) {
    return {
      description: 'The model, dashboard, and gateway have all passed live authoritative checks.',
      label: 'Ready for local inference',
      ready: true,
      stackRunning: true,
      title: `${snapshot.model.displayName}, Hermes, and the gateway are online`
    }
  }

  if (gateway.state === 'starting' || gateway.state === 'restarting') {
    return {
      description: 'The model and dashboard are online and the gateway is still starting.',
      label: 'Gateway starting',
      ready: false,
      stackRunning: true,
      title: 'Waiting for the gateway'
    }
  }

  if (['startup_failed', 'failed', 'crashed'].includes(gateway.state)) {
    return {
      description: 'The local services are online, but the gateway failed to start. Open the Web Dashboard or logs for details.',
      label: 'Gateway failed',
      ready: false,
      stackRunning: true,
      title: 'Gateway startup failed'
    }
  }

  return {
    description: 'The model and dashboard are online, but the gateway is stopped.',
    label: 'Gateway stopped',
    ready: false,
    stackRunning: true,
    title: 'Start or restart the gateway'
  }
}