// Shared validation for profile identifiers crossing the renderer/main-process
// trust boundary. Keep this in sync with hermes_cli.profiles._PROFILE_ID_RE.

export const PROFILE_NAME_RE = /^[a-z0-9][a-z0-9_-]{0,63}$/

export function normalizeBackendProfile(value: unknown, fallback: string): string {
  if (value == null || (typeof value === 'string' && !value.trim())) {
    return fallback
  }

  if (typeof value !== 'string') {
    throw new Error('Invalid profile name: expected a string')
  }

  const profile = value.trim()

  if (profile !== 'default' && !PROFILE_NAME_RE.test(profile)) {
    throw new Error(`Invalid profile name: ${profile}`)
  }

  return profile
}
