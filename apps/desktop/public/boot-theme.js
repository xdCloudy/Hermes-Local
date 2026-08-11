// Pre-paint the themed background before the module graph loads. This file is
// deliberately external so the renderer can enforce script-src 'self' without
// allowing inline script execution.
try {
  let background = localStorage.getItem('hermes-boot-background')
  let scheme = localStorage.getItem('hermes-boot-color-scheme')

  if (!background) {
    const dark = window.matchMedia('(prefers-color-scheme: dark)').matches
    background = dark ? '#111111' : '#f7f7f7'
    scheme = dark ? 'dark' : 'light'
  }

  document.documentElement.style.backgroundColor = background

  if (scheme === 'dark' || scheme === 'light') {
    document.documentElement.style.colorScheme = scheme
  }
} catch {
  // localStorage unavailable — keep UA defaults.
}
