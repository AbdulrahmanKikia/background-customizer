//! Desktop attachment. Operating-system calls stay behind [`DesktopHost`].

/// Solid color used until image wallpapers exist.
pub const LAYER_COLOR: Rgb = Rgb {
    red: 0,
    green: 114,
    blue: 178,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    /// `COLORREF` layout is `0x00BBGGRR`.
    pub fn colorref(self) -> u32 {
        u32::from(self.red) | (u32::from(self.green) << 8) | (u32::from(self.blue) << 16)
    }
}

/// Windows the shell observed after asking Progman to create a wallpaper window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedDesktop {
    pub progman: isize,
    pub defview: isize,
    pub defview_parent: isize,
    /// `WorkerW` that is a child of `defview_parent` (a sibling of the icon view).
    pub worker_child: isize,
    /// Next top-level `WorkerW` after `defview_parent`.
    pub worker_after_parent: isize,
    /// Progman was created with `WS_EX_NOREDIRECTIONBITMAP` (Windows 11 24H2+).
    pub progman_no_redirection: bool,
}

/// Where the wallpaper surface is reparented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperParent {
    /// A `WorkerW` that already sits behind the desktop icons.
    Worker { hwnd: isize },
    /// Layered child of Progman, under the icon view and above the system `WorkerW`.
    /// Required when Progman has `WS_EX_NOREDIRECTIONBITMAP`.
    BehindIcons { progman: isize, defview: isize },
}

/// Pick the window that should own the wallpaper surface.
///
/// On Windows 11 24H2+, Progman uses `WS_EX_NOREDIRECTIONBITMAP`. Microsoft's
/// guidance is a layered child of Progman, z-ordered under `SHELLDLL_DefView`
/// and above the wallpaper `WorkerW`. Older desktops keep the icons in one
/// `WorkerW` and the wallpaper surface in the next top-level `WorkerW`.
pub fn choose_parent(found: ObservedDesktop) -> Result<WallpaperParent, String> {
    if found.defview == 0 || found.defview_parent == 0 {
        return Err("desktop icons were not found".to_string());
    }
    if found.progman_no_redirection && found.progman != 0 && found.defview_parent == found.progman {
        return Ok(WallpaperParent::BehindIcons {
            progman: found.progman,
            defview: found.defview,
        });
    }
    if found.worker_child != 0 && found.worker_child != found.defview {
        return Ok(WallpaperParent::Worker {
            hwnd: found.worker_child,
        });
    }
    if found.worker_after_parent != 0 && found.worker_after_parent != found.defview_parent {
        return Ok(WallpaperParent::Worker {
            hwnd: found.worker_after_parent,
        });
    }
    if found.progman != 0 && found.defview_parent == found.progman {
        return Ok(WallpaperParent::BehindIcons {
            progman: found.progman,
            defview: found.defview,
        });
    }
    Err("WorkerW sibling is missing".to_string())
}

/// Owns the native wallpaper surface for one desktop session.
pub trait DesktopHost {
    fn attach_color(&mut self, color: Rgb) -> Result<(), String>;
    fn detach(&mut self) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::{choose_parent, DesktopHost, ObservedDesktop, Rgb, WallpaperParent, LAYER_COLOR};

    fn observed(
        progman: isize,
        defview: isize,
        defview_parent: isize,
        worker_child: isize,
        worker_after_parent: isize,
        progman_no_redirection: bool,
    ) -> ObservedDesktop {
        ObservedDesktop {
            progman,
            defview,
            defview_parent,
            worker_child,
            worker_after_parent,
            progman_no_redirection,
        }
    }

    #[test]
    fn no_redirection_progman_uses_a_layered_child_even_when_worker_exists() {
        let parent = choose_parent(observed(10, 11, 10, 12, 99, true)).expect("parent");
        assert_eq!(
            parent,
            WallpaperParent::BehindIcons {
                progman: 10,
                defview: 11,
            }
        );
    }

    #[test]
    fn classic_progman_child_worker_is_preferred_without_no_redirection() {
        let parent = choose_parent(observed(10, 11, 10, 12, 99, false)).expect("parent");
        assert_eq!(parent, WallpaperParent::Worker { hwnd: 12 });
    }

    #[test]
    fn classic_worker_sibling_is_used_when_icons_live_in_a_worker() {
        let parent = choose_parent(observed(1, 2, 3, 0, 4, false)).expect("parent");
        assert_eq!(parent, WallpaperParent::Worker { hwnd: 4 });
    }

    #[test]
    fn progman_without_a_worker_places_the_layer_behind_the_icons() {
        let parent = choose_parent(observed(10, 11, 10, 0, 0, false)).expect("parent");
        assert_eq!(
            parent,
            WallpaperParent::BehindIcons {
                progman: 10,
                defview: 11,
            }
        );
    }

    #[test]
    fn missing_icons_and_missing_worker_are_errors() {
        let missing_icons = choose_parent(observed(10, 0, 0, 0, 0, false));
        assert!(missing_icons.is_err());
        let missing_worker = choose_parent(observed(10, 11, 3, 0, 0, false));
        assert!(missing_worker.is_err());
    }

    #[test]
    fn colorref_packs_channels_as_blue_green_red() {
        assert_eq!(LAYER_COLOR.colorref(), 0x00B2_7200);
        assert_eq!(
            Rgb {
                red: 1,
                green: 2,
                blue: 3
            }
            .colorref(),
            0x0003_0201
        );
    }

    #[test]
    fn a_host_can_attach_and_detach_without_the_operating_system() {
        struct FakeHost {
            color: Option<Rgb>,
        }
        impl DesktopHost for FakeHost {
            fn attach_color(&mut self, color: Rgb) -> Result<(), String> {
                self.color = Some(color);
                Ok(())
            }
            fn detach(&mut self) -> Result<(), String> {
                self.color = None;
                Ok(())
            }
        }
        let mut host = FakeHost { color: None };
        host.attach_color(LAYER_COLOR).expect("attach");
        assert_eq!(host.color, Some(LAYER_COLOR));
        host.detach().expect("detach");
        assert_eq!(host.color, None);
    }
}
