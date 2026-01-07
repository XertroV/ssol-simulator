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
    /// Previous closest orb ID (to detect target changes)
    pub prev_closest_orb_id: f32,
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
        self.prev_closest_orb_id = -1.0; // Invalid ID to trigger first-frame skip
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

    // Save orbs collected before resetting (for progress detection)
    let just_collected_orb = reward_signal.orbs_collected_this_step > 0;

    // Orb collection reward: +12.0 per orb collected
    let orb_reward = reward_signal.orbs_collected_this_step as f32 * 12.0;
    reward_signal.step_reward += orb_reward;
    reward_signal.orb_reward += orb_reward;

    // Reset orbs collected counter after applying reward
    reward_signal.orbs_collected_this_step = 0;

    // Momentum bonus: reward velocity toward nearest orb
    // Uses dot product of velocity (local) and direction to orb (local)
    let (orb_direction, current_distance, orb_id) = observations.orb_targets[0];
    let momentum_coef = 0.002;
    let momentum_bonus = if orb_id >= 0.0 && orb_direction.length_squared() > 0.01 {
        // Dot product: positive when moving toward orb, negative when moving away
        let velocity_toward_orb = observations.player_velocity_local.dot(orb_direction.normalize());
        // Normalize by max speed to keep reward scale consistent
        let base_max_speed = game_state.max_player_speed;
        if base_max_speed > 0.0 {
            let normalized_velocity = (velocity_toward_orb / base_max_speed).clamp(-1.0, 1.0);
            momentum_coef * normalized_velocity * game_state.speed_multiplier
        } else {
            0.0
        }
    } else {
        0.0
    };
    reward_signal.step_reward += momentum_bonus;
    reward_signal.momentum_bonus += momentum_bonus;

    // Approach orb reward: reward for getting closer to the nearest uncollected orb
    // Skip when target orb changes (orb collected or new closest) to avoid reward spikes
    let approach_r_coef = 0.05;
    let target_orb_changed = orb_id != reward_signal.prev_closest_orb_id;
    let mut is_making_progress = just_collected_orb;

    if orb_id >= 0.0 && current_distance > 0.0 && !target_orb_changed {
        let prev_distance = reward_signal.prev_closest_orb_distance;
        if prev_distance > 0.0 {
            // Calculate distance change (positive = moved away, negative = got closer)
            let distance_delta = current_distance - prev_distance;
            // Reward coefficient: approach_r_coef per unit closer, penalty for moving away
            // No clamp - let the full signal through
            let approach_reward = -distance_delta * approach_r_coef;
            reward_signal.step_reward += approach_reward;
            reward_signal.approach_reward += approach_reward;

            // Track if we're making progress (getting closer)
            is_making_progress = is_making_progress || approach_reward > 0.01;

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
    // Always update tracked orb ID (even when skipping reward calculation)
    reward_signal.prev_closest_orb_id = orb_id;

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
