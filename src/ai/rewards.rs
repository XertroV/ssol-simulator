use bevy::prelude::*;

use crate::game_state::{GameState, OrbPickedUp};

use super::{AiConfig, AiEpisodeControl, AiObservations};

// Action smoothness penalty thresholds
/// Threshold for penalizing individual large yaw changes (radians)
const INSTANT_YAW_THRESHOLD: f32 = 0.05;
/// Threshold for penalizing sustained jerkiness via EMA (radians)
pub const EMA_YAW_THRESHOLD: f32 = 0.03;
/// Penalty coefficient for instant yaw changes
const INSTANT_YAW_PENALTY_COEFF: f32 = 0.01;
/// Penalty coefficient for EMA-based sustained jerkiness
const EMA_YAW_PENALTY_COEFF: f32 = 1.0;

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
    /// Approaching orb reward component (positive when getting closer)
    pub approach_reward: f32,

    /// Previous distance to closest orb (for approach reward calculation)
    pub prev_closest_orb_distance: f32,
    /// Action smoothness penalty component (negative for jerky camera)
    pub action_smoothness_penalty: f32,
    /// Previous yaw action (for smoothness calculation)
    pub prev_yaw_action: f32,
    /// Exponential moving average of absolute yaw changes (tracks overall jerkiness)
    pub yaw_ema: f32,
}

impl AiRewardSignal {
    /// Reset reward values for a new tick (preserves termination flags)
    pub fn reset_step(&mut self) {
        self.step_reward = 0.0;
        self.orbs_collected_this_step = 0;
        // Note: Do NOT reset terminated/truncated here - they persist until episode reset
        // Reset component breakdown
        self.time_penalty = 0.0;
        self.orb_reward = 0.0;
        self.momentum_bonus = 0.0;
        self.pitch_penalty = 0.0;
        self.win_reward = 0.0;
        self.time_bonus = 0.0;
        self.approach_reward = 0.0;
        self.action_smoothness_penalty = 0.0;
        // Note: Do NOT reset prev_closest_orb_distance - it tracks across ticks
    }

    /// Full reset for new episode start
    pub fn reset_episode(&mut self) {
        self.reset_step();
        self.terminated = false;
        self.truncated = false;
        self.prev_closest_orb_distance = 0.0; // Will be set on first observation
        self.prev_yaw_action = 0.0; // Reset camera action tracking
        self.yaw_ema = 0.0; // Reset movement EMA
    }
}

pub struct AiRewardPlugin;

impl Plugin for AiRewardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiRewardSignal>()
            .add_observer(on_orb_picked_up)
            // Run in FixedPostUpdate to match physics tick rate (100Hz)
            // This ensures consistent reward calculation regardless of frame rate
            .add_systems(FixedPostUpdate, calculate_rewards.run_if(super::not_waiting_for_ai));
    }
}

/// Observer for OrbPickedUp events to track collection
fn on_orb_picked_up(
    _trigger: On<OrbPickedUp>,
    mut reward_signal: ResMut<AiRewardSignal>,
    mut episode_control: ResMut<super::AiEpisodeControl>,
) {
    reward_signal.orbs_collected_this_step += 1;
    episode_control.ticks_since_last_orb = 0;
}

