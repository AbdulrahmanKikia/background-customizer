import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

type BundledSample = 'a' | 'b'

type WallpaperSource =
  | { kind: 'bundled'; sample: BundledSample }
  | { kind: 'path'; path: string }

type MonitorAssignment = {
  monitor_id: string
  fit: string
  wallpaper: WallpaperSource
}

type Profile = {
  name: string
  monitors: MonitorAssignment[]
}

type Config = {
  profiles: Profile[]
  active_profile: string | null
}

type Monitor = {
  id: string
  name: string
  width_px: number
  height_px: number
  left_px: number
  top_px: number
  primary: boolean
}

type AppState = {
  config: Config
  config_path: string
  monitors: Monitor[]
}

function wallpaperLabel(wallpaper: WallpaperSource | undefined): string {
  if (!wallpaper) {
    return 'unset'
  }
  if (wallpaper.kind === 'bundled') {
    return `bundled ${wallpaper.sample.toUpperCase()}`
  }
  return `path ${wallpaper.path}`
}

export default function App() {
  const [state, setState] = useState<AppState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function refresh() {
    const next = await invoke<AppState>('get_state')
    setState(next)
    setError(null)
  }

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        await refresh()
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err))
        }
      }
    })()
    const unlistenConfig = listen<Config>('config-changed', () => {
      void refresh()
    })
    const unlistenMonitors = listen<Monitor[]>('monitors-changed', () => {
      void refresh()
    })
    return () => {
      cancelled = true
      void unlistenConfig.then((unlisten) => unlisten())
      void unlistenMonitors.then((unlisten) => unlisten())
    }
  }, [])

  async function setSample(monitorId: string, sample: BundledSample) {
    setBusy(true)
    try {
      await invoke('set_monitor_wallpaper', {
        monitorId,
        wallpaper: { kind: 'bundled', sample },
      })
      await refresh()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const active = state?.config.profiles.find(
    (profile) => profile.name === state.config.active_profile,
  )

  return (
    <main className="min-h-svh bg-zinc-950 px-6 py-8 text-zinc-100">
      <h1 className="text-xl font-medium tracking-tight">Background Customizer</h1>
      <p className="mt-1 max-w-xl text-sm text-zinc-400">
        Milestone 5 IPC stub — typed get/set commands and events. Library and
        monitor picker arrive in Milestone 6.
      </p>

      {error ? (
        <p className="mt-4 text-sm text-red-400" role="alert">
          {error}
        </p>
      ) : null}

      {state ? (
        <section className="mt-6 space-y-4">
          <p className="text-xs text-zinc-500 break-all">
            Config: {state.config_path}
          </p>
          <ul className="space-y-3">
            {state.monitors.map((monitor) => {
              const assignment = active?.monitors.find(
                (item) => item.monitor_id === monitor.id,
              )
              return (
                <li
                  key={monitor.id}
                  className="border-b border-zinc-800 pb-3 text-sm"
                >
                  <div className="font-medium">
                    {monitor.name || monitor.id}
                    {monitor.primary ? ' (primary)' : ''}
                  </div>
                  <div className="text-zinc-400">
                    {monitor.width_px}×{monitor.height_px} ·{' '}
                    {wallpaperLabel(assignment?.wallpaper)}
                  </div>
                  <div className="mt-2 flex gap-2">
                    <button
                      type="button"
                      disabled={busy}
                      className="rounded bg-teal-700 px-3 py-1 text-xs disabled:opacity-50"
                      onClick={() => void setSample(monitor.id, 'a')}
                    >
                      Sample A
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      className="rounded bg-violet-700 px-3 py-1 text-xs disabled:opacity-50"
                      onClick={() => void setSample(monitor.id, 'b')}
                    >
                      Sample B
                    </button>
                  </div>
                </li>
              )
            })}
          </ul>
        </section>
      ) : (
        <p className="mt-6 text-sm text-zinc-500">Loading state…</p>
      )}
    </main>
  )
}
