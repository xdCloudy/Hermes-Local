import { describe, expect, it } from 'vitest'

import { sanitizeTrustPolicyInput, trustCliEnvironment } from './hermes-local-trust-centre'

describe('Hermes Local Trust Centre native bridge', () => {
  it('rejects renderer-supplied unsupported authority', () => {
    expect(() =>
      sanitizeTrustPolicyInput({
        capabilities: ['root.everything'],
        confirmation: 'never',
        integrationId: 'mcp:demo',
        scope: { kind: 'global' },
        state: 'user-trusted'
      })
    ).toThrow(/unknown capability/i)

    expect(() =>
      sanitizeTrustPolicyInput({
        capabilities: [],
        confirmation: 'never',
        integrationId: 'mcp:demo',
        scope: { kind: 'global' },
        state: 'built-in-verified'
      })
    ).toThrow(/mutable trust state/i)
  })

  it('requires bounded ids for scoped grants', () => {
    expect(() =>
      sanitizeTrustPolicyInput({
        capabilities: ['process.execute'],
        confirmation: 'always',
        integrationId: 'mcp:demo',
        scope: { kind: 'project', id: '../escape' },
        state: 'reviewed-managed'
      })
    ).toThrow(/scope id/i)
  })

  it('does not inherit unrelated secrets into the managed trust helper', () => {
    const environment = trustCliEnvironment('D:\\Hermes-Local', {
      SystemRoot: 'C:\\Windows',
      PATH: 'C:\\Windows\\System32',
      OPENAI_API_KEY: 'secret',
      HERMES_REMOTE_TOKEN: 'also-secret'
    })

    expect(environment.SystemRoot).toBe('C:\\Windows')
    expect(environment.HERMES_LOCAL_ROOT).toBe('D:\\Hermes-Local')
    expect(environment.HERMES_HOME).toBe('D:\\Hermes-Local\\data\\hermes')
    expect(environment.OPENAI_API_KEY).toBeUndefined()
    expect(environment.HERMES_REMOTE_TOKEN).toBeUndefined()
  })
})
