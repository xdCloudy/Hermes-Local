import assert from 'node:assert/strict'

import { test } from 'vitest'

import { shouldAllowRendererPermission } from './media-permissions'

const packaged = {
  devServer: null,
  packagedRendererUrl: 'file:///C:/Program%20Files/Hermes/resources/app.asar/dist/index.html',
}

test('allows audio-only capture from an owned packaged renderer window', () => {
  assert.equal(
    shouldAllowRendererPermission(
      {
        details: { mediaTypes: ['audio'] },
        hasWindowOwner: true,
        permission: 'media',
        url: packaged.packagedRendererUrl,
      },
      packaged,
    ),
    true,
  )
})

test('denies video and non-media permissions by default', () => {
  for (const request of [
    { details: { mediaTypes: ['video'] }, permission: 'media' },
    { details: { mediaTypes: ['audio', 'video'] }, permission: 'media' },
    { details: {}, permission: 'notifications' },
    { details: {}, permission: 'geolocation' },
  ]) {
    assert.equal(
      shouldAllowRendererPermission(
        {
          ...request,
          hasWindowOwner: true,
          url: packaged.packagedRendererUrl,
        },
        packaged,
      ),
      false,
    )
  }
})

test('denies media from webviews and untrusted renderer URLs', () => {
  assert.equal(
    shouldAllowRendererPermission(
      {
        details: { mediaTypes: ['audio'] },
        hasWindowOwner: false,
        permission: 'media',
        url: packaged.packagedRendererUrl,
      },
      packaged,
    ),
    false,
  )
  assert.equal(
    shouldAllowRendererPermission(
      {
        details: { mediaTypes: ['audio'] },
        hasWindowOwner: true,
        permission: 'media',
        url: 'https://attacker.example/',
      },
      packaged,
    ),
    false,
  )
  assert.equal(
    shouldAllowRendererPermission(
      {
        details: { mediaTypes: ['audio'] },
        hasWindowOwner: true,
        permission: 'media',
        url: 'file:///C:/Windows/System32/drivers/etc/hosts',
      },
      packaged,
    ),
    false,
  )
})

test('matches a development renderer by exact origin, not a string prefix', () => {
  const locations = {
    devServer: 'http://127.0.0.1:5174',
    packagedRendererUrl: packaged.packagedRendererUrl,
  }

  const request = {
    details: { mediaTypes: ['audio'] },
    hasWindowOwner: true,
    permission: 'media',
  }

  assert.equal(shouldAllowRendererPermission({ ...request, url: 'http://127.0.0.1:5174/chat' }, locations), true)

  assert.equal(
    shouldAllowRendererPermission({ ...request, url: 'http://127.0.0.1:5174.attacker.example/' }, locations),
    false,
  )
})
