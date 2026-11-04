import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { LibraryView } from '@/components/LibraryView'
import { MonitorPicker } from '@/components/MonitorPicker'
import { Sidebar } from '@/components/Sidebar'
import type { AppState, Config, LibraryItem, Monitor, NavId } from '@/types'

export default function App() {
  const [nav, setNav] = useState<NavId>('library')
  const [state, setState] = useState<AppState | null>(null)
  const [library, setLibrary] = useState<LibraryItem[]>([])
  const [selectedLibraryId, setSelectedLibraryId] = useState<string | null>(null)
  const [selectedMonitorId, setSelectedMonitorId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function refreshState() {
    const next = await invoke<AppState>('get_state')
    setState(next)
    setSelectedMonitorId((current) => {
      if (current && next.monitors.some((monitor) => monitor.id === current)) {
        return current
      }
      return next.monitors.find((monitor) => monitor.primary)?.id ?? next.monitors[0]?.id ?? null
    })
  }

  async function refreshLibrary() {
    const items = await invoke<LibraryItem[]>('list_library')
    setLibrary(items)
    setSelectedLibraryId((current) => {
      if (current && items.some((item) => item.id === current)) {
        return current
      }
      return items[0]?.id ?? null
    })
  }

  async function refreshAll() {
    await Promise.all([refreshState(), refreshLibrary()])
    setError(null)
  }

  useEffect(() => {
    document.documentElement.classList.add('dark')
    let cancelled = false
    void (async () => {
      try {
        await refreshAll()
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err))
        }
      }
    })()
    const unlistenConfig = listen<Config>('config-changed', () => {
      void refreshState()
    })
    const unlistenMonitors = listen<Monitor[]>('monitors-changed', () => {
      void refreshState()
    })
    return () => {
      cancelled = true
      void unlistenConfig.then((unlisten) => unlisten())
      void unlistenMonitors.then((unlisten) => unlisten())
    }
  }, [])

  async function onImport() {
    setBusy(true)
    setStatus(null)
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Images',
            extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp'],
          },
        ],
      })
      if (!selected || Array.isArray(selected)) {
        return
      }
      const item = await invoke<LibraryItem>('import_wallpaper', { path: selected })
      await refreshLibrary()
      setSelectedLibraryId(item.id)
      setNav('library')
      setStatus(`Imported “${item.title}”. Select a monitor and apply.`)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  async function onApply() {
    if (!selectedMonitorId || !selectedLibraryId) {
      return
    }
    setBusy(true)
    setStatus(null)
    try {
      await invoke('apply_library_wallpaper', {
        monitorId: selectedMonitorId,
        libraryId: selectedLibraryId,
      })
      await refreshState()
      const title =
        library.find((item) => item.id === selectedLibraryId)?.title ?? 'wallpaper'
      setStatus(`Applied “${title}” to the selected monitor.`)
      setNav('monitors')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex min-h-svh bg-background text-foreground">
      <Sidebar active={nav} onChange={setNav} />
      <main className="flex min-w-0 flex-1 flex-col">
        <div className="flex-1 overflow-hidden p-6">
          {!state ? (
            <div className="flex h-full items-center justify-center text-step-1 text-muted-foreground">
              Loading settings…
            </div>
          ) : nav === 'library' ? (
            <LibraryView
              items={library}
              selectedId={selectedLibraryId}
              busy={busy}
              onSelect={setSelectedLibraryId}
              onImport={() => void onImport()}
            />
          ) : (
            <MonitorPicker
              state={state}
              library={library}
              selectedMonitorId={selectedMonitorId}
              selectedLibraryId={selectedLibraryId}
              busy={busy}
              onSelectMonitor={setSelectedMonitorId}
              onApply={() => void onApply()}
            />
          )}
        </div>
        <footer className="border-t border-border px-6 py-3 text-step-1">
          {error ? (
            <p className="text-destructive" role="alert">
              {error}
            </p>
          ) : status ? (
            <p className="text-muted-foreground">{status}</p>
          ) : (
            <p className="text-muted-foreground">
              Workflow: Library → select image → Monitors → Apply
            </p>
          )}
        </footer>
      </main>
    </div>
  )
}
