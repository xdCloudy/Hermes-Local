pub const TITLEBAR_HEIGHT_PX: u32 = 34;
pub const WINDOW_MIN_WIDTH: u32 = 400;
pub const WINDOW_MIN_HEIGHT: u32 = 620;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowPhase {
    Normal,
    Minimized,
    Maximized,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeWindowAction {
    Drag,
    Minimize,
    ToggleMaximized,
    Close,
    Relaunch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowLifecycle {
    phase: WindowPhase,
    relaunch_generation: u32,
}

impl Default for WindowLifecycle {
    fn default() -> Self {
        Self {
            phase: WindowPhase::Normal,
            relaunch_generation: 0,
        }
    }
}

impl WindowLifecycle {
    pub const fn phase(self) -> WindowPhase {
        self.phase
    }

    pub const fn relaunch_generation(self) -> u32 {
        self.relaunch_generation
    }

    pub fn apply(&mut self, action: NativeWindowAction) {
        match action {
            NativeWindowAction::Drag => {}
            NativeWindowAction::Minimize if self.phase != WindowPhase::Closed => {
                self.phase = WindowPhase::Minimized;
            }
            NativeWindowAction::ToggleMaximized if self.phase != WindowPhase::Closed => {
                self.phase = if self.phase == WindowPhase::Maximized {
                    WindowPhase::Normal
                } else {
                    WindowPhase::Maximized
                };
            }
            NativeWindowAction::Close => self.phase = WindowPhase::Closed,
            NativeWindowAction::Relaunch => {
                self.relaunch_generation = self.relaunch_generation.saturating_add(1);
                self.phase = WindowPhase::Normal;
            }
            NativeWindowAction::Minimize | NativeWindowAction::ToggleMaximized => {}
        }
    }
}

pub const fn titlebar_control_is_drag_region(control: NativeWindowAction) -> bool {
    matches!(control, NativeWindowAction::Drag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_matches_native_window_action_contract() {
        let mut state = WindowLifecycle::default();
        state.apply(NativeWindowAction::Minimize);
        assert_eq!(state.phase(), WindowPhase::Minimized);
        state.apply(NativeWindowAction::ToggleMaximized);
        assert_eq!(state.phase(), WindowPhase::Maximized);
        state.apply(NativeWindowAction::ToggleMaximized);
        assert_eq!(state.phase(), WindowPhase::Normal);
        state.apply(NativeWindowAction::Close);
        assert_eq!(state.phase(), WindowPhase::Closed);
        state.apply(NativeWindowAction::Minimize);
        assert_eq!(state.phase(), WindowPhase::Closed);
        state.apply(NativeWindowAction::Relaunch);
        assert_eq!(state.phase(), WindowPhase::Normal);
        assert_eq!(state.relaunch_generation(), 1);
    }

    #[test]
    fn titlebar_geometry_and_drag_exclusion_match_shell_contract() {
        assert_eq!(TITLEBAR_HEIGHT_PX, 34);
        assert_eq!(WINDOW_MIN_WIDTH, 400);
        assert_eq!(WINDOW_MIN_HEIGHT, 620);
        assert!(titlebar_control_is_drag_region(NativeWindowAction::Drag));
        for action in [
            NativeWindowAction::Minimize,
            NativeWindowAction::ToggleMaximized,
            NativeWindowAction::Close,
            NativeWindowAction::Relaunch,
        ] {
            assert!(!titlebar_control_is_drag_region(action));
        }
    }
}
