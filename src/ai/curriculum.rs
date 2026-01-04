use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct CurriculumConfig {
    /// If Some, only spawn orbs within this distance of player start
    pub orb_spawn_radius: Option<f32>,
    /// If Some, limit the maximum number of orbs to spawn
    pub max_orbs: Option<u32>,
    /// Cached position of player start (from Sphere/Playermesh object)
    pub player_spawn_position: Vec3,
    /// Number of orbs that were actually spawned (for reference)
    pub active_orb_count: u32,
}

impl Default for CurriculumConfig {
    fn default() -> Self {
        Self {
            orb_spawn_radius: None,
            max_orbs: None,
            player_spawn_position: Vec3::ZERO,
            active_orb_count: 0,
        }
    }
}

impl CurriculumConfig {
    /// Returns true if orb should spawn based on curriculum radius constraint
    pub fn should_spawn_orb(&self, orb_position: Vec3) -> bool {
        match self.orb_spawn_radius {
            Some(radius) => {
                let distance = self.player_spawn_position.distance(orb_position);
                distance <= radius
            }
            None => true, // No radius constraint, spawn all orbs
        }
    }
}

pub struct CurriculumPlugin;

impl Plugin for CurriculumPlugin {
    fn build(&self, _app: &mut App) {
        // CurriculumConfig is initialized in main.rs (needed even without AI mode)
    }
}
