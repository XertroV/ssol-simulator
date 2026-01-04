use bevy::prelude::*;

use crate::game_state::{GameState, OrbPickedUp};

use super::{AiConfig, AiObservations};

/// Resource tracking reward signals for the AI agent
#[derive(Resource, Default)]
pub struct AiRewardSignal {
    /// Accumulated reward for current step
    pub step_reward: f32,
    /// True when all orbs collected (game_win)
    pub terminated: bool,
    /// True when episode exceeds max ticks
    pub truncated: bool,
    /// Count of orbs collected since last step
    pub orbs_collected_this_step: u32,
}

impl AiRewardSignal {
    /// Reset rewards for a new step
    pub fn reset_step(&mut self) {
        self.step_reward = 0.0;
        self.orbs_collected_this_step = 0;
        self.terminated = false;
        self.truncated = false;
    }
}

pub struct AiRewardPlugin;

impl Plugin for AiRewardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiRewardSignal>()
            .add_observer(on_orb_picked_up)
            .add_systems(PostUpdate, calculate_rewards);
    }
}

/// Observer for OrbPickedUp events to track collection
fn on_orb_picked_up(_trigger: On<OrbPickedUp>, mut reward_signal: ResMut<AiRewardSignal>) {
    reward_signal.orbs_collected_this_step += 1;
}

/// System that calculates rewards based on game state and observations
fn calculate_rewards(
    mut reward_signal: ResMut<AiRewardSignal>,
    ai_config: Res<AiConfig>,
    observations: Res<AiObservations>,
    game_state: Res<GameState>,
) {
    // Always apply per-tick rewards

    // Time penalty: -0.005 per tick
    reward_signal.step_reward -= 0.005;

    // Orb collection reward: +10.0 per orb collected
    let orb_reward = reward_signal.orbs_collected_this_step as f32 * 10.0;
    reward_signal.step_reward += orb_reward;

    // Momentum bonus: +0.05 * (speed / max_speed)
    // This rewards maintaining high speed
    // TODO: Replace with dot product of velocity and direction to nearest orb
    let max_speed = game_state.max_player_speed;
    if max_speed > 0.0 {
        let speed_ratio = (game_state.player_speed / max_speed).min(1.0);
        let momentum_bonus = 0.05 * speed_ratio;
        reward_signal.step_reward += momentum_bonus;
    }

    // Camera pitch penalty: penalize looking too far up or down
    // camera_pitch is the pitch angle in radians
    // Neutral is around 0, penalty increases as abs(pitch) increases
    // Max comfortable pitch is around ±30 degrees (±0.52 rad), start penalizing beyond that
    let pitch = observations.camera_pitch;
    let pitch_threshold = 0.35; // ~20 degrees - start penalizing
    let pitch_max = 1.2; // ~70 degrees - maximum penalty
    let pitch_abs = pitch.abs();
    if pitch_abs > pitch_threshold {
        // Quadratic penalty that increases from 0 at threshold to max at pitch_max
        let excess = ((pitch_abs - pitch_threshold) / (pitch_max - pitch_threshold)).clamp(0.0, 1.0);
        let pitch_penalty = 0.02 * excess * excess; // Max penalty of 0.02 per tick
        reward_signal.step_reward -= pitch_penalty;
    }

    // Reset orbs collected counter after applying reward
    reward_signal.orbs_collected_this_step = 0;

    // Only finalize termination/truncation when step is complete
    if ai_config.ticks_remaining == 0 {
        // Check for termination (game win - all orbs collected)
        reward_signal.terminated = game_state.game_win;

        // TODO: Check for truncation (episode exceeds max ticks)
        // For now, truncation is handled externally
    }
}
