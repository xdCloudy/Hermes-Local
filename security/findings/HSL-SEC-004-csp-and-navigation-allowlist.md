# HSL-SEC-004 — renderer lacked CSP and navigation used broad prefix checks

Severity: High  
Status: Fixed and packaged-runtime verification added  
Trust boundary: untrusted link/content → privileged Electron renderer

## Source to sink

The renderer had no explicit Content Security Policy. Its navigation guard accepted any packaged `file:` URL and used `startsWith(DEV_SERVER)` during development. A URL with trusted text in its user-info/prefix or an arbitrary local HTML file could therefore be treated as in-app navigation. A navigated page would inherit the BrowserWindow's preload bridge.

## Impact and reachability

Successful navigation of the privileged window to attacker-controlled content could expose narrow but meaningful native preload capabilities. Node integration and the sandbox limited impact, but this was a direct violation of the navigation and privileged-content requirements.

## Fix

- Navigation now accepts only the exact development origin or exact packaged renderer file URL (including its query/hash routes).
- New windows remain denied by default and are opened externally only after scheme/path validation.
- Added CSP: `default-src 'self'`, `script-src 'self'`, no inline/eval scripts, `object-src 'none'`, bounded connect/media/frame sources, `base-uri 'self'`, and `form-action 'self'`.
- Moved the pre-paint theme bootstrap to an external same-origin file so inline scripts are not required.
- Explicitly set `webSecurity: true` for renderer/helper windows.

## Verification

The packaged Electron Playwright test reads the effective CSP and attempts an inline script. Chromium blocks it. The same test also exercises the real workstation UI, dashboard, Sessions, Projects, and TUI while requiring zero unexpected renderer errors.
