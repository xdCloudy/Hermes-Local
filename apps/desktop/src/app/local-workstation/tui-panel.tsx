import '@xterm/xterm/css/xterm.css'

import { IconPlayerPlay, IconRefresh, IconTerminal2 } from '@tabler/icons-react'
import { FitAddon } from '@xterm/addon-fit'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { Terminal } from '@xterm/xterm'
import { useCallback, useEffect, useRef, useState } from 'react'

import { Button } from '@/components/ui/button'

type TuiState = 'exited' | 'idle' | 'running' | 'starting'

export function HermesTuiPanel() {
  const hostRef = useRef<HTMLDivElement | null>(null)
  const [generation, setGeneration] = useState(0)
  const [pid, setPid] = useState<null | number>(null)
  const [state, setState] = useState<TuiState>('idle')
  const [exitCode, setExitCode] = useState<null | number>(null)

  const restart = useCallback(() => {
    setGeneration(value => value + 1)
  }, [])

  useEffect(() => {
    const host = hostRef.current

    if (!host) {
      return
    }

    let cancelled = false
    let unsubscribeData: () => void = () => undefined
    let unsubscribeExit: () => void = () => undefined
    let observer: null | ResizeObserver = null
    let sessionId: null | string = null

    const terminal = new Terminal({
      allowProposedApi: true,
      convertEol: true,
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: "'JetBrains Mono', 'Cascadia Mono', Consolas, monospace",
      fontSize: 13,
      scrollback: 10_000,
      theme: {
        background: '#111315',
        black: '#111315',
        blue: '#4f8dff',
        brightBlack: '#60656f',
        brightBlue: '#82adff',
        brightCyan: '#62d4e8',
        brightGreen: '#65d6a2',
        brightMagenta: '#c3a6ff',
        brightRed: '#ff8f8f',
        brightWhite: '#ffffff',
        brightYellow: '#f5d276',
        cyan: '#36b7d0',
        foreground: '#eef0f3',
        green: '#42bd88',
        magenta: '#a989ee',
        red: '#e56d6d',
        selectionBackground: '#4f8dff55',
        white: '#d9dde3',
        yellow: '#dbb950'
      }
    })

    const fit = new FitAddon()

    terminal.loadAddon(fit)
    terminal.loadAddon(new Unicode11Addon())
    terminal.unicode.activeVersion = '11'
    terminal.open(host)
    fit.fit()
    setState('starting')
    setExitCode(null)

    const start = async () => {
      try {
        const session = await window.hermesDesktop.terminal.start({
          cols: terminal.cols,
          mode: 'hermes-tui',
          rows: terminal.rows
        })

        if (cancelled) {
          await window.hermesDesktop.terminal.dispose(session.id)

          return
        }

        sessionId = session.id
        setPid(session.pid)
        setState('running')
        unsubscribeData = window.hermesDesktop.terminal.onData(session.id, data => terminal.write(data))
        unsubscribeExit = window.hermesDesktop.terminal.onExit(session.id, payload => {
          setExitCode(payload.code)
          setPid(null)
          setState('exited')
          terminal.writeln(`\r\n\x1b[90mHermes TUI exited with code ${payload.code ?? 'unknown'}.\x1b[0m`)
        })
        terminal.onData(data => {
          void window.hermesDesktop.terminal.write(session.id, data)
        })
        observer = new ResizeObserver(() => {
          fit.fit()
          void window.hermesDesktop.terminal.resize(session.id, {
            cols: terminal.cols,
            rows: terminal.rows
          })
        })
        observer.observe(host)
        terminal.focus()
      } catch (error) {
        terminal.writeln(`\r\n\x1b[31m${error instanceof Error ? error.message : String(error)}\x1b[0m`)
        setState('exited')
      }
    }

    void start()

    return () => {
      cancelled = true
      observer?.disconnect()
      unsubscribeData()
      unsubscribeExit()
      const id = sessionId

      sessionId = null
      terminal.dispose()

      if (id) {
        void window.hermesDesktop.terminal.dispose(id)
      }
    }
  }, [generation])

  return (
    <section className="flex h-full min-h-0 flex-col bg-(--ui-editor-surface-background)">
      <header className="flex h-11 shrink-0 items-center gap-3 border-b border-(--ui-stroke-secondary) px-4">
        <IconTerminal2 aria-hidden className="size-4 text-(--ui-accent)" stroke={1.8} />
        <div className="min-w-0">
          <p className="truncate text-[0.8125rem] font-semibold">Hermes TUI</p>
          <p className="text-[0.6875rem] text-(--ui-text-tertiary)">
            {state === 'running'
              ? `Connected · PID ${pid}`
              : state === 'starting'
                ? 'Starting local PTY…'
                : `Exited${exitCode === null ? '' : ` · ${exitCode}`}`}
          </p>
        </div>
        <div className="ml-auto">
          <Button className="h-7 gap-1.5 px-2 text-xs" onClick={restart} size="sm" variant="outline">
            {state === 'idle' ? <IconPlayerPlay className="size-3.5" /> : <IconRefresh className="size-3.5" />}
            {state === 'idle' ? 'Start' : 'Restart'}
          </Button>
        </div>
      </header>
      <div className="min-h-0 flex-1 bg-[#111315] p-2">
        <div className="h-full min-h-[18rem] w-full" ref={hostRef} />
      </div>
    </section>
  )
}
