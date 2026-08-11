import { useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Loader2, Zap } from '@/lib/icons'

import { ListRow, SectionHeading, SettingsContent } from './primitives'

interface LoginItemStatus {
  available: boolean
  enabled: boolean
  executable: string
}

export function StartupSettings() {
  const [status, setStatus] = useState<LoginItemStatus | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    void window.hermesDesktop.localWorkstation.loginItem
      .get()
      .then(value => {
        setStatus(value)
        setError('')
      })
      .catch(nextError => setError(nextError instanceof Error ? nextError.message : String(nextError)))
  }, [])

  const toggle = async () => {
    if (!status?.available || saving) {
      return
    }

    setSaving(true)
    try {
      setStatus(await window.hermesDesktop.localWorkstation.loginItem.set(!status.enabled))
      setError('')
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError))
    } finally {
      setSaving(false)
    }
  }

  return (
    <SettingsContent>
      <div className="mx-auto w-full max-w-2xl pt-3">
        <SectionHeading icon={Zap} title="Startup" />
        <ListRow
          action={
            <Button
              aria-label={`${status?.enabled ? 'Disable' : 'Enable'} launch at sign-in`}
              disabled={!status?.available || saving}
              onClick={() => void toggle()}
              size="sm"
              variant={status?.enabled ? 'default' : 'outline'}
            >
              {saving && <Loader2 className="size-3 animate-spin" />}
              {status?.enabled ? 'Enabled' : 'Enable'}
            </Button>
          }
          description="Register the running Hermes Launcher for the current Windows user. No elevation or scheduled task is required."
          hint={status?.executable || 'Reading the active launcher executable…'}
          title="Launch Hermes at sign-in"
        />
        {status && !status.available && (
          <p className="mt-3 rounded-lg border border-border/70 bg-muted/20 px-4 py-3 text-xs text-muted-foreground">
            Launch at sign-in is available only in the Windows workstation.
          </p>
        )}
        {error && (
          <p className="mt-3 rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3 text-xs text-destructive">
            {error}
          </p>
        )}
      </div>
    </SettingsContent>
  )
}
