export type BundledSample = 'a' | 'b'

export type WallpaperSource =
  | { kind: 'bundled'; sample: BundledSample }
  | { kind: 'path'; path: string }

export type MonitorAssignment = {
  monitor_id: string
  fit: string
  wallpaper: WallpaperSource
}

export type Profile = {
  name: string
  monitors: MonitorAssignment[]
}

export type Config = {
  profiles: Profile[]
  active_profile: string | null
}

export type Monitor = {
  id: string
  name: string
  width_px: number
  height_px: number
  left_px: number
  top_px: number
  primary: boolean
}

export type AppState = {
  config: Config
  config_path: string
  monitors: Monitor[]
}

export type LibraryKind = 'bundled' | 'file'

export type LibraryItem = {
  id: string
  title: string
  kind: LibraryKind
  wallpaper: WallpaperSource
  thumb_data_url: string
}

export type NavId = 'library' | 'monitors' | 'playlists' | 'settings'

export function wallpaperLabel(wallpaper: WallpaperSource | undefined): string {
  if (!wallpaper) {
    return 'Unset'
  }
  if (wallpaper.kind === 'bundled') {
    return `Sample ${wallpaper.sample.toUpperCase()}`
  }
  const parts = wallpaper.path.split(/[/\\]/)
  return parts[parts.length - 1] || wallpaper.path
}

export function assignmentFor(
  state: AppState,
  monitorId: string,
): MonitorAssignment | undefined {
  const active = state.config.profiles.find(
    (profile) => profile.name === state.config.active_profile,
  )
  return active?.monitors.find((item) => item.monitor_id === monitorId)
}
