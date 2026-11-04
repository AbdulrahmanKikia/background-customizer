import type { AppState, LibraryItem, Monitor } from '@/types'
import { assignmentFor, wallpaperLabel } from '@/types'
import { cn } from '@/lib/utils'

type Props = {
  state: AppState
  library: LibraryItem[]
  selectedMonitorId: string | null
  selectedLibraryId: string | null
  busy: boolean
  onSelectMonitor: (id: string) => void
  onApply: () => void
}

function layoutBounds(monitors: Monitor[]) {
  const minLeft = Math.min(...monitors.map((m) => m.left_px))
  const minTop = Math.min(...monitors.map((m) => m.top_px))
  const maxRight = Math.max(...monitors.map((m) => m.left_px + m.width_px))
  const maxBottom = Math.max(...monitors.map((m) => m.top_px + m.height_px))
  return {
    minLeft,
    minTop,
    width: Math.max(maxRight - minLeft, 1),
    height: Math.max(maxBottom - minTop, 1),
  }
}

export function MonitorPicker({
  state,
  library,
  selectedMonitorId,
  selectedLibraryId,
  busy,
  onSelectMonitor,
  onApply,
}: Props) {
  const bounds = layoutBounds(state.monitors)
  const selectedLibrary = library.find((item) => item.id === selectedLibraryId)
  const canApply = Boolean(selectedMonitorId && selectedLibraryId) && !busy

  return (
    <section className="flex h-full min-h-0 flex-col gap-4">
      <header className="flex items-end justify-between gap-4">
        <div>
          <h2 className="text-step-3 font-medium tracking-tight">Monitors</h2>
          <p className="mt-1 text-step-1 text-muted-foreground">
            Select a display, then apply the library image you chose.
          </p>
        </div>
        <button
          type="button"
          disabled={!canApply}
          onClick={onApply}
          className="rounded-md bg-primary px-3 py-2 text-step-1 font-medium text-primary-foreground transition-opacity duration-[var(--duration-fast)] ease-[var(--ease-out)] disabled:opacity-40"
        >
          Apply to monitor
        </button>
      </header>

      <div className="rounded-lg border border-border bg-card/50 px-4 py-3 text-step-1 text-muted-foreground">
        {selectedLibrary ? (
          <p>
            Selected wallpaper:{' '}
            <span className="text-foreground">{selectedLibrary.title}</span>
          </p>
        ) : (
          <p>Pick an image in Library first.</p>
        )}
      </div>

      <div className="relative min-h-[280px] flex-1 overflow-hidden rounded-lg border border-border bg-[radial-gradient(circle_at_top,oklch(0.22_0_0),oklch(0.16_0_0))] p-6">
        <div
          className="relative mx-auto h-full w-full max-w-3xl"
          style={{ aspectRatio: `${bounds.width} / ${bounds.height}` }}
        >
          {state.monitors.map((monitor) => {
            const selected = selectedMonitorId === monitor.id
            const assignment = assignmentFor(state, monitor.id)
            const left = ((monitor.left_px - bounds.minLeft) / bounds.width) * 100
            const top = ((monitor.top_px - bounds.minTop) / bounds.height) * 100
            const width = (monitor.width_px / bounds.width) * 100
            const height = (monitor.height_px / bounds.height) * 100
            return (
              <button
                key={monitor.id}
                type="button"
                aria-pressed={selected}
                onClick={() => onSelectMonitor(monitor.id)}
                className={cn(
                  'absolute flex flex-col justify-between overflow-hidden rounded-md border p-3 text-left transition-[border-color,background-color,transform] duration-[var(--duration-base)] ease-[var(--ease-out)]',
                  selected
                    ? 'border-ring bg-primary/15 shadow-[0_0_0_1px_var(--ring)]'
                    : 'border-border/80 bg-background/40 hover:bg-background/55',
                )}
                style={{
                  left: `${left}%`,
                  top: `${top}%`,
                  width: `${width}%`,
                  height: `${height}%`,
                }}
              >
                <div>
                  <p className="truncate text-step-1 font-medium text-foreground">
                    {monitor.name || monitor.id}
                    {monitor.primary ? ' · Primary' : ''}
                  </p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    {monitor.width_px}×{monitor.height_px}
                  </p>
                </div>
                <p className="truncate text-[11px] text-muted-foreground">
                  {wallpaperLabel(assignment?.wallpaper)}
                </p>
              </button>
            )
          })}
        </div>
      </div>
    </section>
  )
}
