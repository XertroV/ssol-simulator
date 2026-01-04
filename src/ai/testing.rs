use bevy::prelude::*;
use rand::Rng;

use crate::game_state::GameState;

use super::{AiActionInput, AiConfig, AiEpisodeControl, AiRewardSignal};

#[derive(Resource, Default)]
pub struct AiTestState {
    pub ticks_since_last_action: u32,
    pub total_reward: f32,
    pub episodes_completed: u32,
    pub steps_this_episode: u32,
}

/// Plugin for AI testing mode
pub struct AiTestingPlugin;

/// Default timeout for AI test mode: 15 seconds at 100Hz = 1500 ticks
const AI_TEST_TIMEOUT_TICKS: u32 = 1500;

impl Plugin for AiTestingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiTestState>()
            .add_systems(Startup, setup_test_timeout)
            .add_systems(
                FixedUpdate,
                (
                    apply_random_actions,
                    log_observations,
                    handle_test_reset,
                )
                    .chain()
                    .after(crate::player::player_update_done),
            );
        info!("AiTestingPlugin registered");
    }
}

/// Set up the episode timeout for AI test mode
fn setup_test_timeout(mut episode_control: ResMut<AiEpisodeControl>) {
    episode_control.max_episode_ticks = Some(AI_TEST_TIMEOUT_TICKS);
    info!("AI Test mode: episode timeout set to {} ticks ({}s)", AI_TEST_TIMEOUT_TICKS, AI_TEST_TIMEOUT_TICKS as f32 / 100.0);
}

pub fn apply_random_actions(
    mut action_input: ResMut<AiActionInput>,
    mut test_state: ResMut<AiTestState>,
    config: Res<AiConfig>,
) {
    test_state.ticks_since_last_action += 1;

    if test_state.ticks_since_last_action >= config.action_repeat {
        test_state.ticks_since_last_action = 0;
        test_state.steps_this_episode += 1;

        let mut rng = rand::thread_rng();

        // Generate random look action in [-0.1, 0.1] radians
        action_input.look = Vec2::new(
            rng.gen_range(-0.1..=0.1),
            rng.gen_range(-0.1..=0.1),
        );

        // Generate random move direction in [-1.0, 1.0]
        action_input.move_dir = Vec2::new(
            rng.gen_range(-1.0..=1.0),
            rng.gen_range(-1.0..=1.0),
        );
    }
}

pub fn log_observations(
    reward_signal: Res<AiRewardSignal>,
    mut test_state: ResMut<AiTestState>,
    config: Res<AiConfig>,
) {
    // Check if step is complete (ticks_remaining == 0)
    if config.ticks_remaining == 0 {
        test_state.total_reward += reward_signal.step_reward;

        // Log every 100 steps
        if test_state.steps_this_episode % 100 == 0 {
            info!(
                "Step {} - reward: {:.4}, total: {:.4}",
                test_state.steps_this_episode,
                reward_signal.step_reward,
                test_state.total_reward
            );
        }

        // On termination or truncation, log episode summary
        if reward_signal.terminated || reward_signal.truncated {
            info!(
                "Episode {} complete - total_reward: {:.4}, steps: {}, reason: {}",
                test_state.episodes_completed,
                test_state.total_reward,
                test_state.steps_this_episode,
                if reward_signal.terminated { "won" } else { "truncated" }
            );
        }
    }
}

pub fn handle_test_reset(
    reward_signal: Res<AiRewardSignal>,
    mut episode_control: ResMut<AiEpisodeControl>,
    mut test_state: ResMut<AiTestState>,
    config: Res<AiConfig>,
    game_state: Res<GameState>,
    mut exit_writer: EventWriter<AppExit>,
) {
    // Only handle reset when step is complete
    if config.ticks_remaining != 0 {
        return;
    }

    if reward_signal.terminated || reward_signal.truncated {
        // In test mode: if truncated and we collected at least 1 orb, exit the game
        // If we collected 0 orbs, retry (might have spawned in a bad spot)
        if reward_signal.truncated {
            if game_state.score > 0 {
                info!(
                    "Test mode complete - collected {} orbs in {} steps. Exiting.",
                    game_state.score, test_state.steps_this_episode
                );
                exit_writer.write(AppExit::Success);
                return;
            } else {
                info!("Test mode truncated with 0 orbs collected. Retrying...");
            }
        }

        // If terminated (won the game), also exit
        if reward_signal.terminated {
            info!(
                "Test mode: Game won! Collected all {} orbs in {} steps. Exiting.",
                game_state.score, test_state.steps_this_episode
            );
            exit_writer.write(AppExit::Success);
            return;
        }

        // Request episode reset
        episode_control.reset_requested = true;

        // Reset test state counters
        test_state.episodes_completed += 1;
        test_state.total_reward = 0.0;
        test_state.ticks_since_last_action = 0;
        test_state.steps_this_episode = 0;
    }
}
