use bevy::prelude::*;


pub struct KeyMappingPlugin;

impl Plugin for KeyMappingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KeyMapping>();
    }
}


#[derive(Resource)]
pub struct KeyMapping {
    pub forward: KeyCode,
    pub backward: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub jump: KeyCode, // todo (optional mode)
    // utils
    pub free_cam: KeyCode,
    pub fps_stats: KeyCode,
    pub vsync_toggle: KeyCode, // todo
    pub fullscreen_toggle: KeyCode, // todo
    // Free camera controls
    pub free_cam_up: KeyCode,
    pub free_cam_down: KeyCode,
    // Game state controls
    pub reset_game: KeyCode,
    pub pause_game: KeyCode,
}

impl Default for KeyMapping {
    fn default() -> Self {
        Self {
            forward: KeyCode::KeyW,
            backward: KeyCode::KeyS,
            left: KeyCode::KeyA,
            right: KeyCode::KeyD,
            jump: KeyCode::Space,
            free_cam: KeyCode::KeyC,
            fps_stats: KeyCode::F7,
            vsync_toggle: KeyCode::F9,
            fullscreen_toggle: KeyCode::F11,
            free_cam_up: KeyCode::Space,
            free_cam_down: KeyCode::ShiftLeft,
            reset_game: KeyCode::Backspace,
            pause_game: KeyCode::Pause,
        }
    }
}
