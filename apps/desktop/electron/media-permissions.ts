export interface RendererPermissionRequest {
  details?: {
    mediaType?: string
    mediaTypes?: string[]
  }
  hasWindowOwner: boolean
  permission: string
  url: string
}

export interface TrustedRendererLocations {
  devServer?: string | null
  packagedRendererUrl: string
}

export function isAudioCapturePermission(
  permission: string,
  details: RendererPermissionRequest['details'] = {},
): boolean {
  if (permission === 'audioCapture') {
    return true
  }

  if (permission !== 'media') {
    return false
  }

  if (details?.mediaType === 'video') {
    return false
  }

  const mediaTypes = details?.mediaTypes

  if (!Array.isArray(mediaTypes) || mediaTypes.length === 0) {
    // Windows frequently omits mediaTypes for microphone requests.
    return true
  }

  return mediaTypes.includes('audio') && !mediaTypes.includes('video')
}

export function isTrustedRendererUrl(url: string, locations: TrustedRendererLocations): boolean {
  if (!url) {
    return false
  }

  if (locations.devServer) {
    try {
      return new URL(url).origin === new URL(locations.devServer).origin
    } catch {
      return false
    }
  }

  return (
    url === locations.packagedRendererUrl ||
    url.startsWith(`${locations.packagedRendererUrl}?`) ||
    url.startsWith(`${locations.packagedRendererUrl}#`)
  )
}

export function shouldAllowRendererPermission(
  request: RendererPermissionRequest,
  locations: TrustedRendererLocations,
): boolean {
  return (
    request.hasWindowOwner &&
    isTrustedRendererUrl(request.url, locations) &&
    isAudioCapturePermission(request.permission, request.details)
  )
}
