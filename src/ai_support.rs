use bevy::prelude::*;

#[cfg(feature = "ai")]
pub use crate::ai::{AiActionInput, AiConfig};

#[cfg(not(feature = "ai"))]
#[derive(Resource, Default, Clone, Debug)]
pub struct AiActionInput {
    /// Pitch (x) and yaw (y) delta in radians.
    pub look: Vec2,
    /// Forward/back and left/right movement inputs.
    pub move_dir: Vec2,
}

#[cfg(not(feature = "ai"))]
#[derive(Resource, Clone, Debug, Default)]
pub struct AiConfig {
    /// Whether AI control is active.
    pub enabled: bool,
    /// Whether fixed-step systems should wait for an AI action.
    #[allow(dead_code)]
    pub waiting_for_action: bool,
}

/// Shared UI/bridge telemetry for received AI actions.
#[derive(Resource, Default)]
pub struct ActionCounter {
    pub actions_this_second: u32,
    pub actions_per_second: u32,
}

#[cfg(feature = "ai")]
pub fn not_waiting_for_ai(ai_config: Option<Res<AiConfig>>) -> bool {
    crate::ai::not_waiting_for_ai(ai_config)
}

#[cfg(not(feature = "ai"))]
pub fn not_waiting_for_ai(_: Option<Res<AiConfig>>) -> bool {
    true
}
