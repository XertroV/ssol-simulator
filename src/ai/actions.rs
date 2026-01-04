use bevy::prelude::*;

/// AI action input resource - provides look and movement commands
#[derive(Resource, Default, Clone, Debug)]
pub struct AiActionInput {
    /// Pitch (x) and yaw (y) delta in radians
    pub look: Vec2,
    /// Forward/back (x: -1 to 1) and left/right (y: -1 to 1)
    pub move_dir: Vec2,
}

/// AI configuration resource
#[derive(Resource, Clone, Debug)]
pub struct AiConfig {
    /// Whether AI control is active
    pub enabled: bool,
    /// How many physics ticks to hold each action
    pub action_repeat: u32,
    /// Countdown for current action
    pub ticks_remaining: u32,
    /// Whether to use lockstep synchronization (blocking wait for Python)
    pub lockstep: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            action_repeat: 4,
            ticks_remaining: 0,
            lockstep: false,
        }
    }
}

/// Plugin for AI action system
pub struct AiActionPlugin;

impl Plugin for AiActionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiActionInput>()
            .init_resource::<AiConfig>();
    }
}
