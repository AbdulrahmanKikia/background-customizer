//! Resident-process policy for the tray.
//!
//! Pause does not touch wallpaper playback. That arrives in a later milestone.

use std::sync::atomic::{AtomicBool, Ordering};

pub const SETTINGS_LABEL: &str = "main";
pub const MENU_PAUSE: &str = "pause";
pub const MENU_OPEN: &str = "open-settings";
pub const MENU_QUIT: &str = "quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    TogglePause,
    OpenSettings,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEffect {
    PauseStub { paused: bool },
    ShowSettings,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidentState {
    paused: bool,
}

impl ResidentState {
    pub fn new() -> Self {
        Self { paused: false }
    }

    pub fn on_tray(self, command: TrayCommand) -> (Self, TrayEffect) {
        match command {
            TrayCommand::TogglePause => {
                let paused = !self.paused;
                (Self { paused }, TrayEffect::PauseStub { paused })
            }
            TrayCommand::OpenSettings => (self, TrayEffect::ShowSettings),
            TrayCommand::Quit => (self, TrayEffect::Quit),
        }
    }

    pub fn command_from_id(id: &str) -> Option<TrayCommand> {
        match id {
            MENU_PAUSE => Some(TrayCommand::TogglePause),
            MENU_OPEN => Some(TrayCommand::OpenSettings),
            MENU_QUIT => Some(TrayCommand::Quit),
            _ => None,
        }
    }
}

impl Default for ResidentState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn pause_label(paused: bool) -> &'static str {
    if paused {
        "Resume"
    } else {
        "Pause"
    }
}

/// Closing settings hides the window. The process stays in the tray.
pub fn hides_on_close() -> bool {
    true
}

/// A second launch shows the existing settings window instead of a second process.
pub fn on_second_instance() -> TrayEffect {
    TrayEffect::ShowSettings
}

pub trait LoginAutostart {
    fn enable(&self) -> Result<(), String>;
    fn is_enabled(&self) -> Result<bool, String>;
}

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_quit() {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn quit_requested() -> bool {
    QUIT_REQUESTED.load(Ordering::SeqCst)
}

pub fn ensure_login_autostart(autostart: &impl LoginAutostart) -> Result<(), String> {
    if !autostart.is_enabled()? {
        autostart.enable()?;
    }
    Ok(())
}

pub trait TaskbarPresence {
    fn omit_from_taskbar(&mut self) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{
        ensure_login_autostart, hides_on_close, on_second_instance, pause_label, LoginAutostart,
        ResidentState, TrayCommand, TrayEffect,
    };

    #[test]
    fn closing_settings_hides_instead_of_quitting() {
        assert!(hides_on_close());
    }

    #[test]
    fn pause_is_a_stub_that_toggles_the_label() {
        let state = ResidentState::new();
        assert!(!state.paused);
        assert_eq!(pause_label(state.paused), "Pause");
        let (state, effect) = state.on_tray(TrayCommand::TogglePause);
        assert_eq!(effect, TrayEffect::PauseStub { paused: true });
        assert!(state.paused);
        assert_eq!(pause_label(true), "Resume");
        let (state, effect) = state.on_tray(TrayCommand::TogglePause);
        assert_eq!(effect, TrayEffect::PauseStub { paused: false });
        assert_eq!(pause_label(state.paused), "Pause");
    }

    #[test]
    fn open_settings_and_quit_do_not_toggle_pause() {
        let state = ResidentState::new();
        let (next, effect) = state.on_tray(TrayCommand::OpenSettings);
        assert_eq!(effect, TrayEffect::ShowSettings);
        assert!(!next.paused);
        let (_, effect) = state.on_tray(TrayCommand::Quit);
        assert_eq!(effect, TrayEffect::Quit);
    }

    #[test]
    fn menu_ids_map_to_the_milestone_commands() {
        assert_eq!(
            ResidentState::command_from_id("pause"),
            Some(TrayCommand::TogglePause)
        );
        assert_eq!(
            ResidentState::command_from_id("open-settings"),
            Some(TrayCommand::OpenSettings)
        );
        assert_eq!(
            ResidentState::command_from_id("quit"),
            Some(TrayCommand::Quit)
        );
        assert_eq!(ResidentState::command_from_id("next"), None);
    }

    #[test]
    fn second_instance_opens_settings() {
        assert_eq!(on_second_instance(), TrayEffect::ShowSettings);
    }

    #[test]
    fn autostart_is_enabled_once() {
        struct Fake {
            enabled: Cell<bool>,
            enables: Cell<u32>,
        }
        impl LoginAutostart for Fake {
            fn enable(&self) -> Result<(), String> {
                self.enables.set(self.enables.get() + 1);
                self.enabled.set(true);
                Ok(())
            }
            fn is_enabled(&self) -> Result<bool, String> {
                Ok(self.enabled.get())
            }
        }
        let fake = Fake {
            enabled: Cell::new(false),
            enables: Cell::new(0),
        };
        ensure_login_autostart(&fake).expect("enable");
        ensure_login_autostart(&fake).expect("already enabled");
        assert_eq!(fake.enables.get(), 1);
        assert!(fake.is_enabled().expect("status"));
    }
}
