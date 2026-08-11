export type TrustState =
  | 'built-in-verified'
  | 'reviewed-managed'
  | 'user-trusted'
  | 'restricted'
  | 'quarantined'
  | 'disabled'
  | 'unknown'

export type TrustConfirmation = 'never' | 'always' | 'writes-or-side-effects' | 'local-only'

export type TrustCapability =
  | 'filesystem.read'
  | 'filesystem.write'
  | 'filesystem.delete'
  | 'process.execute'
  | 'network.outbound'
  | 'network.listen'
  | 'browser.control'
  | 'clipboard.read'
  | 'clipboard.write'
  | 'media.microphone'
  | 'media.camera'
  | 'credentials.use'
  | 'credentials.manage'
  | 'communications.read'
  | 'communications.send'
  | 'calendar.read'
  | 'calendar.write'
  | 'external.side-effect'
  | 'runtime.read'
  | 'runtime.manage'
  | 'project.read'
  | 'project.write'
  | 'index.read'
  | 'index.write'
  | 'audit.read'
  | 'remote.connect'
  | 'remote.admin'
  | 'embedded.render'
  | 'embedded.navigate'

export type TrustScopeKind = 'agent' | 'global' | 'profile' | 'project' | 'session' | 'user'

export interface TrustIdentity {
  approvedManifestSha256?: string
  declaredCapabilities: TrustCapability[]
  displayName: string
  healthState: 'degraded' | 'healthy' | 'stopped' | 'unhealthy' | 'unknown'
  id: string
  integrationType:
    | 'browser-bridge'
    | 'built-in-tool'
    | 'executable'
    | 'mcp-local'
    | 'mcp-remote'
    | 'project-integration'
    | 'remote-client'
    | 'skill'
  manifestRevision: number
  provenance: {
    provenanceId: string
    revision: string
    sha256?: string
    sourceType: 'built-in' | 'github' | 'local-path' | 'package' | 'remote-endpoint'
    uri?: string
  }
  trustState: TrustState
}

export interface TrustIntegrationRecord {
  identity: TrustIdentity
  accessScopes?: Array<{ id?: string; kind: TrustScopeKind }>
  accessPrincipals?: Array<{ id: string; kind: string }>
  sourceLabel?: string
  lastSeenAt?: string
  serverName?: string
  source?: string
  transport?: 'remote' | 'stdio'
  unknownCapabilities?: string[]
}

export interface TrustGrant {
  capability: TrustCapability
  confirmation: TrustConfirmation
  constraints: { integrationIds?: string[] }
  effect: 'allow' | 'deny'
  id: string
  principal: { id: string; kind: string }
  scope: { id?: string; kind: TrustScopeKind }
}

export interface TrustAuditEvent {
  details: Record<string, boolean | number | string | string[]>
  eventType: string
  id: string
  outcome: 'allowed' | 'denied' | 'failure' | 'success'
  timestamp: string
}

export interface TrustCentreSnapshot {
  activeGrants: TrustGrant[]
  alerts: Array<{ integrationId?: string; reason: string; severity: 'high' | 'info' }>
  auditSummary: {
    counts: Record<string, number>
    recent: TrustAuditEvent[]
  }
  generatedAt: string
  integrations: TrustIntegrationRecord[]
  schemaVersion: 1
}

export interface TrustPolicyInput {
  capabilities: TrustCapability[]
  confirmation: TrustConfirmation
  integrationId: string
  scope: { id?: string; kind: TrustScopeKind }
  state: Exclude<TrustState, 'built-in-verified'>
}
