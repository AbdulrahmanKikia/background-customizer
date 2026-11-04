import { Images, LayoutGrid, ListMusic, Settings2 } from 'lucide-react'
import type { NavId } from '@/types'
import { cn } from '@/lib/utils'

const items: Array<{
  id: NavId
  label: string
  icon: typeof Images
  enabled: boolean
}> = [
  { id: 'library', label: 'Library', icon: Images, enabled: true },
  { id: 'monitors', label: 'Monitors', icon: LayoutGrid, enabled: true },
  { id: 'playlists', label: 'Playlists', icon: ListMusic, enabled: false },
  { id: 'settings', label: 'Settings', icon: Settings2, enabled: false },
]

type Props = {
  active: NavId
  onChange: (id: NavId) => void
}

export function Sidebar({ active, onChange }: Props) {
  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
      <div className="border-b border-sidebar-border px-4 py-5">
        <p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
          Background
        </p>
        <h1 className="mt-1 text-step-3 font-medium tracking-tight">Customizer</h1>
      </div>
      <nav className="flex flex-1 flex-col gap-1 p-3" aria-label="Settings sections">
        {items.map((item) => {
          const Icon = item.icon
          const selected = active === item.id
          return (
            <button
              key={item.id}
              type="button"
              disabled={!item.enabled}
              aria-current={selected ? 'page' : undefined}
              onClick={() => onChange(item.id)}
              className={cn(
                'flex items-center gap-3 rounded-md px-3 py-2 text-left text-step-1 transition-[background-color,color,opacity] duration-[var(--duration-fast)] ease-[var(--ease-out)]',
                selected
                  ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                  : 'text-muted-foreground hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground',
                !item.enabled && 'cursor-not-allowed opacity-40 hover:bg-transparent hover:text-muted-foreground',
              )}
            >
              <Icon className="size-4 shrink-0" aria-hidden />
              <span>{item.label}</span>
              {!item.enabled ? (
                <span className="ml-auto text-[10px] uppercase tracking-wide">Soon</span>
              ) : null}
            </button>
          )
        })}
      </nav>
    </aside>
  )
}
