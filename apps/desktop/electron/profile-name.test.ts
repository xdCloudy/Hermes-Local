import assert from 'node:assert/strict'

import { test } from 'vitest'

import { normalizeBackendProfile, PROFILE_NAME_RE } from './profile-name'

test('profile validation accepts the CLI-compatible identifier grammar', () => {
  for (const profile of ['default', 'research', 'worker-2', 'deep_research']) {
    assert.match(profile, PROFILE_NAME_RE)
    assert.equal(normalizeBackendProfile(profile, 'default'), profile)
  }
})

test('profile validation uses the trusted fallback for an omitted value', () => {
  assert.equal(normalizeBackendProfile(undefined, 'research'), 'research')
  assert.equal(normalizeBackendProfile('  ', 'default'), 'default')
})

test('profile validation rejects shell syntax and path traversal', () => {
  for (const profile of ['prod & calc.exe', '$(whoami)', '../default', 'name;whoami', 'Uppercase']) {
    assert.throws(() => normalizeBackendProfile(profile, 'default'), /Invalid profile name/)
  }
})

test('profile validation rejects non-string IPC values', () => {
  assert.throws(() => normalizeBackendProfile({ profile: 'default' }, 'default'), /expected a string/)
})
