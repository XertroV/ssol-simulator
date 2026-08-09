use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct CurriculumConfig {
    /// If Some, only spawn orbs within this distance of player start.
    pub orb_spawn_radius: Option<f32>,
    /// If Some, limit the maximum number of orbs to spawn.
    pub max_orbs: Option<u32>,
    /// Cached position of player start (from Sphere/Playermesh object).
    pub player_spawn_position: Vec3,
    /// Number of orbs that were actually spawned (for reference).
    pub active_orb_count: u32,
    /// Random-spawn nearest curriculum: keep spawn orb + N nearest → N+1 total.
    /// When set, overrides max_orbs/radius selection at scene load.
    pub nearest_extra: Option<u32>,
    /// RNG seed for choosing which orb is the spawn center (with nearest_extra).
    pub nearest_spawn_seed: u64,
}

impl Default for CurriculumConfig {
    fn default() -> Self {
        Self {
            orb_spawn_radius: None,
            max_orbs: None,
            player_spawn_position: Vec3::ZERO,
            active_orb_count: 0,
            nearest_extra: None,
            nearest_spawn_seed: 0,
        }
    }
}

impl CurriculumConfig {
    /// Returns true if an orb should spawn based on the curriculum radius constraint.
    pub fn should_spawn_orb(&self, orb_position: Vec3) -> bool {
        match self.orb_spawn_radius {
            Some(radius) => self.player_spawn_position.distance(orb_position) <= radius,
            None => true,
        }
    }
}
