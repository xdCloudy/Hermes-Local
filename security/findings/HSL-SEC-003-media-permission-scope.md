# HSL-SEC-003 — microphone permission was not scoped to the trusted renderer

Severity: Medium  
Status: Fixed and regression-tested  
Trust boundary: remote/webview content → Chromium permission broker

## Source to sink

The default-session permission handlers allowed audio capture based only on the permission type. They did not verify that the request came from an owned Hermes renderer window rather than webview or other default-session content.

## Impact and reachability

The operating system still applies Windows microphone privacy controls, but hostile content rendered in the default session could reach an application-level allow decision. This violated fail-closed permission handling.

## Fix

Permission approval now requires all of:

1. an owning `BrowserWindow`;
2. the exact packaged renderer URL or exact development origin;
3. an audio-only media request.

Video, geolocation, notifications, webviews, malformed URLs, and all other origins are denied.

## Verification

`media-permissions.test.ts` covers audio-only success and denial for video, mixed media, non-media permissions, unowned webviews, hostile origins, arbitrary file URLs, and origin-prefix confusion. Electron unit tests, typecheck, and ESLint pass.
