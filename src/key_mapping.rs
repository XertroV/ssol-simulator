use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub struct KeyMappingPlugin;

impl Plugin for KeyMappingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KeyMapping>();
    }
}


#[derive(Resource, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyMapping {
    pub forward: Option<KeyCode>,
    pub backward: Option<KeyCode>,
    pub left: Option<KeyCode>,
    pub right: Option<KeyCode>,
    pub jump: Option<KeyCode>, // todo (optional mode)
    // utils
    pub free_cam: Option<KeyCode>,
    pub fps_stats: Option<KeyCode>,
    pub vsync_toggle: Option<KeyCode>, // todo
    pub fullscreen_toggle: Option<KeyCode>, // todo
    // Free camera controls
    pub free_cam_up: Option<KeyCode>,
    pub free_cam_down: Option<KeyCode>,
    // Game state controls
    pub reset_game: Option<KeyCode>,
    pub pause_game: Option<KeyCode>,
    // debug
    pub toggle_white_arch: Option<KeyCode>,
    pub cheat_99_orbs: Option<KeyCode>,
    pub gizmo_toggle: Option<KeyCode>,
}

impl Default for KeyMapping {
    fn default() -> Self {
        Self {
            forward: Some(KeyCode::KeyW),
            backward: Some(KeyCode::KeyS),
            left: Some(KeyCode::KeyA),
            right: Some(KeyCode::KeyD),
            jump: None,
            free_cam: Some(KeyCode::KeyC),
            fps_stats: Some(KeyCode::F7),
            vsync_toggle: Some(KeyCode::F9),
            fullscreen_toggle: Some(KeyCode::F11),
            free_cam_up: Some(KeyCode::Space),
            free_cam_down: Some(KeyCode::ShiftLeft),
            reset_game: Some(KeyCode::Backspace),
            pause_game: Some(KeyCode::Pause),
            toggle_white_arch: Some(KeyCode::KeyH),
            cheat_99_orbs: Some(KeyCode::Backslash),
            gizmo_toggle: Some(KeyCode::KeyG),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    Forward,
    Backward,
    Left,
    Right,
    FreeCam,
    FpsStats,
    VsyncToggle,
    FullscreenToggle,
    FreeCamUp,
    FreeCamDown,
    ResetGame,
    PauseGame,
    ToggleWhiteArch,
    Cheat99Orbs,
    PhysicsGizmos,
}

impl KeyAction {
    pub const ALL: [Self; 15] = [
        Self::Forward,
        Self::Backward,
        Self::Left,
        Self::Right,
        Self::FreeCam,
        Self::FpsStats,
        Self::VsyncToggle,
        Self::FullscreenToggle,
        Self::FreeCamUp,
        Self::FreeCamDown,
        Self::ResetGame,
        Self::PauseGame,
        Self::ToggleWhiteArch,
        Self::Cheat99Orbs,
        Self::PhysicsGizmos,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Forward => "Move forward",
            Self::Backward => "Move backward",
            Self::Left => "Move left",
            Self::Right => "Move right",
            Self::FreeCam => "Toggle free camera",
            Self::FpsStats => "Toggle perf HUD",
            Self::VsyncToggle => "Toggle VSync",
            Self::FullscreenToggle => "Toggle fullscreen",
            Self::FreeCamUp => "Free camera up",
            Self::FreeCamDown => "Free camera down",
            Self::ResetGame => "Reset game",
            Self::PauseGame => "Pause game",
            Self::ToggleWhiteArch => "Toggle white arch",
            Self::Cheat99Orbs => "Cheat 99 orbs",
            Self::PhysicsGizmos => "Toggle physics gizmos",
        }
    }
}

impl KeyMapping {
    pub fn key_for(&self, action: KeyAction) -> Option<KeyCode> {
        match action {
            KeyAction::Forward => self.forward,
            KeyAction::Backward => self.backward,
            KeyAction::Left => self.left,
            KeyAction::Right => self.right,
            KeyAction::FreeCam => self.free_cam,
            KeyAction::FpsStats => self.fps_stats,
            KeyAction::VsyncToggle => self.vsync_toggle,
            KeyAction::FullscreenToggle => self.fullscreen_toggle,
            KeyAction::FreeCamUp => self.free_cam_up,
            KeyAction::FreeCamDown => self.free_cam_down,
            KeyAction::ResetGame => self.reset_game,
            KeyAction::PauseGame => self.pause_game,
            KeyAction::ToggleWhiteArch => self.toggle_white_arch,
            KeyAction::Cheat99Orbs => self.cheat_99_orbs,
            KeyAction::PhysicsGizmos => self.gizmo_toggle,
        }
    }

    pub fn set_key(&mut self, action: KeyAction, key: Option<KeyCode>) {
        match action {
            KeyAction::Forward => self.forward = key,
            KeyAction::Backward => self.backward = key,
            KeyAction::Left => self.left = key,
            KeyAction::Right => self.right = key,
            KeyAction::FreeCam => self.free_cam = key,
            KeyAction::FpsStats => self.fps_stats = key,
            KeyAction::VsyncToggle => self.vsync_toggle = key,
            KeyAction::FullscreenToggle => self.fullscreen_toggle = key,
            KeyAction::FreeCamUp => self.free_cam_up = key,
            KeyAction::FreeCamDown => self.free_cam_down = key,
            KeyAction::ResetGame => self.reset_game = key,
            KeyAction::PauseGame => self.pause_game = key,
            KeyAction::ToggleWhiteArch => self.toggle_white_arch = key,
            KeyAction::Cheat99Orbs => self.cheat_99_orbs = key,
            KeyAction::PhysicsGizmos => self.gizmo_toggle = key,
        }
    }

    pub fn reset_key(&mut self, action: KeyAction) {
        self.set_key(action, Self::default().key_for(action));
    }

    pub fn action_for_key(&self, key: KeyCode) -> Option<KeyAction> {
        KeyAction::ALL
            .into_iter()
            .find(|action| self.key_for(*action) == Some(key))
    }

    pub fn binding_label(&self, action: KeyAction) -> String {
        match self.key_for(action) {
            Some(key) => format!("{key:?}"),
            None => "Unbound".to_string(),
        }
    }

    pub fn pressed(&self, input: &ButtonInput<KeyCode>, action: KeyAction) -> bool {
        self.key_for(action)
            .map(|key| input.pressed(key))
            .unwrap_or(false)
    }

    pub fn just_pressed(&self, input: &ButtonInput<KeyCode>, action: KeyAction) -> bool {
        self.key_for(action)
            .map(|key| input.just_pressed(key))
            .unwrap_or(false)
    }
}
