//! AI Training Infrastructure for SSOL Simulator
//!
//! This module provides the infrastructure for training RL agents to play the game.
//! It includes:
//! - Observation extraction (player state, orb checklist, wall rays, NavMesh guidance)
//! - Action input (continuous look/move replacing keyboard/mouse)
//! - Reward calculation
//! - Episode control (reset, termination)
//! - Curriculum learning support
//! - Testing mode for validation

use bevy::prelude::*;

pub mod actions;
pub mod curriculum;
pub mod navmesh;
pub mod observations;
pub mod rewards;
pub mod testing;

pub use actions::{AiActionInput, AiConfig};
pub use curriculum::CurriculumConfig;
pub use navmesh::NavMeshState;
pub use observations::{AiObservations, OrbId};
pub use rewards::AiRewardSignal;
pub use testing::AiTestingPlugin;

use crate::player::PlayerRespawnRequest;

/// Main AI plugin that bundles all AI-related functionality
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiEpisodeControl>()
            .add_plugins(actions::AiActionPlugin)
            .add_plugins(observations::AiObservationPlugin)
            .add_plugins(rewards::AiRewardPlugin)
            .add_plugins(curriculum::CurriculumPlugin)
            .add_plugins(navmesh::AiNavMeshPlugin)
            .add_systems(Startup, configure_ai_from_simconfig)
            .add_systems(FixedUpdate, handle_episode_reset.before(crate::player::player_update_start))
            .add_systems(FixedUpdate, increment_episode_tick.after(crate::player::player_update_done));

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
}

/// System to handle episode reset requests
fn handle_episode_reset(
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

        // Reset reward signal for new episode
        ai_rewards.step_reward = 0.0;
        ai_rewards.terminated = false;
        ai_rewards.truncated = false;

        info!("AI Episode {} started", episode_control.episode_count);
    }
}

/// System to increment episode tick counter
fn increment_episode_tick(
    mut episode_control: ResMut<AiEpisodeControl>,
    mut ai_rewards: ResMut<AiRewardSignal>,
    config: Res<AiConfig>,
) {
    if !config.enabled {
        return;
    }

    episode_control.episode_ticks += 1;
    episode_control.observation_ready = true;

    // Check for episode truncation (timeout)
    if let Some(max_ticks) = episode_control.max_episode_ticks {
        if episode_control.episode_ticks >= max_ticks {
            ai_rewards.truncated = true;
        }
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
}

impl AiEpisodeControl {
    pub fn request_reset(&mut self) {
        self.reset_requested = true;
    }
}
