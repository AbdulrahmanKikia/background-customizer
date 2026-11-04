import { FolderPlus, ImagePlus } from 'lucide-react'
import type { LibraryItem } from '@/types'
import { cn } from '@/lib/utils'

type Props = {
  items: LibraryItem[]
  selectedId: string | null
  busy: boolean
  onSelect: (id: string) => void
  onImport: () => void
}

export function LibraryView({
  items,
  selectedId,
  busy,
  onSelect,
  onImport,
}: Props) {
  return (
    <section className="flex h-full min-h-0 flex-col gap-4">
      <header className="flex items-end justify-between gap-4">
        <div>
          <h2 className="text-step-3 font-medium tracking-tight">Library</h2>
          <p className="mt-1 text-step-1 text-muted-foreground">
            Choose a still image, then apply it from Monitors.
          </p>
        </div>
        <button
          type="button"
          disabled={busy}
          onClick={onImport}
          className="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-step-1 font-medium text-primary-foreground transition-opacity duration-[var(--duration-fast)] ease-[var(--ease-out)] disabled:opacity-50"
        >
          <FolderPlus className="size-4" aria-hidden />
          Import image
        </button>
      </header>

      {items.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-border bg-card/40 text-muted-foreground">
          <ImagePlus className="size-8 opacity-60" aria-hidden />
          <p className="text-step-1">No wallpapers yet</p>
        </div>
      ) : (
        <ul className="grid min-h-0 flex-1 grid-cols-[repeat(auto-fill,minmax(180px,1fr))] content-start gap-3 overflow-y-auto pb-2">
          {items.map((item) => {
            const selected = selectedId === item.id
            return (
              <li key={item.id}>
                <button
                  type="button"
                  aria-pressed={selected}
                  onClick={() => onSelect(item.id)}
                  className={cn(
                    'group flex w-full flex-col overflow-hidden rounded-lg border bg-card text-left transition-[border-color,box-shadow,transform] duration-[var(--duration-base)] ease-[var(--ease-out)]',
                    selected
                      ? 'border-ring shadow-[0_0_0_1px_var(--ring)]'
                      : 'border-border hover:border-muted-foreground/40',
                  )}
                >
                  <div className="relative aspect-video overflow-hidden bg-muted">
                    <img
                      src={item.thumb_data_url}
                      alt=""
                      className="size-full object-cover transition-transform duration-[var(--duration-base)] ease-[var(--ease-out)] group-hover:scale-[1.03]"
                    />
                    <span className="absolute left-2 top-2 rounded bg-background/80 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-foreground backdrop-blur-sm">
                      {item.kind}
                    </span>
                  </div>
                  <div className="px-3 py-2">
                    <p className="truncate text-step-1 font-medium">{item.title}</p>
                  </div>
                </button>
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
}
