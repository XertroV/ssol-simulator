use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::{audio::PlayOrbPickupSound, game_state::{GameState, OrbPickedUp}, ui::in_game::OrbUiUpdateEvent};


// Constants ported from GameState.cs
pub const ORB_SPEED_INC: f32 = 0.05;
pub const ORB_DECEL_RATE: f32 = 0.6;
pub const ORB_SPEED_DUR: f32 = 2.0;
pub const FINAL_MAX_SPEED: f32 = 0.99;
pub const NORM_PERCENT_SPEED: f32 = 0.625;

pub fn orb_picked_up(_trigger: Trigger<OrbPickedUp>, mut commands: Commands, mut state: ResMut<GameState>) {
    state.score += 1;
    info!("Score: {}", state.score);
    // Reset the timer and increase the speed multiplier.
    state.orb_speed_boost_timer = ORB_SPEED_DUR;
    state.speed_multiplier += ORB_SPEED_INC;
    state.speed_multiplier = state.speed_multiplier.min(FINAL_MAX_SPEED);
    info!("Speed multiplier: {}", state.speed_multiplier);
    commands.trigger(PlayOrbPickupSound::from(state.as_ref()));
    commands.trigger(OrbUiUpdateEvent::Orbs(state.as_ref().into()));

    // returnGrowth functionality
    let power: f32 = 0.0 - state.t_step as f32 * 6.0 / state.score as f32;
    state.sol_target = state.start_speed_of_light - 1.0 / (1.0 + (power).exp()) * (state.start_speed_of_light - state.max_player_speed);
    if state.t_step >= state.score {
        state.sol_target = state.max_player_speed;
    }
    state.t_step += 1;
    state.sol_step = (state.speed_of_light - state.sol_target).abs() / 20.0;

    // Check if we have collected all orbs
    state.game_win = state.score >= state.nb_orbs;
}
