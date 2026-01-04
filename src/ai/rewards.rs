use bevy::prelude::*;

use crate::game_state::{GameState, OrbPickedUp};

use super::{AiConfig, AiEpisodeControl, AiObservations};

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

    // Reward component breakdown for debugging UI
    /// Time penalty component (negative)
    pub time_penalty: f32,
    /// Orb collection reward component
    pub orb_reward: f32,
    /// Momentum bonus component
    pub momentum_bonus: f32,
    /// Camera pitch penalty component (negative)
    pub pitch_penalty: f32,
    /// Win completion reward (given when all orbs collected)
    pub win_reward: f32,
    /// Time bonus for fast completion
    pub time_bonus: f32,
}

impl AiRewardSignal {
    /// Reset rewards for a new step
    pub fn reset_step(&mut self) {
        self.step_reward = 0.0;
        self.orbs_collected_this_step = 0;
        self.terminated = false;
        self.truncated = false;
        // Reset component breakdown
        self.time_penalty = 0.0;
        self.orb_reward = 0.0;
        self.momentum_bonus = 0.0;
        self.pitch_penalty = 0.0;
        self.win_reward = 0.0;
        self.time_bonus = 0.0;
    }

    /// Full reset for new episode start
    pub fn reset_episode(&mut self) {
        self.reset_step();
    }
}

pub struct AiRewardPlugin;

impl Plugin for AiRewardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiRewardSignal>()
            .add_observer(on_orb_picked_up)
            .add_systems(PostUpdate, calculate_rewards.run_if(super::not_waiting_for_ai));
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
    episode_control: Res<AiEpisodeControl>,
) {
    // Always apply per-tick rewards

    // Time penalty: -0.005 per tick
    let time_penalty = 0.005;
    reward_signal.step_reward -= time_penalty;
    reward_signal.time_penalty -= time_penalty;

    // Orb collection reward: +10.0 per orb collected
    let orb_reward = reward_signal.orbs_collected_this_step as f32 * 10.0;
    reward_signal.step_reward += orb_reward;
    reward_signal.orb_reward += orb_reward;

    // Momentum bonus: +0.05 * (speed / max_speed)
    // This rewards maintaining high speed
    // TODO: Replace with dot product of velocity and direction to nearest orb
    let max_speed = game_state.max_player_speed;
    let momentum_bonus = if max_speed > 0.0 {
        let speed_ratio = (game_state.player_speed / max_speed).min(1.0);
        0.05 * speed_ratio
    } else {
        0.0
    };
    reward_signal.step_reward += momentum_bonus;
    reward_signal.momentum_bonus += momentum_bonus;

    // Camera pitch penalty: penalize looking too far up or down
    // camera_pitch is the pitch angle in radians
    // Neutral is around 0, penalty increases as abs(pitch) increases
    // Max comfortable pitch is around ±30 degrees (±0.52 rad), start penalizing beyond that
    let pitch = observations.camera_pitch;
    let pitch_threshold = 0.35; // ~20 degrees - start penalizing
    let pitch_max = 1.2; // ~70 degrees - maximum penalty
    let pitch_abs = pitch.abs();
    let pitch_penalty = if pitch_abs > pitch_threshold {
        // Quadratic penalty that increases from 0 at threshold to max at pitch_max
        let excess = ((pitch_abs - pitch_threshold) / (pitch_max - pitch_threshold)).clamp(0.0, 1.0);
        0.02 * excess * excess // Max penalty of 0.02 per tick
    } else {
        0.0
    };
    reward_signal.step_reward -= pitch_penalty;
    reward_signal.pitch_penalty -= pitch_penalty;

    // Reset orbs collected counter after applying reward
    reward_signal.orbs_collected_this_step = 0;

    // Check for win condition and give completion rewards
    if game_state.game_win && !reward_signal.terminated {
        // Win reward: large bonus for completing the level
        let win_reward = 100.0;
        reward_signal.step_reward += win_reward;
        reward_signal.win_reward += win_reward;

        // Time bonus: reward for fast completion
        // Target: ~1 second per orb (100 ticks at 100Hz)
        // WR for 100 orbs is 99 seconds, so sub-1s per orb is excellent
        let ticks_per_orb = episode_control.episode_ticks as f32 / game_state.nb_orbs.max(1) as f32;
        let target_ticks_per_orb = 100.0; // 1 second at 100Hz

        // Time bonus scales from 0 (slow) to 50 (at target) to 100 (2x faster than target)
        let time_ratio = (target_ticks_per_orb / ticks_per_orb.max(1.0)).clamp(0.0, 2.0);
        let time_bonus = 50.0 * time_ratio;
        reward_signal.step_reward += time_bonus;
        reward_signal.time_bonus += time_bonus;

        info!("Win! Episode ticks: {}, Time bonus: {:.2}", episode_control.episode_ticks, time_bonus);
    }

    // Only finalize termination/truncation when step is complete
    if ai_config.ticks_remaining == 0 {
        // Check for termination (game win - all orbs collected)
        reward_signal.terminated = game_state.game_win;

        // TODO: Check for truncation (episode exceeds max ticks)
        // For now, truncation is handled externally
    }
}
