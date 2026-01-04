//! AI Observations Module
//!
//! Provides the `AiObservations` resource that captures the current game state
//! for AI agents. This includes player state, orb status, wall distances, and
//! navigation targets.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use bevy_rapier3d::plugin::context::systemparams::RapierContext;
use std::f32::consts::PI;

use crate::game_state::{GameState, OrbParent};
use crate::player::{Player, PlayerCamera};

/// Component to identify orbs by a numeric ID (0-99).
/// This should be added to OrbParent entities during scene loading.
#[derive(Component, Clone, Copy, Debug)]
pub struct OrbId(pub u8);

/// Resource containing all observations needed for AI agents.
#[derive(Resource, Debug, Clone)]
pub struct AiObservations {
    /// Orb checklist: 1.0 = active, 0.0 = collected, indexed by OrbId (0-99)
    pub orb_checklist: [f32; 100],

    /// Player's world position
    pub player_position: Vec3,

    /// Camera yaw angle in radians (horizontal look direction)
    pub camera_yaw: f32,

    /// Camera pitch angle in radians (vertical look angle, 0 = horizontal)
    pub camera_pitch: f32,

    /// Player's velocity in their local reference frame
    pub player_velocity_local: Vec3,

    /// Player's velocity in world frame
    pub player_velocity_world: Vec3,

    /// Ratio of current speed of light to starting speed of light
    pub speed_of_light_ratio: f32,

    /// Combo timer (orb_speed_boost_timer from GameState)
    pub combo_timer: f32,

    /// Speed multiplier from GameState
    pub speed_multiplier: f32,

    /// Wall ray distances: normalized (0.0 = touching, 1.0 = far/no hit)
    /// 16 rays at 22.5 degree intervals around the player
    pub wall_rays: [f32; 16],

    /// Nearest orb targets: (direction_local, path_distance, orb_id)
    /// orb_id is -1.0 for empty slots, 0.0-99.0 for valid orbs
    /// Python side should use an embedding layer for the orb ID
    /// Populated by navmesh module
    pub orb_targets: [(Vec3, f32, f32); 10],

    /// Frame counter when observation was captured
    pub observation_tick: u64,
}

impl Default for AiObservations {
    fn default() -> Self {
        Self {
            orb_checklist: [1.0; 100],
            player_position: Vec3::ZERO,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            player_velocity_local: Vec3::ZERO,
            player_velocity_world: Vec3::ZERO,
            speed_of_light_ratio: 1.0,
            combo_timer: 0.0,
            speed_multiplier: 1.0,
            wall_rays: [1.0; 16],
            orb_targets: [(Vec3::ZERO, 0.0, -1.0); 10],
            observation_tick: 0,
        }
    }
}

/// Plugin that initializes and updates AI observations.
pub struct AiObservationPlugin;

impl Plugin for AiObservationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiObservations>()
            .init_resource::<ObservationTick>()
            .add_systems(PostUpdate, update_observations);
    }
}

// Note: orb_id is passed as a raw f32 (0-99, or -1 for empty)
// Python side should use nn.Embedding(101, embed_dim) with id+1 to handle -1 as padding

/// Internal resource to track observation frame count
#[derive(Resource, Default)]
pub struct ObservationTick(u64);

/// Maximum raycast distance for wall detection
const MAX_RAY_DISTANCE: f32 = 150.0;

/// System that updates all AI observations each frame.
pub fn update_observations(
    mut observations: ResMut<AiObservations>,
    mut tick: ResMut<ObservationTick>,
    game_state: Res<GameState>,
    rapier_context: ReadRapierContext,
    q_player: Query<(&Transform, &Velocity), With<Player>>,
    q_camera: Query<&Transform, (With<PlayerCamera>, Without<Player>)>,
    q_orbs: Query<(Entity, Option<&OrbId>, &Visibility), With<OrbParent>>,
) {
    // Increment tick
    tick.0 += 1;
    observations.observation_tick = tick.0;

    // Update player state
    if let Ok((transform, velocity)) = q_player.single() {
        // Player position
        observations.player_position = transform.translation;

        // Yaw is from player transform (horizontal rotation)
        let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        observations.camera_yaw = yaw;

        // Pitch is from camera transform (vertical look angle)
        if let Ok(camera_transform) = q_camera.single() {
            let (_, pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);
            observations.camera_pitch = pitch;
        }

        // World velocity
        observations.player_velocity_world = velocity.linvel;

        // Local velocity (transform world velocity to player's local frame)
        let inverse_rotation = transform.rotation.inverse();
        observations.player_velocity_local = inverse_rotation * velocity.linvel;

        // Perform wall raycasts if rapier context is available
        if let Ok(rapier_ctx) = rapier_context.single() {
            update_wall_rays(&mut observations, transform, &rapier_ctx);
        }
    }

    // Update game state values
    observations.speed_of_light_ratio = game_state.speed_of_light / game_state.start_speed_of_light;
    observations.combo_timer = game_state.orb_speed_boost_timer;
    observations.speed_multiplier = game_state.speed_multiplier;

    // Update orb checklist
    // Reset to 0.0 first, then set active orbs to 1.0
    observations.orb_checklist = [0.0; 100];

    for (_entity, orb_id, visibility) in q_orbs.iter() {
        if let Some(orb_id) = orb_id {
            let idx = orb_id.0 as usize;
            if idx < 100 {
                // 1.0 if visible (active), 0.0 if hidden (collected)
                observations.orb_checklist[idx] = match visibility {
                    Visibility::Hidden => 0.0,
                    Visibility::Visible | Visibility::Inherited => 1.0,
                };
            }
        }
    }

    // Note: orb_targets is populated by the navmesh module, left as zeros here
}

/// Performs 16 raycasts at 22.5 degree intervals to detect walls.
fn update_wall_rays(
    observations: &mut AiObservations,
    player_transform: &Transform,
    rapier_context: &RapierContext,
) {
    let origin = player_transform.translation;

    // Query filter to exclude sensors (only detect solid geometry)
    let filter = QueryFilter::default().exclude_sensors();

    for i in 0..16 {
        // Calculate ray direction at 22.5 degree intervals
        let angle = (i as f32) * (PI / 8.0); // 22.5 degrees = PI/8 radians

        // Ray direction in the horizontal plane (XZ), rotated by player's yaw
        let local_dir = Vec3::new(angle.cos(), 0.0, angle.sin());
        let world_dir = player_transform.rotation * local_dir;

        // Cast the ray
        if let Some((_entity, toi)) = rapier_context.cast_ray(
            origin,
            world_dir,
            MAX_RAY_DISTANCE,
            true,
            filter,
        ) {
            // Normalize distance: 0.0 = touching, 1.0 = at max distance or beyond
            observations.wall_rays[i] = (toi / MAX_RAY_DISTANCE).clamp(0.0, 1.0);
        } else {
            // No hit = far away
            observations.wall_rays[i] = 1.0;
        }
    }
}
