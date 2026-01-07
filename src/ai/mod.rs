//! AI Training Infrastructure for SSOL Simulator
//!
//! This module provides the infrastructure for training RL agents to play the game.
//! It includes:
//! - Observation extraction (player state, orb checklist, wall rays, NavMesh guidance)
//! - Action input (continuous look/move replacing keyboard/mouse)
//! - Reward calculation
//! - Episode control (reset, termination)
//! - Curriculum learning support
//! - ZMQ bridge for Python communication
//! - Testing mode for validation

use bevy::prelude::*;

pub mod actions;
pub mod bridge;
pub mod curriculum;
pub mod gizmos;
pub mod navmesh;
pub mod observations;
pub mod rewards;
pub mod testing;

pub use actions::{AiActionInput, AiConfig};
pub use bridge::BridgePlugin;
pub use curriculum::CurriculumConfig;
pub use gizmos::AiGizmosPlugin;
pub use navmesh::NavMeshState;
pub use observations::{AiObservations, OrbId};
pub use rewards::AiRewardSignal;
pub use testing::AiTestingPlugin;

use crate::player::PlayerRespawnRequest;

/// Run condition: returns false when AI is in lockstep mode and waiting for action.
/// Use this to skip physics systems while waiting for the AI to send a command.
/// Returns true if AiConfig doesn't exist (non-AI mode).
pub fn not_waiting_for_ai(ai_config: Option<Res<AiConfig>>) -> bool {
    match ai_config {
        Some(config) => !config.waiting_for_action,
        None => true, // No AI config = not in AI mode, always run
    }
}

/// Main AI plugin that bundles all AI-related functionality
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiEpisodeControl>()
            .add_plugins(actions::AiActionPlugin)
            .add_plugins(rewards::AiRewardPlugin)
            .add_plugins(curriculum::CurriculumPlugin)
            .add_plugins(navmesh::AiNavMeshPlugin)
            .add_plugins(bridge::BridgePlugin)
            .add_systems(Startup, configure_ai_from_simconfig)
            .add_systems(
                FixedUpdate,
                handle_episode_reset
                    .before(crate::player::player_update_start)
                    .run_if(not_waiting_for_ai),
            )
            .add_systems(
                FixedUpdate,
                increment_episode_tick
                    .after(crate::player::player_update_done)
                    .run_if(not_waiting_for_ai),
            )
            // Update observations after physics/episode_tick but before bridge step completion
            .add_systems(
                FixedUpdate,
                observations::update_observations
                    .after(increment_episode_tick)
                    .before(bridge::complete_pending_step)
                    .run_if(not_waiting_for_ai),
            );

        // Note: Testing plugin is added conditionally from main.rs based on SimConfig.ai_test
    }
}

/// Configure AiConfig based on SimConfig CLI flags
fn configure_ai_from_simconfig(
    sim_config: Res<crate::SimConfig>,
    mut ai_config: ResMut<AiConfig>,
) {
    // Enable AI control when either --ai-mode or --ai-test is passed
    if sim_config.ai_mode || sim_config.ai_test {
        ai_config.enabled = true;
        info!("AI control enabled (ai_mode={}, ai_test={})", sim_config.ai_mode, sim_config.ai_test);
    }

    // Enable lockstep synchronization when using ZMQ bridge (ai_mode, not ai_test)
    if sim_config.ai_mode && sim_config.zmq_port.is_some() {
        ai_config.lockstep = true;
        info!("Lockstep synchronization enabled for ZMQ bridge");
    }
}

/// System to handle episode reset requests
pub fn handle_episode_reset(
    mut commands: Commands,
    mut episode_control: ResMut<AiEpisodeControl>,
    mut ai_rewards: ResMut<AiRewardSignal>,
    config: Res<AiConfig>,
) {
    if !config.enabled {
        return;
    }

    if episode_control.reset_requested {
        // Trigger the player respawn which resets everything
        commands.trigger(PlayerRespawnRequest);

        // Update episode counters
        episode_control.reset_requested = false;
        episode_control.observation_ready = true;
        episode_control.episode_count += 1;
        episode_control.episode_ticks = 0;
        episode_control.ticks_since_last_orb = 0;

        // Reset all for new episode
        ai_rewards.reset_episode();

        info!("AI Episode {} started", episode_control.episode_count);
    }
}

/// System to increment episode tick counter
pub fn increment_episode_tick(
    mut episode_control: ResMut<AiEpisodeControl>,
    mut ai_rewards: ResMut<AiRewardSignal>,
    config: Res<AiConfig>,
) {
    if !config.enabled {
        return;
    }

    // Always increment global tick counter (used for lockstep startup delay)
    episode_control.global_ticks += 1;

    episode_control.episode_ticks += 1;
    episode_control.ticks_since_last_orb += 1;
    episode_control.observation_ready = true;

    // Check for episode truncation (timeout)
    if let Some(max_ticks) = episode_control.max_episode_ticks {
        if episode_control.episode_ticks >= max_ticks {
            ai_rewards.truncated = true;
        }
    }

    // Check for stale progress truncation (no orb collected in too long)
    if episode_control.ticks_since_last_orb >= STALE_PROGRESS_TIMEOUT_TICKS {
        ai_rewards.truncated = true;
    }
}

/// Resource to control AI episodes (reset, curriculum changes)
#[derive(Resource, Default)]
pub struct AiEpisodeControl {
    /// Set to true to trigger a reset at the start of the next frame
    pub reset_requested: bool,
    /// True after reset, before first action - indicates initial observation is ready
    pub observation_ready: bool,
    /// Current episode number
    pub episode_count: u32,
    /// Ticks elapsed in current episode
    pub episode_ticks: u32,
    /// Maximum ticks before truncation (None = no limit)
    pub max_episode_ticks: Option<u32>,
    /// Global tick counter since startup (used for lockstep delay)
    pub global_ticks: u32,
    /// Ticks since last orb was collected (for stale progress detection)
    pub ticks_since_last_orb: u32,
}

/// Number of ticks to wait after startup before enabling lockstep
/// This allows the scene to fully load and stabilize
pub const LOCKSTEP_STARTUP_DELAY_TICKS: u32 = 100;

/// Maximum ticks without collecting an orb before truncation (stale progress)
/// 1000 ticks = 10 seconds of simulated time at 100Hz
pub const STALE_PROGRESS_TIMEOUT_TICKS: u32 = 1000;

impl AiEpisodeControl {
    pub fn request_reset(&mut self) {
        self.reset_requested = true;
    }
}
