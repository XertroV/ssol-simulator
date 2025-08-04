use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::{audio::PlayOrbPickupSound, game_state::{GameState, OrbPickedUp}};


// Constants ported from GameState.cs
pub const ORB_SPEED_INC: f32 = 0.05;
pub const ORB_DECEL_RATE: f32 = 0.6;
pub const ORB_SPEED_DUR: f32 = 2.0;
pub const FINAL_MAX_SPEED: f32 = 0.99;
pub const NORM_PERCENT_SPEED: f32 = 0.9; // 0.625;

pub fn orb_picked_up(_trigger: Trigger<OrbPickedUp>, mut commands: Commands, mut state: ResMut<GameState>) {
    state.score += 1;
    info!("Score: {}", state.score);
    // Reset the timer and increase the speed multiplier.
    state.orb_speed_boost_timer = ORB_SPEED_DUR;
    state.speed_multiplier += ORB_SPEED_INC;
    state.speed_multiplier = state.speed_multiplier.min(FINAL_MAX_SPEED);
    info!("Speed multiplier: {}", state.speed_multiplier);
    commands.trigger(PlayOrbPickupSound::from(state.as_ref()));
}