/// System that calculates rewards based on game state and observations
fn calculate_rewards(
    mut reward_signal: ResMut<AiRewardSignal>,
    ai_config: Res<AiConfig>,
    ai_action: Res<super::AiActionInput>,
    observations: Res<AiObservations>,
    game_state: Res<GameState>,
    episode_control: Res<AiEpisodeControl>,
) {
    // Always apply per-tick rewards

    // Time penalty: -0.005 per tick
    let time_penalty = 0.005;
    reward_signal.step_reward -= time_penalty;
    reward_signal.time_penalty -= time_penalty;

    // Orb collection reward: +12.0 per orb collected
    let orb_reward = reward_signal.orbs_collected_this_step as f32 * 12.0;
    reward_signal.step_reward += orb_reward;
    reward_signal.orb_reward += orb_reward;

    let momentum_coef = 0.001;
    // Momentum bonus: +momentum_coef * (speed / base_max_speed)
    // Dividing by base max_player_speed (not multiplied by speed_mult) means:
    // - Rewards going fast relative to base speed
    // - Higher speed_multiplier allows higher speeds = higher rewards
    // TODO: Replace with dot product of velocity and direction to nearest orb
    let base_max_speed = game_state.max_player_speed;
    let momentum_bonus = if base_max_speed > 0.0 {
        let speed_ratio = (game_state.player_speed / base_max_speed).min(1.0);
        momentum_coef * speed_ratio * game_state.speed_multiplier
    } else {
        0.0
    };
    reward_signal.step_reward += momentum_bonus;
    reward_signal.momentum_bonus += momentum_bonus;

    /*
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
        -0.005 // small bonus for being close to horizontal
    };
    reward_signal.step_reward -= pitch_penalty;
    reward_signal.pitch_penalty -= pitch_penalty;
    */

    // Reset orbs collected counter after applying reward
    reward_signal.orbs_collected_this_step = 0;

    // Approach orb reward: reward for getting closer to the nearest uncollected orb
    // orb_targets[0] contains (direction, distance, orb_id) for closest orb
    let approach_r_coef = 0.05;
    let (_, current_distance, orb_id) = observations.orb_targets[0];
    let is_making_progress = reward_signal.orbs_collected_this_step > 0; // Just collected an orb
    if orb_id >= 0.0 && current_distance > 0.0 {
        let prev_distance = reward_signal.prev_closest_orb_distance;
        if prev_distance > 0.0 {
            // Calculate distance change (positive = moved away, negative = got closer)
            let distance_delta = current_distance - prev_distance;
            // Reward coefficient: approach_r_coef per unit closer, penalty for moving away
            // Clamp to avoid huge rewards when orbs are collected (distance jumps)
            let approach_reward = (-distance_delta * approach_r_coef).clamp(-0.05, 0.05);
            reward_signal.step_reward += approach_reward;
            reward_signal.approach_reward += approach_reward;

            // Track if we're making progress (getting closer)
            let is_making_progress = is_making_progress || approach_reward > 0.01;

            // Action smoothness penalties: penalize jerky camera when NOT making progress
            // Only apply if not approaching orb and not just collected
            let current_yaw = ai_action.look.y;
            let yaw_change = (current_yaw - reward_signal.prev_yaw_action).abs();

            // Update EMA of yaw changes (alpha=0.1 for ~10-step window)
            // EMA tracks sustained jerkiness over time
            let alpha = 0.1;
            reward_signal.yaw_ema = alpha * yaw_change + (1.0 - alpha) * reward_signal.yaw_ema;

            if !is_making_progress {
                // Penalty 1: Penalize individual large yaw changes
                if yaw_change > INSTANT_YAW_THRESHOLD {
                    let instant_penalty = INSTANT_YAW_PENALTY_COEFF * (yaw_change - INSTANT_YAW_THRESHOLD);
                    reward_signal.step_reward -= instant_penalty;
                    reward_signal.action_smoothness_penalty -= instant_penalty;
                }

                // Penalty 2: Penalize high sustained jerkiness (EMA)
                // This catches agents that constantly twitch even if each change is small
                if reward_signal.yaw_ema > EMA_YAW_THRESHOLD {
                    let ema_penalty = EMA_YAW_PENALTY_COEFF * (reward_signal.yaw_ema - EMA_YAW_THRESHOLD);
                    reward_signal.step_reward -= ema_penalty;
                    reward_signal.action_smoothness_penalty -= ema_penalty;
                }
            }

            // Update previous yaw for next step
            reward_signal.prev_yaw_action = current_yaw;
        }
        reward_signal.prev_closest_orb_distance = current_distance;
    }

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
